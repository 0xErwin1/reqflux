//! The Export… dialog, `export-dialog`, and the Replace API docs question,
//! `export-replace-dialog`, as design/export/01-one-export-dialog draws them: the format
//! control `export-format` with a segment `export-format-<postman|bundle|docs>` per format the
//! scope takes, then the options of the format, then Cancel (`export-cancel`) and the primary
//! button (`export-confirm`) whose label names the next step.

use bolt::controls::{Button, Checkbox, Input, InputState};
use bolt::modals::{Modal, modal_field};
use bolt::primitives::{BannerBlock, BannerVariant, SegmentedControl, SegmentedItem, TextVariant};
use bolt::tokens::{ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, Entity, IntoElement, ParentElement, Point, Role, SharedString,
    Styled, Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::model::{Format, omitted_warning, plaintext_warning, secrets_off_note, secrets_on_note};
use super::{Dialog, ReplaceDocs};
use crate::shell::request::{segmented, ui};
use crate::shell::{Shell, region, text_region};
use crate::tokens;

/// The dialog or the Replace question, while one is open, over the window.
pub(crate) fn render(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    shell.ensure_passphrase_inputs(window, cx);
    shell.export.focus.apply_pending(window, cx);
    let modal = if let Some(replace) = shell.export.replace.as_ref() {
        replace_modal(replace, shell, cx)
    } else {
        dialog_modal(shell.export.dialog.as_ref()?, shell, cx)
    };
    let viewport = window.viewport_size();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                deferred(
                    anchored().position(Point::default()).child(
                        div()
                            .relative()
                            .w(viewport.width)
                            .h(viewport.height)
                            .child(modal),
                    ),
                )
                .with_priority(3),
            )
            .into_any_element(),
    )
}

fn caption(id: &'static str, text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let text = text.into();
    ui::role(text_region(id), TextVariant::Caption, cx)
        .aria_label(text.clone())
        .child(text)
}

/// What the format writes, under the control.
fn format_note(format: Format, environment: bool) -> &'static str {
    match (format, environment) {
        (Format::Postman, true) => "A Postman environment file with the variables.",
        (Format::Postman, false) => {
            "A Postman collection v2.1 file. Secret values are written empty unless you \
             include them; the variables stay so it still runs once they are filled in."
        }
        (Format::Bundle, _) => {
            "ReqFlux bundle: the requests with their folders, scripts and examples, in \
             ReqFlux's format. Import it into any workspace."
        }
        (Format::Docs, _) => {
            "README.md and index.html, generated from the collection's requests, \
             descriptions and examples. Variables stay as {{variables}}; no secret value \
             is written."
        }
    }
}

fn dialog_modal(dialog: &Dialog, shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    let title = dialog.scope.title();
    let environment = matches!(dialog.scope, super::Scope::Environment { .. });
    let control = format_control(dialog, cx);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::BODY_GAP)
        .child(modal_field("Format", control, cx))
        .child(caption(
            "export-format-note",
            format_note(dialog.format, environment),
            cx,
        ));
    body = match dialog.format {
        Format::Postman => body.children(postman_options(dialog, environment, cx)),
        Format::Bundle => body.children(bundle_options(dialog, cx)),
        Format::Docs => body,
    };
    let (next, confirm) = match dialog.format {
        Format::Docs => ("Next: choose a folder", "Choose folder…"),
        Format::Postman if dialog.include_secrets && !dialog.postman_secrets.is_empty() => {
            ("Next: choose where to save", "Save with secrets…")
        }
        Format::Postman | Format::Bundle => ("Next: choose where to save", "Save as…"),
    };
    let footer = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(caption("export-next", next, cx))
        .child(div().flex_1())
        .child(
            Button::new("export-cancel", "Cancel")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| this.close_export(cx))),
        )
        .child(
            Button::new("export-confirm", confirm)
                .primary()
                .icon(ui::icon(IconName::Download))
                .on_click(cx.listener(|this, _event, window, cx| this.confirm_export(window, cx))),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    Modal::new(title.clone())
        .id("export-backdrop")
        .icon(ui::icon(IconName::Upload))
        .width(tokens::EXPORT_DIALOG_WIDTH)
        .focus_handle(shell.export.focus.handle())
        .body(
            region("export-dialog")
                .role(Role::Dialog)
                .aria_label(title)
                .child(body),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_export(cx)) {
                log::warn!("export close: {error}");
            }
        })
        .on_confirm(move |window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.confirm_export(window, cx)) {
                log::warn!("export confirm: {error}");
            }
        })
        .into_any_element()
}

/// The format control with the formats the scope takes.
// bolt gap: `SegmentedItem` has no disabled state with a tooltip, so a format the scope
// cannot take ("API docs cover a whole collection") is left out instead of drawn disabled.
fn format_control(dialog: &Dialog, cx: &mut Context<Shell>) -> SegmentedControl {
    let items = dialog
        .scope
        .formats()
        .iter()
        .map(|format| {
            SegmentedItem::new(format.slug(), format.label())
                .element_id(format!("export-format-{}", format.slug()))
        })
        .collect();
    let shell = cx.entity().downgrade();
    segmented::control(
        "export-format",
        Some(SharedString::from("Format")),
        true,
        items,
        dialog.format.slug(),
        move |id, _window, cx| {
            let Some(format) = Format::from_slug(id) else {
                return;
            };
            if let Err(error) = shell.update(cx, |this, cx| this.set_export_format(format, cx)) {
                log::warn!("export format: {error}");
            }
        },
    )
}

/// Include secret values, its note and, while on, the plaintext warning.
fn postman_options(dialog: &Dialog, environment: bool, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let secrets = &dialog.postman_secrets;
    let note = if dialog.include_secrets && !secrets.is_empty() {
        secrets_on_note(secrets)
    } else {
        secrets_off_note(secrets, environment)
    };
    let mut options = vec![
        Checkbox::new("export-include-secrets")
            .label("Include secret values")
            .checked(dialog.include_secrets)
            .disabled(secrets.is_empty())
            .on_click(cx.listener(|this, checked: &bool, _window, cx| {
                this.set_include_secrets(*checked, cx);
            }))
            .into_any_element(),
        caption("export-secrets-note", note, cx).into_any_element(),
    ];
    if dialog.include_secrets && !secrets.is_empty() {
        let warning = plaintext_warning(secrets);
        options.push(
            region("export-plaintext-warning")
                .role(Role::Label)
                .aria_label(warning.clone())
                .child(
                    BannerBlock::new(BannerVariant::Warning, "Secrets in plain text")
                        .default_icon(ui::icon(IconName::TriangleAlert))
                        .with_body(warning),
                )
                .into_any_element(),
        );
    }
    options
}

/// The secrets choice of a bundle and, when encrypting, the passphrase fields.
// bolt gap: no radio or option group; the two choices are `bolt` checkboxes that act as one.
fn bundle_options(dialog: &Dialog, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let secrets = &dialog.bundle_secrets;
    let mut options = vec![
        caption(
            "export-secrets-count",
            format!("Secrets · {} in {}", secrets.len(), dialog.scope.place()),
            cx,
        )
        .into_any_element(),
        Checkbox::new("export-secrets-omit")
            .label("Leave secrets out")
            .checked(!dialog.encrypt)
            .on_click(cx.listener(|this, _: &bool, _window, cx| this.set_encrypt(false, cx)))
            .into_any_element(),
        caption(
            "export-secrets-omit-note",
            "The bundle lists them by name, empty. Whoever imports it fills them in.",
            cx,
        )
        .into_any_element(),
        Checkbox::new("export-secrets-encrypt")
            .label("Encrypt secrets with a passphrase")
            .checked(dialog.encrypt)
            .on_click(cx.listener(|this, _: &bool, _window, cx| this.set_encrypt(true, cx)))
            .into_any_element(),
        caption(
            "export-secrets-encrypt-note",
            "Values are sealed with age (scrypt). Share the passphrase another way.",
            cx,
        )
        .into_any_element(),
    ];
    if !dialog.encrypt && !secrets.is_empty() {
        let warning = omitted_warning(secrets);
        options.push(
            region("export-omitted")
                .role(Role::Label)
                .aria_label(warning.clone())
                .child(
                    BannerBlock::new(BannerVariant::Warning, "Secrets left out")
                        .default_icon(ui::icon(IconName::TriangleAlert))
                        .with_body(warning),
                )
                .into_any_element(),
        );
    }
    if dialog.encrypt
        && let Some([first, second]) = dialog.passphrase.as_ref()
    {
        let error = dialog.error;
        let first_error = error.filter(|error| *error == super::PassphraseError::Empty);
        let second_error = error.filter(|error| *error == super::PassphraseError::Mismatch);
        options.push(
            passphrase_field(
                ("export-passphrase", "export-passphrase-error"),
                "Passphrase",
                first,
                first_error.map(super::PassphraseError::message),
                cx,
            )
            .into_any_element(),
        );
        options.push(
            passphrase_field(
                (
                    "export-passphrase-confirm",
                    "export-passphrase-confirm-error",
                ),
                "Confirm passphrase",
                second,
                second_error.map(super::PassphraseError::message),
                cx,
            )
            .into_any_element(),
        );
        options.push(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(
                    Button::new(
                        "export-passphrase-reveal",
                        if dialog.reveal { "Hide" } else { "Show" },
                    )
                    .ghost()
                    .icon(ui::icon(IconName::Eye))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.toggle_passphrase_reveal(window, cx);
                    })),
                )
                .child(caption(
                    "export-passphrase-hint",
                    "Asked once; ReqFlux never stores it. Without it the bundle cannot be opened.",
                    cx,
                ))
                .into_any_element(),
        );
    }
    options
}

/// A masked passphrase field `id` under `label`, its error under it as `error_id`. It stays a
/// secret field while Show draws its text, so the passphrase never reaches the automation
/// tree.
// bolt gap: `Input` has no inline error slot; the error is a caption in the danger color.
fn passphrase_field(
    (id, error_id): (&'static str, &'static str),
    label: &'static str,
    input: &Entity<InputState>,
    error: Option<&'static str>,
    cx: &App,
) -> Div {
    let field = region(id)
        .role(Role::TextInput)
        .aria_label(label)
        .child(Input::new(input).secret(true));
    let mut column = div()
        .flex()
        .flex_col()
        .gap(Spacing::XS)
        .child(modal_field(label, field, cx));
    if let Some(error) = error {
        column = column.child(
            text_region(error_id)
                .aria_label(error)
                .text_color(bolt::colors(cx).danger)
                .child(error),
        );
    }
    column
}

/// Replace API docs?, over the files that exist.
fn replace_modal(replace: &ReplaceDocs, shell: &Shell, cx: &mut Context<Shell>) -> AnyElement {
    const TITLE: &str = "Replace API docs?";
    let names = replace
        .docs
        .files
        .iter()
        .map(|file| file.name.clone())
        .collect::<Vec<_>>();
    let text = format!(
        "{} already exist in {}. Replacing overwrites both files.",
        super::listed(&names),
        replace.dir.display()
    );
    let footer = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(div().flex_1())
        .child(
            Button::new("export-replace-browse", "Choose another folder…")
                .secondary()
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.browse_docs_again(window, cx);
                })),
        )
        .child(
            Button::new("export-replace-confirm", "Replace files")
                .danger()
                .on_click(cx.listener(|this, _event, _window, cx| this.replace_docs(cx))),
        );
    let on_close = cx.entity().downgrade();
    Modal::new(TITLE)
        .id("export-replace-backdrop")
        .icon(ui::icon(IconName::TriangleAlert))
        .focus_handle(shell.export.focus.handle())
        .body(
            region("export-replace-dialog")
                .role(Role::Dialog)
                .aria_label(text.clone())
                .child(text),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| {
                this.close_replace(cx);
            }) {
                log::warn!("replace api docs close: {error}");
            }
        })
        .into_any_element()
}
