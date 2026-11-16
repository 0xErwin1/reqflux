//! Generate code (v1-features S2): the dialog opens from the request line, Ctrl+Shift+C and
//! the tree menu's event, shows the engine's code of the request as the view shows it with
//! every `{{variable}}` as written, switches language, copies and closes.

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::codegen::Language;
use tempfile::TempDir;

use super::GenerateCodeFor;
use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, edit_url, render, state_file};
use crate::shell::toasts::tests::{pushed, success, tree_label};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J000000000000000000000C1";
const CREATE: &str = "01J000000000000000000000C2";
const LIST: &str = "01J000000000000000000000C3";

/// A workspace whose collection `Billing` declares `base` and the secret `token`, holding
/// `POST Create invoice` at `{{base}}/v1/invoices` with `Authorization: Bearer {{token}}`
/// and a JSON body, and `GET List invoices`.
fn billing_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/billing/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"Billing\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"https://api.test\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/billing/create.req.toml",
        &format!(
            "id = \"{CREATE}\"\nname = \"Create invoice\"\nmethod = \"POST\"\n\
             url = \"{{{{base}}}}/v1/invoices\"\n\n\
             [[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{token}}}}\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"currency\": \"eur\"}}'\n"
        ),
    );
    write(
        root,
        "collections/billing/list.req.toml",
        &format!(
            "id = \"{LIST}\"\nname = \"List invoices\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/v1/invoices\"\n"
        ),
    );
    dir
}

/// The engine's code of the saved request `id` in `language`.
fn engine_code(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    id: &str,
    language: Language,
) -> String {
    cx.update(|_, cx| match &shell.read(cx).workspace {
        OpenedWorkspace::Loaded(engine) => engine
            .code(id, language)
            .unwrap_or_else(|error| panic!("code: {error}")),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => panic!("no workspace"),
    })
}

/// The text of the code editor of the open dialog.
fn editor_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).codegen_view().map(|view| view.text(cx)))
}

fn shown_code(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<(Language, String)> {
    cx.update(|_, cx| shell.read(cx).codegen_code())
}

fn open(cx: &mut TestAppContext) -> (TempDir, TempDir, Entity<Shell>, &mut VisualTestContext) {
    let dir = billing_workspace();
    let (state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{CREATE}"));
    (dir, state_dir, shell, cx)
}

#[gpui::test]
fn ctrl_shift_c_opens_the_curl_code_of_the_request_with_its_variables_as_written(
    cx: &mut TestAppContext,
) {
    let (_dir, _state, shell, cx) = open(cx);
    assert!(
        bounds(cx, "codegen-dialog").is_none(),
        "closed before the key"
    );

    cx.simulate_keystrokes("ctrl-shift-c");
    cx.run_until_parked();

    let expected = engine_code(cx, &shell, CREATE, Language::Curl);
    assert_eq!(
        shown_code(cx, &shell),
        Some((Language::Curl, expected.clone()))
    );
    assert!(expected.contains("{{base}}/v1/invoices"), "{expected}");
    assert!(expected.contains("Bearer {{token}}"), "{expected}");
    assert!(bounds(cx, "codegen-dialog").is_some());
    assert_eq!(
        editor_text(cx, &shell).as_deref(),
        Some(expected.as_str()),
        "the editor shows the code"
    );
    assert_eq!(
        tree_label(cx, "codegen-lines"),
        Some(format!("{} lines", expected.lines().count()))
    );
    assert_eq!(
        tree_label(cx, "codegen-request").as_deref(),
        Some("POST Create invoice")
    );
    assert_eq!(
        tree_label(cx, "codegen-secrets").as_deref(),
        Some(
            "Every {{variable}} is written as is: nothing is resolved. The value of token is \
             not in the code; set it before you run it."
        )
    );
}

#[gpui::test]
fn the_line_button_opens_the_dialog(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    click(cx, "codegen-open");
    let expected = engine_code(cx, &shell, CREATE, Language::Curl);
    assert_eq!(shown_code(cx, &shell), Some((Language::Curl, expected)));
    assert_eq!(
        tree_label(cx, "codegen-source").as_deref(),
        Some("Read only · from the saved request")
    );
}

#[gpui::test]
fn a_language_segment_shows_the_code_in_that_language(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    click(cx, "codegen-open");

    for language in [Language::Python, Language::Httpie, Language::Fetch] {
        click(cx, &format!("codegen-language-{language}"));
        let expected = engine_code(cx, &shell, CREATE, language);
        assert_eq!(shown_code(cx, &shell), Some((language, expected)));
    }
}

#[gpui::test]
fn copy_puts_the_code_on_the_clipboard_and_names_the_secrets_left(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    click(cx, "codegen-open");
    click(cx, "codegen-language-python");

    click(cx, "codegen-copy");

    let expected = engine_code(cx, &shell, CREATE, Language::Python);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied, Some(expected));
    assert_eq!(shown_code(cx, &shell), None, "Copy closes the dialog");
    assert!(bounds(cx, "codegen-dialog").is_none());
    assert_eq!(
        pushed(cx),
        vec![success(
            "Copied as Python requests",
            "Create invoice · 1 secret left as {{token}}"
        )]
    );
}

#[gpui::test]
fn the_code_follows_the_draft(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    edit_url(cx, "{{base}}/v2/drafts");

    click(cx, "codegen-open");

    let Some((_, code)) = shown_code(cx, &shell) else {
        panic!("the dialog is closed");
    };
    assert!(code.contains("{{base}}/v2/drafts"), "{code}");
    assert!(!code.contains("/v1/invoices"), "{code}");
    assert_eq!(
        tree_label(cx, "codegen-source").as_deref(),
        Some("Read only · from the current draft")
    );
}

#[gpui::test]
fn close_and_escape_close_without_copying(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    click(cx, "codegen-open");
    click(cx, "codegen-close");
    assert_eq!(shown_code(cx, &shell), None);

    click(cx, "codegen-open");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(shown_code(cx, &shell), None);
    assert!(bounds(cx, "codegen-dialog").is_none());
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard()), None);
    assert!(pushed(cx).is_empty(), "{:?}", pushed(cx));
}

#[gpui::test]
fn the_tree_menu_opens_another_request_without_switching_tabs(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    let sidebar = cx.update(|_, cx| shell.read(cx).sidebar.clone());

    sidebar.update(cx, |_, cx| cx.emit(GenerateCodeFor(LIST.to_owned())));
    cx.run_until_parked();

    let expected = engine_code(cx, &shell, LIST, Language::Curl);
    assert_eq!(shown_code(cx, &shell), Some((Language::Curl, expected)));
    assert_eq!(
        tree_label(cx, "codegen-request").as_deref(),
        Some("GET List invoices")
    );
    let active = cx.update(|_, cx| shell.read(cx).active_request().map(str::to_owned));
    assert_eq!(active.as_deref(), Some(CREATE));
}

#[gpui::test]
fn the_send_button_still_sends_beside_the_new_button(cx: &mut TestAppContext) {
    let (_dir, _state, _shell, cx) = open(cx);
    let (Some(code), Some(send)) = (bounds(cx, "codegen-open"), bounds(cx, "request-send")) else {
        panic!("the line misses a button");
    };
    assert!(
        code.right() <= send.left(),
        "Generate code sits before Send"
    );
    assert_eq!(code.center().y, send.center().y);
}

#[test]
fn each_language_colors_with_its_grammar() {
    assert_eq!(super::code_language(Language::Curl), "bash");
    assert_eq!(super::code_language(Language::Httpie), "bash");
    assert_eq!(super::code_language(Language::Python), "python");
    assert_eq!(super::code_language(Language::Fetch), "javascript");
}

#[gpui::test]
fn the_curl_command_takes_the_bash_roles(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    click(cx, "codegen-open");
    let Some((language, code)) = shown_code(cx, &shell) else {
        panic!("the dialog is closed");
    };
    let runs =
        cx.update(|_, cx| bolt::syntax::highlight(super::code_language(language), &code, cx));
    let color_at = |at: usize| {
        runs.iter()
            .find(|(range, _)| range.contains(&at))
            .and_then(|(_, style)| style.color)
            .map(crate::tokens::tests::rgb_of)
    };
    assert_eq!(color_at(0), Some(0xFFC23D), "the command: function");
    let Some(quote) = code.find('\'') else {
        panic!("no quoted argument: {code}");
    };
    assert_eq!(
        color_at(quote + 1),
        Some(0x7BE0A0),
        "a quoted argument: string"
    );
}

#[gpui::test]
fn unresolved_references_take_the_warning_tone(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx) = open(cx);
    edit_url(cx, "{{base}}/v1/{{missing}}");
    click(cx, "codegen-open");
    let Some((_, code)) = shown_code(cx, &shell) else {
        panic!("the dialog is closed");
    };
    let Some(references) = cx.update(|_, cx| shell.read(cx).codegen_references()) else {
        panic!("no references");
    };
    let text = |ranges: &[std::ops::Range<usize>]| -> Vec<String> {
        ranges
            .iter()
            .filter_map(|range| code.get(range.clone()))
            .map(str::to_owned)
            .collect()
    };
    // `token` is a secret with no value set, so it does not resolve either.
    assert_eq!(text(&references.unresolved), ["{{missing}}", "{{token}}"]);
    assert!(
        text(&references.resolved).contains(&"{{base}}".to_owned()),
        "{:?}",
        text(&references.resolved)
    );
    // The editor draws the references over its syntax colors: bold in the tint when they
    // resolve, in the warning color when they do not.
    let (marked, warning, tint) = cx.update(|_, cx| {
        let shell = shell.read(cx);
        let marks = shell
            .codegen_view()
            .map(|view| view.marks().to_vec())
            .unwrap_or_default();
        (
            marks,
            bolt::colors(cx).warning,
            bolt::brand::brand(cx).accent_tint,
        )
    });
    let color_of = |reference: &str| {
        marked
            .iter()
            .find(|mark| code.get(mark.range.clone()) == Some(reference))
            .and_then(|mark| mark.style.color)
    };
    assert_eq!(color_of("{{missing}}"), Some(warning));
    assert_eq!(color_of("{{token}}"), Some(warning));
    assert_eq!(color_of("{{base}}"), Some(tint));
}

/// The code the open dialog shows, with the editor focused by a click.
fn open_code<'a>(
    cx: &'a mut TestAppContext,
    language: &str,
) -> (
    TempDir,
    TempDir,
    Entity<Shell>,
    &'a mut VisualTestContext,
    String,
) {
    let (dir, state, shell, cx) = open(cx);
    click(cx, "codegen-open");
    click(cx, &format!("codegen-language-{language}"));
    let code = editor_text(cx, &shell).unwrap_or_default();
    click(cx, "codegen-code");
    (dir, state, shell, cx, code)
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui::test]
fn the_code_is_a_read_only_editor(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx, code) = open_code(cx, "python");
    assert!(code.contains("requests"), "{code}");

    cx.simulate_input("zz");
    // Not Enter: the dialog's confirm key copies the code and closes.
    for keys in ["backspace", "delete", "tab", "ctrl-x", "ctrl-z"] {
        cx.simulate_keystrokes(keys);
    }
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("replaced");
    cx.run_until_parked();

    assert_eq!(editor_text(cx, &shell), Some(code.clone()));
    assert_eq!(
        shown_code(cx, &shell),
        Some((Language::Python, code)),
        "nothing typed reaches the code the dialog copies"
    );
}

#[gpui::test]
fn the_selection_of_the_code_copies_with_ctrl_c(cx: &mut TestAppContext) {
    let (_dir, _state, shell, cx, code) = open_code(cx, "curl");

    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.run_until_parked();
    assert_eq!(clipboard(cx), Some(code.clone()), "select all");

    let Some(start) = code.find("invoices") else {
        panic!("no invoices in {code}");
    };
    let state = cx.update(|_, cx| {
        shell
            .read(cx)
            .codegen_view()
            .map(|view| view.state().clone())
    });
    let Some(state) = state else {
        panic!("the dialog has no editor");
    };
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_selected_range(start..start + 8, cx)
        });
    });
    cx.simulate_keystrokes("ctrl-c");
    cx.run_until_parked();
    assert_eq!(clipboard(cx).as_deref(), Some("invoices"));
    assert!(
        shown_code(&mut *cx, &shell).is_some(),
        "copying the selection keeps the dialog open"
    );
}
