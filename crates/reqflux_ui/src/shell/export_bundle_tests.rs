//! Export… as a ReqFlux bundle or API docs (v1-close S4, design/export/01-one-export-dialog):
//! secrets are left out by default and listed; encrypting asks the passphrase twice, refuses
//! an empty or mismatched one before the save dialog, and writes a bundle the engine's import
//! opens with it; API docs refuse to overwrite until Replace files.

use std::fs;

use gpui::TestAppContext;
use reqflux_engine::{BundleImport, BundleSecrets, MemorySecretStore, import_bundle};

use super::tests::{COLLECTION, fixture, read};
use crate::shell::draft_tests::{bounds, click, present};
use crate::shell::toasts::tests::{click_node, pushed, success, tree, warning};
use crate::workspace_tests::demo_workspace;

/// Types `text` into the field `id`.
fn type_into(cx: &mut gpui::VisualTestContext, id: &str, text: &str) {
    click(cx, id);
    cx.simulate_input(text);
    cx.run_until_parked();
}

/// The node `id` of `frame`, or a panic naming it.
fn node<'a>(frame: &'a gpui_mcp::UiTree, id: &str) -> &'a gpui_mcp::UiNode {
    frame
        .nodes
        .get(id)
        .unwrap_or_else(|| panic!("{id} is missing"))
}

#[gpui::test]
fn a_bundle_leaves_secrets_out_by_default_and_lists_them(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.save_to("billing.reqflux");
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    assert_eq!(
        f.label("option-group:export-secrets:omit").as_deref(),
        Some("Leave secrets out")
    );
    assert_eq!(
        f.label("export-omitted").as_deref(),
        Some("2 secrets will be empty in the bundle: access_token and client_secret.")
    );
    assert!(bounds(f.cx, "export-passphrase").is_none());
    assert_choice(f.cx, "omit");
    click(f.cx, "export-confirm");

    assert_eq!(
        f.host.suggested.borrow().as_slice(),
        ["billing-api.reqflux".to_owned()]
    );
    let expected = f.engine(|engine| {
        engine
            .bundle(Some(COLLECTION), BundleSecrets::Omit)
            .map(|bundle| bundle.text)
            .unwrap_or_else(|failure| panic!("bundle: {failure}"))
    });
    assert_eq!(read(&path), expected);
    assert_eq!(
        pushed(f.cx),
        vec![warning(
            "Bundle saved without secrets",
            &format!(
                "Billing API · 2 secrets left out\n{}\naccess_token, client_secret",
                path.display()
            )
        )]
    );
}

#[gpui::test]
fn an_empty_or_mismatched_passphrase_is_refused_before_the_save_dialog(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.save_to("billing.reqflux");
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    click(f.cx, "option-group:export-secrets:encrypt");
    present(f.cx, "export-passphrase");
    present(f.cx, "export-passphrase-confirm");

    click(f.cx, "export-confirm");
    assert_eq!(
        f.label("export-passphrase-error").as_deref(),
        Some("Enter a passphrase to encrypt the secrets.")
    );
    let frame = tree(f.cx);
    let caption = node(&frame, "export-passphrase-error");
    assert_eq!(
        caption.role,
        gpui_mcp::Role::Alert,
        "the field's error is the caption the input draws as an alert"
    );
    assert!(f.host.suggested.borrow().is_empty());

    type_into(f.cx, "export-passphrase", "correct horse");
    type_into(f.cx, "export-passphrase-confirm", "correct hors");
    click(f.cx, "export-confirm");
    assert!(bounds(f.cx, "export-passphrase-error").is_none());
    assert_eq!(
        f.label("export-passphrase-confirm-error").as_deref(),
        Some("Passphrases do not match.")
    );
    present(f.cx, "export-dialog");
    assert!(f.host.suggested.borrow().is_empty());
    assert!(pushed(f.cx).is_empty());
    assert_eq!(fs::read_dir(f.out.path()).map_or(0, Iterator::count), 0);
}

#[gpui::test]
fn an_encrypted_bundle_opens_with_its_passphrase(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.store_secret("access_token", "tok-123");
    let path = f.save_to("billing.reqflux");
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    click(f.cx, "option-group:export-secrets:encrypt");
    type_into(f.cx, "export-passphrase", "correct horse");
    type_into(f.cx, "export-passphrase-confirm", "correct horse");
    click(f.cx, "export-confirm");

    let text = read(&path);
    assert!(!text.contains("tok-123"), "the value is encrypted");
    let target = demo_workspace();
    let store = MemorySecretStore::new();
    let options = BundleImport {
        into: None,
        passphrase: Some("correct horse"),
    };
    match import_bundle(target.path(), &text, &options, &store) {
        Ok(summary) => assert_eq!(summary.secrets_imported, 1),
        Err(failure) => panic!("import: {failure}"),
    }
    let toasts = pushed(f.cx);
    assert_eq!(
        toasts,
        vec![warning(
            "Bundle saved, secrets encrypted",
            &format!(
                "Billing API · 1 secret sealed with your passphrase\n{}\nclient_secret",
                path.display()
            )
        )]
    );
    assert!(
        toasts
            .iter()
            .all(|toast| !toast.text.contains("correct horse"))
    );
}

#[gpui::test]
fn api_docs_refuse_to_overwrite_until_replace_files(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let docs = f.out.path().join("docs");
    if let Err(failure) = fs::create_dir_all(&docs) {
        panic!("create: {failure}");
    }
    if let Err(failure) = fs::write(docs.join("README.md"), "keep") {
        panic!("write: {failure}");
    }
    *f.host.folder.borrow_mut() = Some(docs.clone());
    f.export_row(COLLECTION);
    click(f.cx, "export-format-docs");
    assert_eq!(f.label("export-confirm").as_deref(), Some("Choose folder…"));
    click(f.cx, "export-confirm");

    present(f.cx, "export-replace-dialog");
    assert_eq!(read(&docs.join("README.md")), "keep");
    assert!(!docs.join("index.html").exists());

    click(f.cx, "export-replace-confirm");
    assert!(bounds(f.cx, "export-replace-dialog").is_none());
    let expected = f.engine(|engine| {
        engine
            .docs(COLLECTION)
            .unwrap_or_else(|failure| panic!("docs: {failure}"))
    });
    for file in &expected.files {
        assert_eq!(read(&docs.join(&file.name)), file.content);
    }
    assert_eq!(
        pushed(f.cx),
        vec![success(
            "API docs written",
            &format!("README.md and index.html\n{}", docs.display())
        )]
    );
    click_node(f.cx, "toast-action-1-0");
    assert_eq!(f.host.opened.borrow().as_slice(), [docs.join("index.html")]);
}

/// Asserts the bundle secrets group has `id` chosen, through the automation tree.
fn assert_choice(cx: &mut gpui::VisualTestContext, id: &str) {
    let frame = tree(cx);
    for option in ["omit", "encrypt"] {
        let node = node(&frame, &format!("option-group:export-secrets:{option}"));
        assert_eq!(
            node.state.checked,
            Some(option == id),
            "{option} is chosen only when the group reports {id}"
        );
    }
}

/// The bundle secrets choice is one radio group: two options, each named and described, and no
/// checkbox standing in for a radio.
#[gpui::test]
fn the_secrets_choice_is_one_radio_group_of_two_options(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");

    let frame = tree(f.cx);
    let group = node(&frame, "option-group:export-secrets");
    assert_eq!(group.role, gpui_mcp::Role::Group);
    assert_eq!(group.label.as_deref(), Some("Secrets in a bundle"));

    let omit = node(&frame, "option-group:export-secrets:omit");
    assert_eq!(omit.role, gpui_mcp::Role::Radio);
    assert_eq!(omit.label.as_deref(), Some("Leave secrets out"));
    assert_eq!(omit.state.checked, Some(true));
    assert_eq!(
        omit.description.as_deref(),
        Some("The bundle lists them by name, empty. Whoever imports it fills them in.")
    );

    let encrypt = node(&frame, "option-group:export-secrets:encrypt");
    assert_eq!(encrypt.role, gpui_mcp::Role::Radio);
    assert_eq!(
        encrypt.label.as_deref(),
        Some("Encrypt secrets with a passphrase")
    );
    assert_eq!(encrypt.state.checked, Some(false));

    assert!(
        bounds(f.cx, "export-secrets-omit").is_none()
            && bounds(f.cx, "export-secrets-encrypt").is_none(),
        "the two checkboxes that acted as one are gone"
    );
    present(f.cx, "option-group:export-secrets:omit:description");
    present(f.cx, "option-group:export-secrets:encrypt:description");
}

/// Each option reports the choice it lands on, and the keyboard moves it: the group is one
/// focusable stop, so the arrow key that chose Encrypt takes it back to Leave secrets out.
#[gpui::test]
fn the_secrets_choice_reports_the_option_it_lands_on(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");

    click(f.cx, "option-group:export-secrets:encrypt");
    present(f.cx, "export-passphrase");
    assert!(bounds(f.cx, "export-omitted").is_none());
    assert_choice(f.cx, "encrypt");

    f.cx.simulate_keystrokes("up");
    f.cx.run_until_parked();
    assert!(bounds(f.cx, "export-passphrase").is_none());
    present(f.cx, "export-omitted");
    assert_choice(f.cx, "omit");
}

/// The window's automation tree as text, read after a fresh frame.
fn automation_dump(cx: &mut gpui::VisualTestContext) -> String {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    format!("{:?}", automation.snapshot())
}

#[gpui::test]
fn the_passphrase_never_reaches_the_automation_tree_even_revealed(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    click(f.cx, "option-group:export-secrets:encrypt");
    type_into(f.cx, "export-passphrase", "zebra-pass-77");
    type_into(f.cx, "export-passphrase-confirm", "zebra-pass-77");
    assert!(!automation_dump(f.cx).contains("zebra-pass-77"));
    click(f.cx, "export-passphrase-reveal");
    assert!(
        !automation_dump(f.cx).contains("zebra-pass-77"),
        "a revealed passphrase stays out of the automation tree"
    );
}

#[gpui::test]
fn cancel_drops_the_passphrase(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    click(f.cx, "option-group:export-secrets:encrypt");
    type_into(f.cx, "export-passphrase", "zebra-pass-77");
    click(f.cx, "export-cancel");
    f.export_row(COLLECTION);
    click(f.cx, "export-format-bundle");
    click(f.cx, "option-group:export-secrets:encrypt");
    let shell = f.shell.clone();
    let held = f.cx.update(|_, cx| {
        shell
            .read(cx)
            .export
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.passphrase.as_ref())
            .map(|[first, second]| {
                (
                    first.read(cx).value().to_string(),
                    second.read(cx).value().to_string(),
                )
            })
    });
    assert_eq!(held, Some((String::new(), String::new())));
}
