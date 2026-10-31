//! Independent probes for the security surface of the sandbox (S1, S3, S4, S8).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "probes fail the calling test"
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use reqflux_script::{
    RequestView, ResponseView, ScriptContext, ScriptError, ScriptErrorKind, ScriptStage, run,
};

fn context(stage: ScriptStage) -> ScriptContext {
    ScriptContext {
        stage,
        request: RequestView {
            method: "POST".into(),
            url: "https://api.example.com/users?limit=1".into(),
            headers: BTreeMap::from([("authorization".into(), "Bearer token".into())]),
            body: Some(r#"{"name":"ada"}"#.into()),
        },
        variables: BTreeMap::from([("token".into(), "secret".into())]),
        response: (stage == ScriptStage::Test).then(|| ResponseView {
            code: 201,
            status: "Created".into(),
            response_time_ms: 120,
            response_size: 13,
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body_text: r#"{"ok":true}"#.into(),
            parsed_json: Some(serde_json::json!({ "ok": true })),
        }),
        send_request: None,
    }
}

fn bytes_saved(error: &ScriptError) -> usize {
    error
        .outcome
        .console
        .iter()
        .map(|entry| entry.message.len())
        .sum::<usize>()
        + error
            .outcome
            .tests
            .iter()
            .map(|test| test.name.len())
            .sum::<usize>()
        + error
            .outcome
            .writes
            .iter()
            .map(|write| write.value.as_deref().unwrap_or("").len())
            .sum::<usize>()
}

#[test]
fn interrupt_cannot_be_caught() {
    let started = Instant::now();
    let error = run(
        "while (true) { try { while (true) {} } catch (e) {} }",
        &context(ScriptStage::Test),
    )
    .unwrap_err();
    assert_eq!(error.kind, ScriptErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(12),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn deadline_holds_under_large_allocations() {
    let started = Instant::now();
    let error = run(
        "while (true) { console.log('x'.repeat(1000000)); }",
        &context(ScriptStage::Test),
    )
    .unwrap_err();
    assert!(
        matches!(
            error.kind,
            ScriptErrorKind::Timeout | ScriptErrorKind::MemoryLimit
        ),
        "{error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn host_collected_state_is_bounded() {
    let error = run(
        "while (true) { pm.environment.set('a', 'x'.repeat(200000)); }",
        &context(ScriptStage::Test),
    )
    .unwrap_err();
    assert_eq!(error.kind, ScriptErrorKind::MemoryLimit, "{error}");
    assert!(
        bytes_saved(&error) < 64 * 1024 * 1024,
        "{} bytes kept",
        bytes_saved(&error)
    );
}

#[test]
fn memory_limit_covers_array_buffers() {
    for script in [
        "new ArrayBuffer(134217728);",
        "new Uint8Array(134217728);",
        "new SharedArrayBuffer(134217728);",
    ] {
        let error = run(script, &context(ScriptStage::Test)).unwrap_err();
        assert_eq!(
            error.kind,
            ScriptErrorKind::MemoryLimit,
            "{script}: {error}"
        );
    }
}

/// Each step adds a 1 MiB buffer, so a container reaches the heap cap in a few dozen
/// cheap allocations: the run ends on the memory limit long before the wall-clock time
/// limit, even on a heavily loaded machine. Many small elements took about 0.8 s in a
/// debug build, close enough to the 5 s time limit that load turned it into a Timeout.
#[test]
fn growth_oom_reports_the_memory_limit() {
    for script in [
        "const a = []; for (;;) { a.push(new ArrayBuffer(1 << 20)); }",
        "const m = new Map(); for (;;) { m.set(m.size, new ArrayBuffer(1 << 20)); }",
    ] {
        let error = run(script, &context(ScriptStage::Test)).unwrap_err();
        assert_eq!(
            error.kind,
            ScriptErrorKind::MemoryLimit,
            "{script}: {error}"
        );
    }
}

#[test]
fn thrown_errors_are_not_reclassified_from_text() {
    for script in [
        "throw new Error('interrupted');",
        "throw new Error('out of memory');",
        "JSON.parse('nope');",
    ] {
        let error = run(script, &context(ScriptStage::Test)).unwrap_err();
        assert_eq!(error.kind, ScriptErrorKind::Thrown, "{script}: {error}");
    }
    assert_eq!(
        run("if ( {", &context(ScriptStage::PreRequest))
            .unwrap_err()
            .kind,
        ScriptErrorKind::Syntax
    );
}

#[test]
fn timeout_keeps_the_outcome_collected_so_far() {
    let error = run(
        "pm.environment.set('a', '1'); pm.test('t', () => {}); console.log('c'); while (true) {}",
        &context(ScriptStage::Test),
    )
    .unwrap_err();
    assert_eq!(error.kind, ScriptErrorKind::Timeout);
    assert_eq!(error.stage, ScriptStage::Test);
    assert_eq!(
        (
            error.outcome.tests.len(),
            error.outcome.console.len(),
            error.outcome.writes.len()
        ),
        (1, 1, 1)
    );
}

#[test]
fn runs_do_not_share_globals() {
    let shared = context(ScriptStage::Test);
    run("globalThis.leak = 1; Object.prototype.evil = 1;", &shared).unwrap();
    let outcome = run(
        "console.log('leak=' + typeof globalThis.leak + ' evil=' + [].evil);",
        &shared,
    )
    .unwrap();
    assert_eq!(outcome.console[0].message, "leak=undefined evil=undefined");
}

#[test]
fn init_data_cannot_inject_code() {
    let mut hostile_context = context(ScriptStage::Test);
    let hostile = "a\"); throw new Error('INJECTED'); //\\\n\u{2028}\u{2029}\u{0}\u{7f}\u{feff}";
    hostile_context.request.body = Some(hostile.into());
    hostile_context
        .request
        .headers
        .insert("x-hostile".into(), hostile.into());
    let outcome = run(
        "console.log(pm.request.body.raw); console.log(pm.request.headers.get('X-Hostile'));",
        &hostile_context,
    )
    .unwrap();
    assert_eq!(outcome.console[0].message, hostile);
    assert_eq!(outcome.console[1].message, hostile);
}

#[test]
fn no_host_globals_are_reachable() {
    let outcome = run(
        r#"
        // `require` is part of the documented surface and only serves the bundled modules.
        const names = ["process","std","os","fetch","XMLHttpRequest","Deno","Buffer",
                       "load","print","readFile","open","env","module","exports"];
        const seen = names.filter((n) => typeof globalThis[n] !== "undefined");
        const viaFunction = Function("return typeof process")();
        let unknown = "served";
        try { require("os"); } catch (error) { unknown = "refused"; }
        console.log(seen.join(",") + "|" + viaFunction + "|" + typeof __reqfluxInit + "|" + unknown);
        "#,
        &context(ScriptStage::Test),
    )
    .unwrap();
    assert_eq!(outcome.console[0].message, "|undefined|undefined|refused");
}

#[test]
fn views_cannot_change_the_host_context() {
    let shared = context(ScriptStage::Test);
    run(
        r#"pm.request.url = "https://evil.example.com/"; pm.request.method = "DELETE";
        pm.request.headers.get = () => "spoof"; pm.response.code = 999;
        pm.response.headers.get = () => "spoof";"#,
        &shared,
    )
    .unwrap();
    assert_eq!(shared.request.url, "https://api.example.com/users?limit=1");
    assert_eq!(shared.request.method, "POST");
    assert_eq!(shared.request.headers["authorization"], "Bearer token");
    assert_eq!(
        shared.response.as_ref().map(|response| response.code),
        Some(201)
    );
}

#[test]
fn nested_tests_and_console_do_not_deadlock() {
    let outcome = run(
        r#"let depth = 0;
        function go() { depth += 1; if (depth < 200) { pm.test("t" + depth, go); } }
        go();
        pm.test("tricky", () => { console.log(String({ toString() { return "t"; } })); });"#,
        &context(ScriptStage::Test),
    )
    .unwrap();
    assert_eq!(outcome.tests.len(), 200);
    assert!(outcome.tests.iter().all(|test| test.passed));
}
