//! The run document model: rows, header texts and the current values a run persists.

use reqflux_engine::{
    CurrentValues, RunEnd, RunEvent, RunItem, RunSummary, SecretScope, StepResult, TestResult,
};

use super::{
    Change, OrderLine, Phase, RowStatus, RunModel, TargetKind, Tone, cells, changes,
    unresolved_text,
};
use crate::shell::tree_model::{Node, NodeKind};

fn node(id: &str, name: &str, kind: NodeKind, children: Vec<Node>) -> Node {
    Node {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
        url: String::new(),
        children,
    }
}

fn request(id: &str, name: &str, method: &str) -> Node {
    node(
        id,
        name,
        NodeKind::Request {
            method: method.to_owned(),
        },
        Vec::new(),
    )
}

/// `Billing API` with `invoices` (List, Create) and `customers` (Get).
fn roots() -> Vec<Node> {
    vec![node(
        "c",
        "Billing API",
        NodeKind::Collection,
        vec![
            node(
                "f1",
                "invoices",
                NodeKind::Folder,
                vec![
                    request("r1", "List invoices", "GET"),
                    request("r2", "Create invoice", "POST"),
                ],
            ),
            node(
                "f2",
                "customers",
                NodeKind::Folder,
                vec![request("r3", "Get customer", "GET")],
            ),
        ],
    )]
}

fn item(id: &str, name: &str) -> RunItem {
    RunItem {
        id: id.to_owned(),
        name: name.to_owned(),
        path: format!("billing/{id}"),
    }
}

fn items() -> Vec<RunItem> {
    vec![
        item("r1", "List invoices"),
        item("r2", "Create invoice"),
        item("r3", "Get customer"),
    ]
}

fn model() -> RunModel {
    match RunModel::new(
        &roots(),
        "c",
        &items(),
        Some(("staging".to_owned(), "Staging".to_owned())),
    ) {
        Some(model) => model,
        None => panic!("the collection has a run"),
    }
}

fn test(name: &str, passed: bool) -> TestResult {
    TestResult {
        name: name.to_owned(),
        passed,
        message: (!passed).then(|| "expected 201 but got 422".to_owned()),
    }
}

fn result(step: usize, id: &str, status: Option<u16>, tests: Vec<TestResult>) -> StepResult {
    StepResult {
        step,
        item: item(id, id),
        method: Some("GET".to_owned()),
        url: Some("http://localhost/".to_owned()),
        status,
        reason: status.map(|_| "OK".to_owned()),
        elapsed_ms: 142,
        size: 3482,
        timings: None,
        tests,
        error: status.is_none().then(|| "connection refused".to_owned()),
        unresolved: Vec::new(),
    }
}

fn summary(results: Vec<StepResult>, end: RunEnd) -> RunSummary {
    RunSummary {
        items: items(),
        results,
        end,
        elapsed_ms: 1400,
        current: CurrentValues::new(),
    }
}

fn statuses(model: &RunModel) -> Vec<(String, RowStatus)> {
    model
        .rows()
        .into_iter()
        .map(|row| (row.plan.name, row.status))
        .collect()
}

#[test]
fn the_order_heads_each_folder_and_leaves_root_requests_unheaded() {
    let folder = |index: usize, name: &str, count: usize| OrderLine::Folder {
        index,
        name: name.to_owned(),
        count,
    };
    assert_eq!(
        model().order(),
        [
            folder(0, "invoices", 2),
            OrderLine::Request(0),
            OrderLine::Request(1),
            folder(2, "customers", 1),
            OrderLine::Request(2),
        ],
        "folders keep their header and count"
    );

    let mut roots = roots();
    if let Some(collection) = roots.first_mut() {
        collection
            .children
            .insert(0, request("r0", "Health", "GET"));
        collection.children.push(request("r4", "Slow", "GET"));
    }
    let mut items = items();
    items.insert(0, item("r0", "Health"));
    items.push(item("r4", "Slow"));
    let Some(model) = RunModel::new(&roots, "c", &items, None) else {
        panic!("the collection has a run");
    };
    assert_eq!(
        model.order(),
        [
            OrderLine::Request(0),
            folder(1, "invoices", 2),
            OrderLine::Request(1),
            OrderLine::Request(2),
            folder(3, "customers", 1),
            OrderLine::Request(3),
            OrderLine::Request(4),
        ]
    );
    assert_eq!(
        model.plan.first().map(|row| row.folder.as_str()),
        Some("Billing API"),
        "the grid still names the collection of a root request"
    );
}

#[test]
fn the_unresolved_text_names_every_variable_sent_literally() {
    let mut step = result(0, "r1", Some(200), Vec::new());
    assert_eq!(unresolved_text(&step), None);
    step.unresolved = vec!["secret".to_owned(), "who".to_owned()];
    assert_eq!(
        unresolved_text(&step).as_deref(),
        Some("Unresolved: {{secret}}, {{who}}")
    );
}

#[test]
fn the_setup_names_the_target_and_plans_every_request_with_its_folder() {
    let model = model();
    assert_eq!(model.kind, TargetKind::Collection);
    assert_eq!(model.phase, Phase::Setup);
    assert_eq!(model.title(), "Run Billing API");
    assert_eq!(model.badge(), None);
    let plan: Vec<_> = model
        .plan
        .iter()
        .map(|row| (row.method.as_str(), row.name.as_str(), row.folder.as_str()))
        .collect();
    assert_eq!(
        plan,
        [
            ("GET", "List invoices", "invoices"),
            ("POST", "Create invoice", "invoices"),
            ("GET", "Get customer", "customers"),
        ]
    );
    assert!(
        RunModel::new(&roots(), "r1", &items(), None).is_none(),
        "a request is not a run target"
    );
}

#[test]
fn a_live_run_marks_the_request_in_flight_and_queues_the_rest() {
    let mut model = model();
    model.start();
    model.apply(RunEvent::Started {
        step: 0,
        item: item("r1", "List invoices"),
    });
    model.apply(RunEvent::Finished(result(
        0,
        "r1",
        Some(200),
        vec![test("ok", true)],
    )));
    model.apply(RunEvent::Started {
        step: 1,
        item: item("r2", "Create invoice"),
    });
    assert_eq!(
        statuses(&model),
        [
            ("List invoices".to_owned(), RowStatus::Done(0)),
            ("Create invoice".to_owned(), RowStatus::Sending),
            ("Get customer".to_owned(), RowStatus::Queued),
        ]
    );
    assert_eq!(model.title(), "Running Billing API");
    assert_eq!(model.badge(), Some(("RUNNING", Tone::Accent)));
    let [requests, passed, failed, _] = model.totals();
    assert_eq!(requests.0, "1 of 3");
    assert_eq!(passed.0, "1 test passed");
    assert_eq!(failed, ("0 failed".to_owned(), Tone::Muted));
}

#[test]
fn a_finished_run_shows_its_verdict_and_every_step() {
    let mut model = model();
    model.start();
    let results = vec![
        result(0, "r1", Some(200), vec![test("a", true)]),
        result(1, "r2", Some(200), vec![test("b", true), test("c", true)]),
        result(2, "r3", Some(200), Vec::new()),
    ];
    model.finish(Ok(&summary(results, RunEnd::Completed)));
    assert_eq!(model.title(), "Billing API passed");
    assert_eq!(model.badge(), Some(("PASSED", Tone::Success)));
    assert_eq!(model.subline(), ["Staging", "continues after a failure"]);
    let [requests, passed, failed, time] = model.totals();
    assert_eq!(requests.0, "3 of 3 requests");
    assert_eq!(passed.0, "3 tests passed");
    assert_eq!(failed.0, "0 failed");
    assert_eq!(time.0, "1.4 s");
    let rows = model.rows();
    assert_eq!(rows.len(), 3);
    let texts: Vec<String> = rows
        .first()
        .map(|row| {
            cells(row, &model)
                .into_iter()
                .map(|(text, _)| text)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        texts,
        [
            "GET",
            "List invoices",
            "invoices",
            "200 OK",
            "142 ms",
            "3.4 KB",
            "1 passed"
        ]
    );
}

#[test]
fn stop_on_first_failure_skips_the_rest_and_says_where_it_stopped() {
    let mut model = model();
    model.bail = true;
    model.start();
    model.apply(RunEvent::Started {
        step: 0,
        item: item("r1", "List invoices"),
    });
    let failing = result(
        0,
        "r1",
        Some(422),
        vec![test("status is 201", false), test("x", true)],
    );
    model.finish(Ok(&summary(vec![failing], RunEnd::Bailed)));
    assert_eq!(model.title(), "Billing API failed");
    assert_eq!(model.badge(), Some(("STOPPED", Tone::Danger)));
    assert_eq!(
        model.subline(),
        [
            "Staging",
            "stopped on first failure at request 1",
            "2 skipped"
        ]
    );
    assert_eq!(
        statuses(&model),
        [
            ("List invoices".to_owned(), RowStatus::Done(0)),
            ("Create invoice".to_owned(), RowStatus::Skipped),
            ("Get customer".to_owned(), RowStatus::Skipped),
        ]
    );
    let row = model.rows().into_iter().next();
    let tests = row.map(|row| cells(&row, &model)[6].clone());
    assert_eq!(
        tests,
        Some(("1 failed \u{b7} 1 passed".to_owned(), Tone::Danger))
    );
    let [_, _, failed, _] = model.totals();
    assert_eq!(failed, ("1 failed".to_owned(), Tone::Danger));
}

#[test]
fn a_request_without_a_response_reads_failed_and_fails_the_run() {
    let mut model = model();
    model.start();
    let results = vec![result(0, "r1", None, Vec::new())];
    model.finish(Ok(&summary(results, RunEnd::Completed)));
    let row = model.rows().into_iter().next();
    let status = row.map(|row| cells(&row, &model)[3].clone());
    assert_eq!(status, Some(("Failed".to_owned(), Tone::Danger)));
    assert_eq!(model.title(), "Billing API failed");
}

#[test]
fn stop_cancels_and_a_run_that_did_not_start_says_why() {
    let mut model = model();
    model.start();
    model.finish(Ok(&summary(Vec::new(), RunEnd::Cancelled)));
    assert_eq!(model.title(), "Billing API stopped");
    assert_eq!(model.badge(), Some(("CANCELLED", Tone::Warning)));

    let mut model = self::model();
    model.start();
    model.finish(Err("unknown environment staging".to_owned()));
    assert_eq!(model.title(), "Billing API did not run");
    assert_eq!(model.badge(), Some(("FAILED", Tone::Danger)));
    assert!(
        model
            .subline()
            .contains(&"unknown environment staging".to_owned())
    );
}

#[test]
fn run_again_clears_the_last_run() {
    let mut model = model();
    model.start();
    model.finish(Ok(&summary(
        vec![result(0, "r1", Some(200), Vec::new())],
        RunEnd::Completed,
    )));
    model.selected = Some(0);
    model.start();
    assert_eq!(model.phase, Phase::Running);
    assert!(model.results.is_empty());
    assert_eq!(model.selected, None);
    assert!(
        statuses(&model)
            .iter()
            .all(|(_, status)| *status == RowStatus::Queued)
    );
}

#[test]
fn the_values_a_run_changed_are_set_or_cleared() {
    let mut started = CurrentValues::new();
    started.insert(SecretScope::Global, "kept", "1");
    started.insert(SecretScope::Global, "changed", "old");
    started.insert(SecretScope::Global, "cleared", "x");
    let mut ended = CurrentValues::new();
    ended.insert(SecretScope::Global, "kept", "1");
    ended.insert(SecretScope::Global, "changed", "new");
    ended.insert(SecretScope::Global, "added", "in_1");
    assert_eq!(
        changes(&started, &ended),
        [
            Change {
                scope: SecretScope::Global,
                key: "added".to_owned(),
                value: Some("in_1".to_owned()),
            },
            Change {
                scope: SecretScope::Global,
                key: "changed".to_owned(),
                value: Some("new".to_owned()),
            },
            Change {
                scope: SecretScope::Global,
                key: "cleared".to_owned(),
                value: None,
            },
        ]
    );
    assert!(changes(&started, &started).is_empty());
}
