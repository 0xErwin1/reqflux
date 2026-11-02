//! `reqflux run`: send one request and print the response (S11) and the script results
//! (S9), or run a collection or folder (`crate::run_collection`).

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use reqflux_engine::{Engine, ExecuteError, ExecuteOptions, Response, ScriptOutcome};

use crate::cli::RunArgs;

/// Why `reqflux run` stopped, with its exit code.
#[derive(Debug)]
enum Failure {
    /// Exit code 2: the arguments, the request or environment name, or the workspace.
    Usage(String),
    /// Exit code 1: the request could not be executed.
    Execution(String),
}

impl From<ExecuteError> for Failure {
    fn from(error: ExecuteError) -> Self {
        if error.is_usage() {
            Self::Usage(error.to_string())
        } else {
            Self::Execution(error.to_string())
        }
    }
}

/// Runs `reqflux run` and returns its exit code.
pub(crate) fn run(args: RunArgs) -> ExitCode {
    // Load warnings are printed below in their documented format, not through the logger.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    match send(args) {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Execution(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Applies the user's `[network]` settings (v1-features S5) to `engine`. A config file that
/// cannot be read leaves the defaults and says so; the cookie jar lives for this run only.
fn apply_network(engine: &Engine) {
    let loaded =
        reqflux_engine::config::default_path().and_then(|path| reqflux_engine::config::load(&path));
    match loaded {
        Ok(user) => {
            for warning in engine.apply_network(&user.network) {
                eprintln!("warning: {warning}");
            }
        }
        Err(error) => eprintln!("warning: network settings not applied: {error}"),
    }
}

fn send(args: RunArgs) -> Result<ExitCode, Failure> {
    let vars = args
        .vars
        .iter()
        .map(|var| parse_var(var))
        .collect::<Result<Vec<_>, _>>()?;
    let root = args.workspace.unwrap_or_else(|| PathBuf::from("."));
    let engine = Engine::open(root).map_err(|error| Failure::Usage(error.to_string()))?;
    for warning in engine.warnings() {
        eprintln!("warning: {warning}");
    }
    apply_network(&engine);

    let options = ExecuteOptions {
        environment: args.env,
        vars,
        // `reqflux run` never reads or writes current values (variables-editor S5).
        ..ExecuteOptions::default()
    };
    if !engine.has_request(&args.target) {
        if args.include {
            return Err(Failure::Usage(
                "--include applies to a single request".to_owned(),
            ));
        }
        return Ok(crate::run_collection::run(
            &engine,
            &args.target,
            options,
            args.bail,
            args.json,
        )?);
    }
    if args.json || args.bail {
        return Err(Failure::Usage(
            "--json and --bail apply to a collection or folder run".to_owned(),
        ));
    }
    let execution = engine.execute(&args.target, &options)?;
    if let Some(problem) = &execution.secret_problem {
        eprintln!("warning: {problem}");
    }
    for name in &execution.unresolved {
        eprintln!("warning: unresolved variable {{{{{name}}}}}");
    }
    let (method, url) = (execution.method.clone(), execution.url.clone());
    // The pre-request has already run when the transport fails; its results live on the
    // execution, so keep them to print before the error line.
    let scripts = execution.scripts.clone();
    let executed = match engine.wait(execution) {
        Ok(executed) => executed,
        Err(error) => {
            write_scripts(&scripts);
            return Err(error.into());
        }
    };
    let response = &executed.response;

    write_scripts(&executed.scripts);
    write_response(response, args.include)
        .map_err(|error| Failure::Execution(format!("could not write the response: {error}")))?;
    eprintln!(
        "{method} {url} -> {} in {} ms, {}",
        status(response),
        response.elapsed.as_millis(),
        size(response.size())
    );
    Ok(ExitCode::SUCCESS)
}

/// Prints the script results in their documented shape, pre-request entries first: one
/// `test` line per recorded test and one `console` line per console entry. A failing test
/// does not change the exit code. On a transport failure the pre-request results are
/// printed before the error line, so they are never lost.
fn write_scripts(scripts: &ScriptOutcome) {
    for test in &scripts.tests {
        match (&test.passed, &test.message) {
            (true, _) => eprintln!("test pass {}", test.name),
            (false, Some(message)) => eprintln!("test fail {}: {message}", test.name),
            (false, None) => eprintln!("test fail {}", test.name),
        }
    }
    for entry in &scripts.console {
        eprintln!("console {}: {}", entry.level.as_str(), entry.message);
    }
}

/// Splits `<key>=<value>` at the first `=`.
fn parse_var(var: &str) -> Result<(String, String), Failure> {
    var.split_once('=')
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .ok_or_else(|| Failure::Usage(format!("invalid --var {var}: expected <key>=<value>")))
}

/// Writes the body bytes unchanged, after the status line and headers with `include`.
fn write_response(response: &Response, include: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if include {
        writeln!(stdout, "HTTP/{} {}", response.version, status(response))?;
        for (name, value) in &response.headers {
            writeln!(stdout, "{name}: {value}")?;
        }
        writeln!(stdout)?;
    }
    stdout.write_all(&response.body)?;
    stdout.flush()
}

/// `<code> <reason>`, or the code alone when it has no reason phrase.
fn status(response: &Response) -> String {
    match &response.reason {
        Some(reason) => format!("{} {reason}", response.status),
        None => response.status.to_string(),
    }
}

/// `<n> B`, `<n.n> KB` or `<n.n> MB`, 1024 based.
pub(crate) fn size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} B")
    } else if value < KB * KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{:.1} MB", value / (KB * KB))
    }
}

#[cfg(test)]
mod tests {
    use super::size;

    #[test]
    fn sizes_are_1024_based() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(1023), "1023 B");
        assert_eq!(size(1024), "1.0 KB");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(1024 * 1024), "1.0 MB");
        assert_eq!(size(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    }
}
