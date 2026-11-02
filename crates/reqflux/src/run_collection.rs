//! `reqflux run <collection|folder>`: runs every request of a collection or folder through
//! the engine's collection runner (v1-features S1) and prints a plain or JSON report.

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqflux_engine::{
    CancellationToken, Engine, ExecuteOptions, RunEnd, RunEvent, RunOptions, RunSummary,
    StepResult, Timings,
};
use serde_json::{Value, json};

use crate::run::size;

/// How often the Ctrl-C watcher checks for a pending signal.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs `target` and prints the report: one line per request as it finishes, then the
/// summary; with `json`, one JSON document at the end. Either way each step's unresolved
/// variables go to stderr as it finishes, one `warning: unresolved variable {{name}}` line
/// each, as a single send warns. Exit code 0 when the run passed, 1
/// otherwise. `reqflux run` never touches the app state, so the run records no history.
pub(crate) fn run(
    engine: &Engine,
    target: &str,
    options: ExecuteOptions,
    bail: bool,
    as_json: bool,
) -> Result<ExitCode, reqflux_engine::ExecuteError> {
    let options = RunOptions {
        execute: options,
        bail,
        max_steps: None,
    };
    let cancel = cancel_on_interrupt();
    let mut stdout = io::stdout().lock();
    let mut write_failed = false;
    let summary = engine.run_collection(target, &options, &cancel, None, &mut |event| {
        let RunEvent::Finished(result) = event else {
            return;
        };
        for name in &result.unresolved {
            eprintln!("warning: unresolved variable {{{{{name}}}}}");
        }
        if !as_json && let Err(error) = write_step(&mut stdout, result) {
            write_failed = true;
            log::error!("could not write the report: {error}");
        }
    })?;
    let written = if as_json {
        writeln!(stdout, "{}", report(target, &summary))
    } else {
        writeln!(stdout, "{}", summary_line(&summary))
    };
    if let Err(error) = written.and_then(|()| stdout.flush()) {
        eprintln!("error: could not write the report: {error}");
        return Ok(ExitCode::FAILURE);
    }
    Ok(if summary.passed() && !write_failed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// A token Ctrl-C (SIGINT) or SIGTERM cancels: the send in flight is cancelled, the report
/// is still printed. A handler that cannot be registered keeps the signal's default.
fn cancel_on_interrupt() -> CancellationToken {
    let cancel = CancellationToken::new();
    let requested = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if let Err(error) = signal_hook::flag::register(signal, Arc::clone(&requested)) {
            log::warn!("could not handle signal {signal}: {error}");
        }
    }
    let token = cancel.clone();
    let spawned = std::thread::Builder::new()
        .name("reqflux-run-signals".into())
        .spawn(move || {
            while !requested.load(Ordering::Relaxed) {
                std::thread::sleep(POLL_INTERVAL);
            }
            token.cancel();
        });
    if let Err(error) = spawned {
        log::warn!("could not watch for Ctrl-C: {error}");
    }
    cancel
}

/// `<METHOD> <name> -> <code> <reason> in <ms> ms, <size>, tests <passed>/<total>`, or
/// `<METHOD> <name> -> error: <message>`, then one `  fail <test>: <message>` line per
/// failing test.
fn write_step(out: &mut impl Write, result: &StepResult) -> io::Result<()> {
    let method = result.method.as_deref().unwrap_or("-");
    let name = &result.item.name;
    match (&result.error, result.status) {
        (Some(error), _) => writeln!(out, "{method} {name} -> error: {error}")?,
        (None, status) => {
            let status = status.map_or_else(String::new, |code| match &result.reason {
                Some(reason) => format!("{code} {reason}"),
                None => code.to_string(),
            });
            let passed = result.tests.iter().filter(|test| test.passed).count();
            writeln!(
                out,
                "{method} {name} -> {status} in {} ms, {}, tests {passed}/{}",
                result.elapsed_ms,
                size(usize::try_from(result.size).unwrap_or(usize::MAX)),
                result.tests.len()
            )?;
        }
    }
    for test in result.tests.iter().filter(|test| !test.passed) {
        match &test.message {
            Some(message) => writeln!(out, "  fail {}: {message}", test.name)?,
            None => writeln!(out, "  fail {}", test.name)?,
        }
    }
    Ok(())
}

/// `<n> requests, <n> failed; tests <n> passed, <n> failed; <ms> ms`, then why the run
/// ended early, if it did.
fn summary_line(summary: &RunSummary) -> String {
    let mut line = format!(
        "{} requests, {} failed; tests {} passed, {} failed; {} ms",
        summary.requests(),
        summary.failed_requests(),
        summary.tests_passed(),
        summary.tests_failed(),
        summary.elapsed_ms
    );
    let end = match &summary.end {
        RunEnd::Completed => None,
        RunEnd::Stopped => Some("stopped by pm.setNextRequest(null)".to_owned()),
        RunEnd::Bailed => Some("stopped at the first failure (--bail)".to_owned()),
        RunEnd::Cancelled => Some("cancelled".to_owned()),
        RunEnd::StepLimit(limit) => Some(format!("stopped after {limit} requests (step limit)")),
        RunEnd::UnknownNext(next) => Some(format!(
            "pm.setNextRequest named no request of the run: {next}"
        )),
    };
    if let Some(end) = end {
        line.push_str("; ");
        line.push_str(&end);
    }
    line
}

/// The phases of a send in the `--json` report, one object per hop, in milliseconds; a
/// phase that was not measured is `null`.
fn timings(timings: &Timings) -> Value {
    let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
    timings
        .hops
        .iter()
        .map(|hop| {
            json!({
                "dns_ms": hop.dns.map(ms),
                "connect_ms": hop.connect.map(ms),
                "tls_ms": hop.tls.map(ms),
                "first_byte_ms": ms(hop.first_byte),
                "download_ms": ms(hop.download),
            })
        })
        .collect()
}

/// The `--json` report.
fn report(target: &str, summary: &RunSummary) -> Value {
    let (end, next) = match &summary.end {
        RunEnd::Completed => ("completed", None),
        RunEnd::Stopped => ("stopped", None),
        RunEnd::Bailed => ("bailed", None),
        RunEnd::Cancelled => ("cancelled", None),
        RunEnd::StepLimit(_) => ("step_limit", None),
        RunEnd::UnknownNext(next) => ("unknown_next", Some(next.as_str())),
    };
    let results: Vec<Value> = summary
        .results
        .iter()
        .map(|result| {
            let tests: Vec<Value> = result
                .tests
                .iter()
                .map(|test| {
                    json!({ "name": test.name, "passed": test.passed, "message": test.message })
                })
                .collect();
            json!({
                "step": result.step,
                "id": result.item.id,
                "name": result.item.name,
                "path": result.item.path,
                "method": result.method,
                "url": result.url,
                "status": result.status,
                "reason": result.reason,
                "elapsed_ms": result.elapsed_ms,
                "size": result.size,
                "timings": result.timings.as_ref().map(timings),
                "tests": tests,
                "error": result.error,
                "unresolved": result.unresolved,
            })
        })
        .collect();
    let mut report = json!({
        "target": target,
        "passed": summary.passed(),
        "end": end,
        "requests": summary.requests(),
        "failed_requests": summary.failed_requests(),
        "tests_passed": summary.tests_passed(),
        "tests_failed": summary.tests_failed(),
        "elapsed_ms": summary.elapsed_ms,
        "results": results,
    });
    if let (Some(next), Some(object)) = (next, report.as_object_mut()) {
        object.insert("next".to_owned(), Value::from(next));
    }
    report
}
