//! The variable popover (T12, S10, design/variable-popover/01-inline-popover.html): a
//! `{{name}}` mark opens it after a 400 ms hover or on a click, and it edits the value of the
//! winning scope in place, or adds an undefined variable.

use std::time::Duration;

use gpui::{Entity, Modifiers, Point, TestAppContext, VisualTestContext, px};
use reqflux_engine::{SecretKey, SecretScope};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J000000000000000000000V1";
const REQUEST: &str = "01J000000000000000000000V2";
const STAGING: &str = "01J000000000000000000000V3";
const PRODUCTION: &str = "01J000000000000000000000V4";
const WORKSPACE: &str = "01J00000000000000000000900";

/// A workspace whose collection `API` declares `base`, whose environment Staging declares
/// `base` and the secret `token`, whose environment Production declares nothing, and whose request `GET Vars` goes to
/// `{{base}}/ok/{{token}}/{{tenant}}` with the header `X-Base: {{base}}`; `tenant` is
/// defined nowhere.
pub(super) fn popover_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://h\"\n"
        ),
    );
    write(
        root,
        "environments/staging.env.toml",
        &format!(
            "id = \"{STAGING}\"\nname = \"Staging\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"https://staging\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "environments/production.env.toml",
        &format!("id = \"{PRODUCTION}\"\nname = \"Production\"\n"),
    );
    write(
        root,
        "collections/api/vars.req.toml",
        &format!(
            "id = \"{REQUEST}\"\nname = \"Vars\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/ok/{{{{token}}}}/{{{{tenant}}}}\"\n\n\
             [[headers]]\nkey = \"X-Base\"\nvalue = \"{{{{base}}}}\"\n"
        ),
    );
    dir
}

/// Opens the request and, with `environment`, makes it active.
pub(super) fn open(cx: &mut VisualTestContext, shell: &Entity<Shell>, environment: Option<&str>) {
    click(cx, &format!("tree-row-{REQUEST}"));
    if let Some(slug) = environment {
        shell.update(cx, |shell, cx| {
            shell.set_environment(Some(slug.to_owned()), cx);
        });
        cx.run_until_parked();
    }
}

pub(super) fn popover_open(cx: &mut VisualTestContext) -> bool {
    bounds(cx, "variable-popover").is_some()
}

fn url_editing(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).url_edit.is_some())
}

pub(super) fn type_text(cx: &mut VisualTestContext, text: &str) {
    let typed = text
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

fn popover_value(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .variable_popover
            .as_ref()
            .map(|popover| popover.input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

fn current(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    scope: SecretScope,
    key: &str,
) -> Option<String> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let stored = shell
            .state
            .as_ref()
            .and_then(|state| state.current_values(WORKSPACE).ok())
            .and_then(|values| values.get(scope, key).map(str::to_owned));
        assert_eq!(
            stored.as_deref(),
            shell.current.get(scope, key),
            "the app state and the window disagree"
        );
        stored
    })
}

pub(super) fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(REQUEST)
            .map(|state| state.inspection.unresolved.clone())
            .unwrap_or_default()
    })
}

fn collection() -> SecretScope {
    SecretScope::Collection(id(COLLECTION))
}

fn staging() -> SecretScope {
    SecretScope::Environment(id(STAGING))
}

fn id(text: &str) -> reqflux_core::model::Id {
    match text.parse() {
        Ok(id) => id,
        Err(error) => panic!("id {text}: {error:?}"),
    }
}

fn secret(cx: &mut VisualTestContext, shell: &Entity<Shell>, key: &str) -> Option<String> {
    cx.update(|_, cx| match &shell.read(cx).workspace {
        OpenedWorkspace::Loaded(engine) => {
            let key = SecretKey {
                workspace_id: id(WORKSPACE),
                scope: staging(),
                key: key.to_owned(),
            };
            match engine.secret_store().get(&key) {
                Ok(value) => value,
                Err(error) => panic!("secret store: {error}"),
            }
        }
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => panic!("no workspace"),
    })
}

#[gpui::test]
fn hovering_a_mark_for_400_ms_opens_the_popover(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    let mark = present(cx, "request-url-var-1");
    cx.simulate_mouse_move(mark.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(!popover_open(cx), "the popover opened before 400 ms");

    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert!(popover_open(cx), "the popover did not open after 400 ms");
    assert!(!url_editing(cx, &shell));
}

#[gpui::test]
fn clicking_a_mark_opens_the_popover_instead_of_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    click(cx, "request-url-var-1");

    assert!(popover_open(cx));
    assert!(!url_editing(cx, &shell), "the click started the URL edit");
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn clicking_the_url_outside_a_mark_still_edits_it(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    let field = present(cx, "request-url");
    let outside = Point::new(field.right() - px(12.), field.center().y);
    cx.simulate_click(outside, Modifiers::none());
    cx.run_until_parked();

    assert!(url_editing(cx, &shell));
    assert!(!popover_open(cx));
}

#[gpui::test]
fn enter_saves_the_current_value_of_the_winning_scope(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    let before = std::fs::read(dir.path().join("environments/staging.env.toml")).ok();

    click(cx, "request-url-var-1");
    assert_eq!(popover_value(cx, &shell), "https://staging");
    cx.simulate_keystrokes("ctrl-a");
    type_text(cx, "http://x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    assert_eq!(
        current(cx, &shell, staging(), "base").as_deref(),
        Some("http://x")
    );
    assert_eq!(current(cx, &shell, collection(), "base"), None);
    let after = std::fs::read(dir.path().join("environments/staging.env.toml")).ok();
    assert_eq!(before, after, "the popover wrote the file");
    assert!(!unresolved(cx, &shell).contains(&"base".to_owned()));
}

#[gpui::test]
fn a_secret_stays_masked_and_saves_to_the_keyring(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    assert!(unresolved(cx, &shell).contains(&"token".to_owned()));

    click(cx, "request-url-var-2");
    assert!(popover_open(cx));
    assert!(bounds(cx, "variable-popover-reveal").is_some());
    type_text(cx, "s3cret");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(secret(cx, &shell, "token").as_deref(), Some("s3cret"));
    assert_eq!(current(cx, &shell, staging(), "token"), None);
    assert!(!unresolved(cx, &shell).contains(&"token".to_owned()));

    // Opened again, the stored value is not loaded until revealed.
    click(cx, "request-url-var-2");
    assert_eq!(popover_value(cx, &shell), "");
    click(cx, "variable-popover-reveal");
    assert_eq!(popover_value(cx, &shell), "s3cret");
}

#[gpui::test]
fn escape_closes_the_popover_without_writing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    click(cx, "request-url-var-1");
    cx.simulate_keystrokes("ctrl-a");
    type_text(cx, "zzz");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    assert_eq!(current(cx, &shell, collection(), "base"), None);
}

#[gpui::test]
fn f2_on_a_reference_opens_the_popover_while_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (shell_dir, state_path) = state_file();
    let _keep = shell_dir;
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));

    cx.simulate_keystrokes("home right right right f2");
    cx.run_until_parked();

    assert!(popover_open(cx));
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn alt_click_on_a_reference_opens_the_popover_while_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let mark = present(cx, "request-url-var-1");
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));

    cx.simulate_click(mark.center(), Modifiers::alt());
    cx.run_until_parked();

    assert!(popover_open(cx));
    assert!(url_editing(cx, &shell), "the URL edit closed");
}

#[gpui::test]
fn a_mark_in_a_grid_cell_opens_the_popover_instead_of_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    click(cx, "request-tab-headers");

    click(cx, "request-headers-value-1-var-1");

    assert!(popover_open(cx));
    assert!(
        cx.update(|_, cx| shell.read(cx).cell_edit.is_none()),
        "the click started the cell edit"
    );
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn typing_in_the_opened_popover_replaces_the_value(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-1");
    type_text(cx, "http://x");

    assert_eq!(popover_value(cx, &shell), "http://x");
}

#[gpui::test]
fn the_mark_keeps_its_own_label_while_its_popover_is_open(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-1");
    assert!(popover_open(cx));

    assert_eq!(
        label_of(cx, "request-url-var-1").as_deref(),
        Some("{{base}}")
    );
}

/// The id of the second request the anchor test opens, with one mark of its own.
const OTHER: &str = "01J000000000000000000000V5";

/// The rectangles the surface points at are the marks the frame draws: switching requests drops
/// the anchors of the marks that left the screen instead of keeping one per mark ever seen.
#[gpui::test]
fn the_anchors_keep_only_the_marks_the_frame_draws(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    write(
        dir.path(),
        "collections/api/other.req.toml",
        &format!(
            "id = \"{OTHER}\"\nname = \"Other\"\nmethod = \"GET\"\n\
             url = \"http://h/{{{{tenant}}}}\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    /// The anchors the last frame kept, sorted.
    fn anchors(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
        let mut ids = cx.update(|_, cx| shell.read(cx).variable_anchors.ids());
        ids.sort();
        ids
    }

    let before = anchors(cx, &shell);
    assert_eq!(
        before,
        [
            "request-headers-value-1-var-1",
            "request-url-var-1",
            "request-url-var-2",
            "request-url-var-3",
        ],
        "one anchor per mark the frame draws"
    );
    for id in &before {
        assert!(bounds(cx, id).is_some(), "{id} is still drawn");
    }

    click(cx, &format!("tree-row-{OTHER}"));
    assert_eq!(
        anchors(cx, &shell),
        ["request-url-var-1"],
        "the marks of the request it left are gone"
    );
}

/// The accessible label of element `id`, if it is shown.
pub(super) fn label_of(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    bounds(cx, id)?;
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

fn shell_focused(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|window, cx| shell.read(cx).focus.is_focused(window))
}

fn url_field_focused(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|window, cx| {
        shell
            .read(cx)
            .url_edit
            .as_ref()
            .is_some_and(|edit| gpui::Focusable::focus_handle(&edit.input, cx).is_focused(window))
    })
}

fn cell_editing(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).cell_edit.is_some())
}

/// Draws a frame: a focus change reaches the inputs' blur handlers in the next one, which
/// the real window draws by itself and the test window only when asked. The test window
/// starts inactive, and an inactive window reports no focus at all.
fn draw_frame(cx: &mut VisualTestContext) {
    // Focus events only reach the inputs of an active window.
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// A point of the request pane under its grid, away from every control.
fn empty_area(cx: &mut VisualTestContext) -> Point<gpui::Pixels> {
    let pane = present(cx, "request-pane");
    Point::new(pane.right() - px(30.), pane.bottom() - px(30.))
}

#[gpui::test]
fn clicking_outside_the_url_field_commits_it_and_the_marks_open_the_popover_again(
    cx: &mut TestAppContext,
) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));
    assert!(!shell_focused(cx, &shell), "the field has the focus");
    draw_frame(cx);

    let outside = empty_area(cx);
    cx.simulate_click(outside, Modifiers::none());
    draw_frame(cx);

    assert!(!url_editing(cx, &shell), "the click outside commits");
    assert!(shell_focused(cx, &shell), "the shell has the focus again");
    click(cx, "request-url-var-1");
    assert!(popover_open(cx), "the mark opens the popover again");
}

#[gpui::test]
fn the_url_field_commits_when_it_loses_focus(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));

    draw_frame(cx);
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    draw_frame(cx);

    assert!(!url_editing(cx, &shell));
}

#[gpui::test]
fn clicking_outside_a_cell_field_commits_what_was_typed(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    click(cx, "request-tab-headers");
    let cell = present(cx, "request-headers-value-1");
    cx.simulate_click(
        Point::new(cell.right() - px(8.), cell.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(cell_editing(cx, &shell));
    type_text(cx, "q");
    draw_frame(cx);

    let outside = empty_area(cx);
    cx.simulate_click(outside, Modifiers::none());
    draw_frame(cx);

    assert!(!cell_editing(cx, &shell));
    assert!(shell_focused(cx, &shell));
    let value = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(REQUEST)
            .and_then(|draft| draft.request.headers.first().map(|row| row.value.clone()))
    });
    assert_eq!(value.as_deref(), Some("{{base}}q"));
    click(cx, "request-headers-value-1-var-1");
    assert!(popover_open(cx), "the mark opens the popover again");
}

#[gpui::test]
fn clicking_inside_the_popover_does_not_commit_the_url_field(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let mark = present(cx, "request-url-var-1");
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_click(mark.center(), Modifiers::alt());
    cx.run_until_parked();
    assert!(popover_open(cx));

    click(cx, "variable-popover-input");
    draw_frame(cx);
    // The field's blur reaches the shell as an event; the popover holds the field open.
    cx.update(|_, cx| {
        let input = shell
            .read(cx)
            .url_edit
            .as_ref()
            .map(|edit| edit.input.clone());
        if let Some(input) = input {
            input.update(cx, |_, cx| cx.emit(gpui_component::input::InputEvent::Blur));
        }
    });
    draw_frame(cx);

    assert!(popover_open(cx));
    assert!(url_editing(cx, &shell), "the click inside is not outside");
}

#[gpui::test]
fn a_cell_blur_event_while_the_popover_is_open_does_not_commit_the_cell(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    cell_field_with_popover(cx, &shell);

    cx.update(|_, cx| {
        let input = shell
            .read(cx)
            .cell_edit
            .as_ref()
            .map(|edit| edit.input.clone());
        if let Some(input) = input {
            input.update(cx, |_, cx| cx.emit(gpui_component::input::InputEvent::Blur));
        }
    });
    draw_frame(cx);

    assert!(popover_open(cx));
    assert!(cell_editing(cx, &shell), "the popover holds the field open");
}

#[gpui::test]
fn clicking_an_empty_area_anywhere_in_the_shell_takes_the_focus(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    for (area, corner) in [
        ("request-pane", 30.),
        ("sidebar", 12.),
        ("status-bar", 4.),
        ("title-bar", 4.),
    ] {
        let Some(found) = bounds(cx, area) else {
            panic!("{area} is missing");
        };
        let field = present(cx, "request-url");
        cx.simulate_click(
            Point::new(field.right() - px(12.), field.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(url_editing(cx, &shell), "{area}: the field opens");
        draw_frame(cx);

        cx.simulate_click(
            Point::new(found.right() - px(corner), found.bottom() - px(corner)),
            Modifiers::none(),
        );
        draw_frame(cx);

        assert!(
            !url_editing(cx, &shell),
            "{area}: the click commits the field"
        );
        assert!(shell_focused(cx, &shell), "{area}: the shell has the focus");
    }
}

/// Opens the URL field and, with Alt+click on its first mark, the popover over it.
fn url_field_with_popover(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    open(cx, shell, None);
    let mark = present(cx, "request-url-var-1");
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_click(mark.center(), Modifiers::alt());
    cx.run_until_parked();
    assert!(popover_open(cx));
    assert!(url_editing(cx, shell));
    draw_frame(cx);
}

/// Opens the first header's value cell field and, with F2 on its reference, the popover.
fn cell_field_with_popover(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    open(cx, shell, None);
    click(cx, "request-tab-headers");
    let cell = present(cx, "request-headers-value-1");
    cx.simulate_click(
        Point::new(cell.right() - px(8.), cell.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_keystrokes("home right right right f2");
    cx.run_until_parked();
    assert!(popover_open(cx));
    assert!(cell_editing(cx, shell));
    draw_frame(cx);
}

fn focus_shell(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    draw_frame(cx);
}

#[gpui::test]
fn a_url_blur_during_the_popover_commits_the_field_when_it_closes(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    url_field_with_popover(cx, &shell);

    focus_shell(cx, &shell);
    assert!(url_editing(cx, &shell), "the popover holds the field open");
    shell.update_in(cx, |shell, window, cx| {
        shell.close_variable_popover(window, cx)
    });
    draw_frame(cx);

    assert!(!popover_open(cx));
    assert!(!url_editing(cx, &shell), "the blur was not lost");
    assert!(shell_focused(cx, &shell));
}

#[gpui::test]
fn a_cell_blur_during_the_popover_commits_the_field_when_it_closes(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    cell_field_with_popover(cx, &shell);

    focus_shell(cx, &shell);
    assert!(cell_editing(cx, &shell), "the popover holds the field open");
    shell.update_in(cx, |shell, window, cx| {
        shell.close_variable_popover(window, cx)
    });
    draw_frame(cx);

    assert!(!popover_open(cx));
    assert!(!cell_editing(cx, &shell), "the blur was not lost");
    assert!(shell_focused(cx, &shell));
}

#[gpui::test]
fn a_click_outside_the_open_popover_leaves_no_field_open(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    url_field_with_popover(cx, &shell);

    let outside = empty_area(cx);
    cx.simulate_click(outside, Modifiers::none());
    draw_frame(cx);
    draw_frame(cx);

    assert!(!popover_open(cx));
    assert!(!url_editing(cx, &shell), "the field is not stranded open");
    assert!(shell_focused(cx, &shell));
}

#[gpui::test]
fn escape_in_the_popover_gives_the_focus_back_to_the_field(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    url_field_with_popover(cx, &shell);

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    draw_frame(cx);

    assert!(!popover_open(cx));
    assert!(url_editing(cx, &shell), "Escape closes only the popover");
    assert!(url_field_focused(cx, &shell));
}

#[gpui::test]
fn the_url_field_closes_the_same_way_on_blur_and_on_enter(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    type_text(cx, "z");
    draw_frame(cx);

    focus_shell(cx, &shell);

    assert!(!url_editing(cx, &shell));
    let url = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(REQUEST)
            .map(|draft| draft.request.url.clone())
    });
    assert_eq!(url.as_deref(), Some("{{base}}/ok/{{token}}/{{tenant}}z"));
    assert!(shell_focused(cx, &shell));
}

#[gpui::test]
fn a_click_back_into_the_field_closes_the_popover_and_keeps_the_field(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    url_field_with_popover(cx, &shell);

    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    draw_frame(cx);
    draw_frame(cx);

    assert!(!popover_open(cx));
    assert!(url_editing(cx, &shell), "the press landed in the field");
    assert!(url_field_focused(cx, &shell));
}

#[gpui::test]
fn the_unresolved_label_counts_what_the_draft_would_send(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    super::draft_tests::edit_url(cx, "{{base}}/ok/{{token}}/{{tenant}}/{{missing}}");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();

    let sent = cx.update(|_, cx| {
        let shell = shell.read(cx);
        let (Some(draft), super::OpenedWorkspace::Loaded(engine)) =
            (shell.drafts.get(REQUEST), &shell.workspace)
        else {
            return None;
        };
        engine
            .prepare_draft(REQUEST, &draft.request, &Default::default())
            .ok()
            .map(|prepared| prepared.unresolved)
    });
    assert_eq!(
        sent.as_deref(),
        Some(&["token", "tenant", "missing"].map(String::from)[..])
    );
    assert_eq!(unresolved(cx, &shell), sent.unwrap_or_default());
    assert_eq!(
        label_of(cx, "request-unresolved").as_deref(),
        Some("3 unresolved variables")
    );
}

#[gpui::test]
fn typing_inspects_once_after_the_edits_settle(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let runs = |cx: &mut VisualTestContext| cx.update(|_, cx| shell.read(cx).reinspection_runs);
    let before = runs(cx);

    super::draft_tests::edit_url(cx, "{{base}}/ok/{{token}}/{{tenant}}/{{missing}}");
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(runs(cx), before, "the edits have not settled");
    assert_eq!(unresolved(cx, &shell), ["token", "tenant"]);

    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(runs(cx), before + 1, "one inspection for the whole edit");
    assert_eq!(unresolved(cx, &shell), ["token", "tenant", "missing"]);
    assert_eq!(
        label_of(cx, "request-unresolved").as_deref(),
        Some("3 unresolved variables")
    );
}

#[gpui::test]
fn the_pointer_leaving_the_mark_and_the_surface_closes_the_popover(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    let mark = present(cx, "request-url-var-1");
    cx.simulate_mouse_move(mark.center(), None, Modifiers::none());
    advance(cx, bolt::composites::HOVER_DELAY);
    assert!(popover_open(cx), "the hover opened the popover");

    // Over the surface it stays open, however long the pointer rests there.
    let surface = present(cx, "variable-popover-input");
    cx.simulate_mouse_move(surface.center(), None, Modifiers::none());
    advance(cx, bolt::composites::HOVER_GRACE * 4);
    assert!(popover_open(cx), "the pointer rests on the surface");

    // Off both, it closes once the grace is over.
    let away = empty_area(cx);
    cx.simulate_mouse_move(away, None, Modifiers::none());
    advance(cx, bolt::composites::HOVER_GRACE - Duration::from_millis(1));
    assert!(popover_open(cx), "the grace keeps it open for a moment");
    advance(cx, Duration::from_millis(1));
    draw_frame(cx);
    assert!(!popover_open(cx), "the pointer left both");
    assert!(!url_editing(cx, &shell), "no field is left open");
}

/// Moves the test clock forward by `duration` and draws what it wakes.
fn advance(cx: &mut VisualTestContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}
#[gpui::test]
fn a_click_in_the_value_field_edits_it_and_keeps_the_popover_open(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    click(cx, "request-url-var-1");

    click(cx, "variable-popover-input");
    cx.simulate_keystrokes("end");
    type_text(cx, "z");

    assert!(popover_open(cx), "a click in the field is not a click away");
    assert_eq!(popover_value(cx, &shell), "https://stagingz");
}
