//! The `{{name}}` marks stay drawn while a field is edited (T11, S9): the URL field and a
//! grid cell field style each reference as the read-only text does, and follow typing.

use std::ops::Range;

use bolt::controls::TextDecoration;
use gpui::{Entity, Modifiers, Point, TestAppContext, VisualTestContext, px};
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{click, present, render, state_file};
use crate::shell::request::spans;
use crate::workspace_tests::{demo_workspace, write};

const REQUEST: &str = "01J000000000000000000000M2";
const BODY: &str = "01J000000000000000000000M3";

/// A workspace whose collection declares `base` and whose request `GET Marks` goes to
/// `{{base}}/ok/{{tenant}}` with the header `X-Base: v {{base}} {{nope}}`; `tenant` and
/// `nope` are defined nowhere.
fn marks_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        "id = \"01J000000000000000000000M1\"\nname = \"API\"\n\n\
         [[variables]]\nkey = \"base\"\nvalue = \"http://h\"\n",
    );
    write(
        root,
        "collections/api/marks.req.toml",
        &format!(
            "id = \"{REQUEST}\"\nname = \"Marks\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/ok/{{{{tenant}}}}\"\n\n\
             [[headers]]\nkey = \"X-Base\"\nvalue = \"v {{{{base}}}} {{{{nope}}}}\"\n"
        ),
    );
    dir
}

/// Clicks near the right edge of `selector`, away from its marks.
fn click_edge(cx: &mut VisualTestContext, selector: &str) {
    let field = present(cx, selector);
    cx.simulate_click(
        Point::new(field.right() - px(8.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    let typed = text
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

/// The expected marks: each range resolving or not.
fn expected(cx: &mut VisualTestContext, marks: &[(Range<usize>, bool)]) -> Vec<TextDecoration> {
    cx.update(|_, cx| {
        marks
            .iter()
            .map(|(range, resolves)| {
                TextDecoration::new(range.clone(), spans::mark_style(*resolves, cx))
            })
            .collect()
    })
}

fn url_marks(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Vec<TextDecoration>> {
    cx.update(|_, cx| {
        let edit = shell.read(cx).url_edit.as_ref()?;
        Some(edit.marks.as_ref()?.shown(cx))
    })
}

fn cell_marks(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Vec<TextDecoration>> {
    cx.update(|_, cx| {
        let edit = shell.read(cx).cell_edit.as_ref()?;
        Some(edit.marks.as_ref()?.shown(cx))
    })
}

#[gpui::test]
fn the_url_field_keeps_its_marks_while_edited(cx: &mut TestAppContext) {
    let dir = marks_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{REQUEST}"));
    click_edge(cx, "request-url");
    assert!(cx.update(|_, cx| shell.read(cx).url_edit.is_some()));

    let opened = expected(cx, &[(0..8, true), (12..22, false)]);
    assert_eq!(url_marks(cx, &shell), Some(opened));

    cx.simulate_keystrokes("end");
    type_text(cx, "/{{base}}/{{y}}");
    let typed = expected(
        cx,
        &[
            (0..8, true),
            (12..22, false),
            (23..31, true),
            (32..37, false),
        ],
    );
    assert_eq!(url_marks(cx, &shell), Some(typed));

    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let unbraced = expected(cx, &[(0..8, true), (12..22, false), (23..31, true)]);
    assert_eq!(url_marks(cx, &shell), Some(unbraced));
}

#[gpui::test]
fn a_grid_cell_field_keeps_its_marks_while_edited(cx: &mut TestAppContext) {
    let dir = marks_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{REQUEST}"));
    click(cx, "request-tab-headers");
    click_edge(cx, "request-headers-value-1");
    assert!(cx.update(|_, cx| shell.read(cx).cell_edit.is_some()));

    let opened = expected(cx, &[(2..10, true), (11..19, false)]);
    assert_eq!(cell_marks(cx, &shell), Some(opened));

    cx.simulate_keystrokes("end space");
    type_text(cx, "{{base}}");
    let typed = expected(cx, &[(2..10, true), (11..19, false), (20..28, true)]);
    assert_eq!(cell_marks(cx, &shell), Some(typed));

    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let unbraced = expected(cx, &[(2..10, true), (11..19, false)]);
    assert_eq!(cell_marks(cx, &shell), Some(unbraced));
}

#[gpui::test]
fn adding_a_variable_from_the_open_field_marks_it_resolved(cx: &mut TestAppContext) {
    let dir = marks_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{REQUEST}"));
    click_edge(cx, "request-url");
    let keys = format!("home{}", " right".repeat(14));
    cx.simulate_keystrokes(&format!("{keys} f2"));
    cx.run_until_parked();

    // The quick add starts on the collection, API.
    click(cx, "variable-popover-input");
    type_text(cx, "acme");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(cx.update(|_, cx| shell.read(cx).url_edit.is_some()));
    let resolved = expected(cx, &[(0..8, true), (12..22, true)]);
    assert_eq!(url_marks(cx, &shell), Some(resolved));
}

#[gpui::test]
fn an_unresolved_reference_is_marked_with_the_danger_role(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::theme::install(crate::theme::PaletteMode::Dark, cx);
        let danger = bolt::colors(cx).danger;
        let style = spans::mark_style(false, cx);
        assert_eq!(style.color, Some(danger));
        assert_eq!(
            style.background_color,
            Some(crate::tokens::variable_wash(danger))
        );
        assert_eq!(
            style.underline.and_then(|underline| underline.color),
            Some(danger)
        );
    });
}

/// A workspace whose collection declares `base` and whose request `POST Body` has the raw
/// JSON body `{"a": {{base}}, "b": "{{nope}}"}`: an unquoted reference and a quoted one.
fn body_marks_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        "id = \"01J000000000000000000000M1\"\nname = \"API\"\n\n\
         [[variables]]\nkey = \"base\"\nvalue = \"1\"\n",
    );
    write(
        root,
        "collections/api/body.req.toml",
        &format!(
            "id = \"{BODY}\"\nname = \"Body\"\nmethod = \"POST\"\nurl = \"https://h/b\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\n\
             text = '''{{\"a\": {{{{base}}}}, \"b\": \"{{{{nope}}}}\"}}'''\n"
        ),
    );
    dir
}

fn body_marks(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Vec<TextDecoration>> {
    cx.update(|_, cx| {
        let edit = shell.read(cx).body_edit.as_ref()?;
        Some(edit.marks.shown(cx))
    })
}

#[gpui::test]
fn the_raw_body_editor_marks_its_references_quoted_or_not(cx: &mut TestAppContext) {
    let dir = body_marks_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{BODY}"));
    click(cx, "request-tab-body");
    click(cx, "request-body-code");
    assert!(cx.update(|_, cx| shell.read(cx).body_edit.is_some()));

    let opened = expected(cx, &[(6..14, true), (22..30, false)]);
    assert_eq!(body_marks(cx, &shell), Some(opened));

    cx.simulate_keystrokes("ctrl-end");
    type_text(cx, "{{base}}");
    let typed = expected(cx, &[(6..14, true), (22..30, false), (32..40, true)]);
    assert_eq!(body_marks(cx, &shell), Some(typed));

    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let unbraced = expected(cx, &[(6..14, true), (22..30, false)]);
    assert_eq!(body_marks(cx, &shell), Some(unbraced));
}
