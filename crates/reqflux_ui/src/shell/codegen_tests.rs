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
        tree_label(cx, "codegen-line-1").as_deref(),
        expected.lines().next()
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
