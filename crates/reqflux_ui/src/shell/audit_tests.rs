//! The audit view (app-pending S2): opened from the rail and the palette, the log newest first,
//! filters, an expanded row, the empty state and a log that does not read or parse.

use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::approvals_tests::label;
use super::commands::{CommandId, command};
use super::draft_tests::{bounds, click, present, render, state_file};
use crate::workspace_tests::demo_workspace;

const WORKSPACE: &str = "01J00000000000000000000900";
/// The triggers of the Class and Outcome multi-selects, which show their summaries.
const CLASS_TRIGGER: &str = "audit-class.ms-trigger-wrap.ms-trigger";
const OUTCOME_TRIGGER: &str = "audit-outcome.ms-trigger-wrap.ms-trigger";

/// An execute a minute before now and a refused delete two days before.
fn lines() -> String {
    let now = super::history::now_millis();
    let at = |ago: i64| time(now - ago);
    format!(
        concat!(
            r#"{{"time":"{old}","tool":"item_delete","class":"destructive","decision":"refused","reason":"destructive calls are denied","workspace_id":"{ws}","targets":["01A"]}}"#,
            "\n",
            r#"{{"time":"{new}","tool":"request_execute","class":"execute","decision":"allowed","workspace_id":"{ws}","targets":["01R"],"method":"POST","url":"https://api.test/v1?key=*****"}}"#,
            "\n"
        ),
        old = at(2 * 86_400_000),
        new = at(60_000),
        ws = WORKSPACE,
    )
}

/// `millis` as the audit log writes it.
fn time(millis: i64) -> String {
    match chrono::DateTime::from_timestamp_millis(millis) {
        Some(time) => time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        None => panic!("time out of range"),
    }
}

struct Fixture<'a> {
    shell: Entity<Shell>,
    cx: &'a mut VisualTestContext,
    automation: gpui_mcp::Automation,
    path: PathBuf,
    _dirs: Vec<TempDir>,
}

/// A shell on the demo workspace whose audit log is `contents` (none when `None`) in a
/// temporary directory.
fn fixture<'a>(cx: &'a mut TestAppContext, contents: Option<&str>) -> Fixture<'a> {
    let dir = demo_workspace();
    let (state_dir, state_path) = state_file();
    let log_dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = log_dir.path().join("audit.log");
    if let Some(contents) = contents
        && let Err(error) = std::fs::write(&path, contents)
    {
        panic!("write log: {error}");
    }
    let (shell, cx) = render(cx, &dir, &state_path);
    set_path(cx, &shell, &path);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    Fixture {
        shell,
        cx,
        automation,
        path,
        _dirs: vec![dir, state_dir, log_dir],
    }
}

fn set_path(cx: &mut VisualTestContext, shell: &Entity<Shell>, path: &Path) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| shell.audit.path = Some(path.to_path_buf()));
    });
}

impl Fixture<'_> {
    fn label(&self, id: &str) -> Option<String> {
        label(&self.automation, id)
    }

    fn labels(&self) -> Vec<String> {
        self.automation
            .snapshot()
            .nodes
            .values()
            .filter_map(|node| node.label.clone())
            .collect()
    }
}

#[gpui::test]
fn the_rail_opens_the_log_newest_first(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    assert!(bounds(f.cx, "audit-view").is_none());
    click(f.cx, "rail-audit");
    present(f.cx, "audit-view");
    assert_eq!(f.label("audit-title").as_deref(), Some("Audit log"));
    assert_eq!(
        f.label("audit-row-1-tool").as_deref(),
        Some("request_execute")
    );
    assert_eq!(f.label("audit-row-1-class").as_deref(), Some("Execute"));
    assert_eq!(
        f.label("audit-row-1-target").as_deref(),
        Some("POST https://api.test/v1?key=*****")
    );
    assert_eq!(f.label("audit-row-1-workspace").as_deref(), Some("Demo"));
    assert_eq!(f.label("audit-row-1-outcome").as_deref(), Some("Allowed"));
    // The refusal two days ago is outside the default 24 h.
    assert!(bounds(f.cx, "audit-row-2").is_none());
    assert_eq!(f.label("audit-count").as_deref(), Some("1 event in 24h"));
    assert_eq!(
        f.label("audit-path").as_deref(),
        Some(f.path.display().to_string().as_str())
    );
}

#[gpui::test]
fn the_view_has_its_active_tab_and_its_close_goes_back(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    assert!(bounds(f.cx, "doc-tab-audit").is_none());
    click(f.cx, "rail-audit");
    assert_eq!(f.label("doc-tab-audit").as_deref(), Some("Audit log"));
    assert!(bounds(f.cx, "doc-tab-approvals").is_none());
    click(f.cx, "doc-tab-audit-close");
    assert!(bounds(f.cx, "audit-view").is_none());
    assert!(bounds(f.cx, "doc-tab-audit").is_none());
    present(f.cx, "document");
}

#[gpui::test]
fn the_palette_command_opens_the_view(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    assert_eq!(
        command(CommandId::OpenAudit).map(|command| (command.name, command.label)),
        Some(("open-audit", "Open audit log"))
    );
    let shell = f.shell.clone();
    f.cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.run_command(CommandId::OpenAudit, window, cx);
        });
    });
    f.cx.run_until_parked();
    present(f.cx, "audit-view");
    present(f.cx, "audit-row-1");
}

#[gpui::test]
fn the_range_class_and_outcome_filter_the_rows(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    click(f.cx, "rail-audit");
    click(f.cx, "audit-range-7d");
    assert_eq!(f.label("audit-row-2-tool").as_deref(), Some("item_delete"));
    assert_eq!(f.label("audit-row-2-outcome").as_deref(), Some("Refused"));
    assert_eq!(f.label("audit-count").as_deref(), Some("2 events in 7d"));

    choose(f.cx, &f.shell, |controls| &controls.outcome, &["refused"]);
    assert_eq!(f.label("audit-row-1-tool").as_deref(), Some("item_delete"));
    assert!(bounds(f.cx, "audit-row-2").is_none());

    choose(f.cx, &f.shell, |controls| &controls.class, &["execute"]);
    present(f.cx, "audit-no-match");

    click(f.cx, "audit-clear");
    assert_eq!(
        f.label("audit-row-1-tool").as_deref(),
        Some("request_execute")
    );
    assert!(bounds(f.cx, "audit-row-2").is_none(), "back to 24h");
    assert_eq!(f.label(CLASS_TRIGGER).as_deref(), Some("Class · all"));
}

#[gpui::test]
fn several_classes_and_outcomes_can_be_selected(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    click(f.cx, "rail-audit");
    click(f.cx, "audit-range-7d");
    assert_eq!(f.label(CLASS_TRIGGER).as_deref(), Some("Class · all"));
    assert_eq!(f.label(OUTCOME_TRIGGER).as_deref(), Some("Outcome · all"));

    choose(
        f.cx,
        &f.shell,
        |controls| &controls.class,
        &["execute", "destructive"],
    );
    assert_eq!(f.label(CLASS_TRIGGER).as_deref(), Some("Class · 2"));
    assert_eq!(
        f.label("audit-row-1-tool").as_deref(),
        Some("request_execute")
    );
    assert_eq!(f.label("audit-row-2-tool").as_deref(), Some("item_delete"));
    present(f.cx, "audit-clear");

    choose(
        f.cx,
        &f.shell,
        |controls| &controls.outcome,
        &["allowed", "refused"],
    );
    assert_eq!(f.label(OUTCOME_TRIGGER).as_deref(), Some("Outcome · 2"));
    present(f.cx, "audit-row-2");

    choose(f.cx, &f.shell, |controls| &controls.class, &["write"]);
    present(f.cx, "audit-no-match");

    choose(f.cx, &f.shell, |controls| &controls.class, &[]);
    assert_eq!(f.label(CLASS_TRIGGER).as_deref(), Some("Class · all"));
    present(f.cx, "audit-row-2");
}

/// Selects `values` in the multi-select `which` picks, emitting the change it emits on a click.
fn choose(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    which: impl Fn(&super::audit::Controls) -> &Entity<bolt::controls::MultiSelect>,
    values: &[&str],
) {
    let select = cx.update(|_, cx| {
        shell
            .read(cx)
            .audit
            .controls
            .as_ref()
            .map(|controls| which(controls).clone())
    });
    let Some(select) = select else {
        panic!("the audit view has no controls");
    };
    let values: Vec<String> = values.iter().map(|value| (*value).to_owned()).collect();
    cx.update(|_, cx| {
        select.update(cx, |select, cx| {
            select.set_selected_values(&values, cx);
            cx.emit(bolt::controls::MultiSelectChanged {
                selected_values: select.selected_values(),
            });
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_row_expands_into_its_detail(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    click(f.cx, "rail-audit");
    click(f.cx, "audit-range-7d");
    assert!(bounds(f.cx, "audit-detail-2").is_none());
    click(f.cx, "audit-row-2");
    present(f.cx, "audit-detail-2");
    assert_eq!(
        f.label("audit-detail-2-reason").as_deref(),
        Some("destructive calls are denied")
    );
    assert_eq!(f.label("audit-detail-2-targets").as_deref(), Some("01A"));
    click(f.cx, "audit-row-2");
    assert!(
        bounds(f.cx, "audit-detail-2").is_none(),
        "a second click folds it"
    );
}

#[gpui::test]
fn a_bad_line_is_reported_and_never_shown(cx: &mut TestAppContext) {
    let contents = format!("{}secret-looking garbage {{\n", lines());
    let f = fixture(cx, Some(&contents));
    click(f.cx, "rail-audit");
    assert_eq!(
        f.label("audit-skipped").as_deref(),
        Some("1 line of the audit log could not be read and is not shown: line 3")
    );
    present(f.cx, "audit-row-1");
    assert!(
        f.labels().iter().all(|text| !text.contains("garbage")),
        "the raw line never shows"
    );
}

#[gpui::test]
fn an_empty_log_shows_the_empty_state(cx: &mut TestAppContext) {
    let f = fixture(cx, None);
    click(f.cx, "rail-audit");
    assert_eq!(
        f.label("audit-empty").as_deref(),
        Some("No agent call recorded yet")
    );
    assert!(bounds(f.cx, "audit-row-1").is_none());
}

#[gpui::test]
fn a_log_that_cannot_be_read_is_reported(cx: &mut TestAppContext) {
    let f = fixture(cx, None);
    let dir = f.path.with_file_name("");
    set_path(f.cx, &f.shell, &dir);
    click(f.cx, "rail-audit");
    assert_eq!(
        f.label("audit-error").as_deref(),
        Some("The audit log could not be read")
    );
}

#[gpui::test]
fn refresh_reads_the_log_again(cx: &mut TestAppContext) {
    let f = fixture(cx, None);
    click(f.cx, "rail-audit");
    present(f.cx, "audit-empty");
    if let Err(error) = std::fs::write(&f.path, lines()) {
        panic!("write log: {error}");
    }
    click(f.cx, "audit-refresh");
    present(f.cx, "audit-row-1");
}

#[gpui::test]
fn the_other_rail_views_still_open_from_the_audit_view(cx: &mut TestAppContext) {
    let f = fixture(cx, Some(&lines()));
    click(f.cx, "rail-audit");
    click(f.cx, "rail-approvals");
    present(f.cx, "approvals-view");
    assert!(bounds(f.cx, "audit-view").is_none());
    click(f.cx, "rail-audit");
    click(f.cx, "rail-collections");
    assert!(bounds(f.cx, "audit-view").is_none());
    present(f.cx, "document");
}
