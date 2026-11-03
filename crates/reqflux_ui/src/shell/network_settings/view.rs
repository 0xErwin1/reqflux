//! Drawing App > Network (design/network/01-settings-sections, main frame and States): the
//! title row with the READ FROM CONFIG.TOML badge, `network-open-config` and
//! `network-reload`; `network-config-error` while the file does not parse; Trust with the CA
//! bundle `network-ca-bundle` and the certificate list `network-certificates`, one row
//! `network-certificate-<n>` each (a skipped one struck through, its warnings
//! `network-certificate-<n>-warning-<m>`, a PKCS#12 one with its keyring badge
//! `network-certificate-<n>-keyring` and `-set` or `-change` and `-clear`); the app proxy
//! (`network-proxy-mode`, `network-proxy-url`, `network-proxy-username`,
//! `network-proxy-password` with `network-proxy-password-set|change|clear`,
//! `network-proxy-bypass`, warnings `network-proxy-warning-<m>`), and the open workspace's
//! proxy `network-workspace-proxy` (its password `network-workspace-proxy-password`). The
//! Set and Change dialog is `network-secret-dialog` with the masked field
//! `network-secret-input`, `network-secret-cancel` and `network-secret-save`.
//!
//! It draws with `bolt`: `SegmentedControl`, `Badge`, `Button`, `Input`, `BannerBlock` and
//! `Modal`. No value of a secret is drawn: a keyring badge says `SET` or `NOT SET`.

use bolt::controls::{Button, Input, InputEnter};
use bolt::modals::{Modal, modal_field, modal_lead};
use bolt::primitives::{
    Badge, BadgeTone, BannerBlock, BannerVariant, Icon, SegmentedItem, TextVariant,
};
use bolt::toast::Toast;
use bolt::tokens::{Borders, Fields, HeaderMetrics, ModalMetrics, Spacing};
use gpui::{
    AnyElement, App, Context, Div, IntoElement, ParentElement, Point, Role, Stateful, Styled,
    Window, anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::NetworkSecret;
use reqflux_engine::config::ProxyMode;

use super::{CertificateKind, CertificateRow, Keyring, NetworkModel, ProxyRow, SecretRow};
use crate::shell::request::{segmented, ui};
use crate::shell::rows::{labeled_row, part};
use crate::shell::{Shell, region, text_region};
use crate::tokens::{
    APP_SETTINGS_COLUMN_GAP as COLUMN_GAP, APP_SETTINGS_OVERRIDE_RULE as KIND_WIDTH,
    APP_SETTINGS_RULE_LABEL_WIDTH as LABEL_WIDTH, COOKIE_DOMAIN_COLUMN as HOST_WIDTH,
};

const DESCRIPTION: &str = "Used by every send on this machine: requests, auth requests and \
                           pm.sendRequest. Values come from [network] in config.toml; \
                           passwords and passphrases live in the OS keyring.";

/// The content island of App > Network.
pub(crate) fn content(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> Div {
    let colors = bolt::colors(cx);
    let title = div()
        .flex_none()
        .flex()
        .items_start()
        .gap(Spacing::MD)
        .pt(HeaderMetrics::SECTION_PADDING_TOP)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .pb(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(HeaderMetrics::SECTION_GAP)
                .child(
                    ui::role(text_region("settings-heading"), TextVariant::Title, cx)
                        .aria_label("Network")
                        .child("Network"),
                )
                .child(
                    ui::role(text_region("settings-description"), TextVariant::Body, cx)
                        .aria_label(DESCRIPTION)
                        .text_color(colors.muted)
                        .child(DESCRIPTION),
                ),
        )
        .child(div().pt(Spacing::XS).child(
            Badge::new("READ FROM CONFIG.TOML", BadgeTone::Neutral).icon(ui::icon(IconName::Lock)),
        ))
        .child(
            Button::new("network-open-config", "Open config.toml")
                .secondary()
                .icon(ui::icon(IconName::ExternalLink))
                .on_click(cx.listener(|this, _event, _window, cx| this.open_config_file(cx))),
        )
        .child(
            Button::new("network-reload", "Reload")
                .secondary()
                .icon(ui::icon(IconName::RefreshCw))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.reload_app_config(window, cx);
                })),
        );
    let banner = shell
        .settings
        .network
        .config_error
        .as_ref()
        .map(|detail| config_error(detail, cx));
    let editable = shell.settings.network.config_error.is_none();
    let body = shell.settings.network.model.as_ref().map(|model| {
        div()
            .flex()
            .gap(COLUMN_GAP)
            .child(trust(model, editable, cx))
            .child(proxies(model, editable, cx))
    });
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(title)
        .children(banner)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .pl(HeaderMetrics::SECTION_PADDING_X)
                .pr(HeaderMetrics::SECTION_PADDING_X)
                .children(body),
        )
        .children(secret_dialog(shell, window, cx))
}

/// `network-config-error`: the file does not parse, so sends use the default network
/// settings.
fn config_error(detail: &str, cx: &mut Context<Shell>) -> AnyElement {
    let title = "config.toml does not parse, so sends use the default network settings.";
    let actions = div()
        .flex()
        .gap(Spacing::SM)
        .child(
            Button::new("network-config-open", "Open file")
                .secondary()
                .icon(ui::icon(IconName::ExternalLink))
                .on_click(cx.listener(|this, _event, _window, cx| this.open_config_file(cx))),
        )
        .child(
            Button::new("network-config-reload", "Reload")
                .secondary()
                .icon(ui::icon(IconName::RefreshCw))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.reload_app_config(window, cx);
                })),
        );
    region("network-config-error")
        .role(Role::Alert)
        .aria_label(format!("{title} {detail}"))
        .flex_none()
        .pt(Spacing::LG)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .child(
            BannerBlock::new(BannerVariant::Danger, title)
                .default_icon(ui::icon(IconName::TriangleAlert))
                .with_body(detail.to_owned())
                .with_actions(actions),
        )
        .into_any_element()
}

/// A section title, as `.section` of the artboard: the accent `kicker` over the `heading`
/// (with an optional `hint` beside it), on a line, so a title cannot be read as a label.
fn section(
    kicker: &'static str,
    heading: &'static str,
    hint: Option<&'static str>,
    cx: &App,
) -> Div {
    let colors = bolt::colors(cx);
    let accent = bolt::brand::brand(cx).accent_tint;
    div()
        .flex()
        .flex_col()
        .gap(Spacing::XXS)
        .pt(Spacing::XL)
        .pb(Spacing::SM)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            ui::role(div(), TextVariant::Label, cx)
                .text_color(accent)
                .child(kicker),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(Spacing::MD)
                .child(ui::role(div(), TextVariant::Heading, cx).child(heading))
                .children(hint.map(|hint| ui::role(div(), TextVariant::Caption, cx).child(hint))),
        )
}

/// A value the page shows but cannot change, on its own inset surface with the plain line, as
/// a disabled input draws; an editable control keeps the stronger edge of `bolt`'s fields.
// bolt gap: no read-only value field; an `Input` would need an `InputState` per value.
fn read_only<E: Styled>(value: E, cx: &App) -> E {
    let colors = bolt::colors(cx);
    value
        .min_h(Fields::HEIGHT)
        .pl(Fields::PADDING_X)
        .pr(Fields::PADDING_X)
        .bg(colors.background)
        .border_1()
        .border_color(colors.line)
}

/// A field row: the label and its note on the left, the value on the right.
fn field_row(label: String, note: Option<String>, value: impl IntoElement, cx: &App) -> Div {
    let colors = bolt::colors(cx);
    div()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .py(Spacing::MD)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex_none()
                .w(LABEL_WIDTH)
                .flex()
                .flex_col()
                .gap(Spacing::XXS)
                .child(div().text_color(colors.muted).child(label))
                .children(note.map(|note| ui::role(div(), TextVariant::Caption, cx).child(note))),
        )
        .child(div().flex_1().min_w_0().flex().flex_col().child(value))
}

/// A value as written in the file, in the editor face.
fn mono(id: String, text: String, struck: bool, cx: &App) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    read_only(labeled_row(id, text.clone()), cx)
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(Fields::TEXT)
        .text_color(if struck { colors.muted } else { colors.strong })
        .when(struck, |text| text.line_through())
        .child(text)
}

/// The warnings of an entry, `<prefix>-warning-<m>`, in the warning tone.
fn warnings(prefix: &str, warnings: &[String], cx: &App) -> Vec<AnyElement> {
    let colors = bolt::colors(cx);
    warnings
        .iter()
        .enumerate()
        .map(|(index, warning)| {
            labeled_row(format!("{prefix}-warning-{index}"), warning.clone())
                .flex()
                .items_start()
                .gap(Spacing::SM)
                .pt(Spacing::XS)
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(Fields::TEXT)
                .text_color(colors.warning)
                .child(
                    Icon::new(ui::icon(IconName::TriangleAlert))
                        .size(Fields::CHEVRON)
                        .color(colors.warning),
                )
                .child(warning.clone())
                .into_any_element()
        })
        .collect()
}

/// The keyring badge `<prefix>-keyring` of `secret` and its buttons: Set… when not set,
/// Change… and Clear when set, none while the keyring cannot be asked or the file does not
/// parse.
fn keyring(prefix: &str, secret: &SecretRow, editable: bool, cx: &mut Context<Shell>) -> Div {
    let (text, tone) = match secret.state {
        Keyring::Set => ("KEYRING · SET", BadgeTone::Success),
        Keyring::NotSet => ("KEYRING · NOT SET", BadgeTone::Warning),
        Keyring::Unavailable => ("KEYRING · UNAVAILABLE", BadgeTone::Danger),
    };
    let mut badge = Badge::new(text, tone);
    if secret.state == Keyring::Set {
        badge = badge.icon(ui::icon(IconName::KeyRound));
    }
    let set = |label: &'static str, suffix: &str, cx: &mut Context<Shell>| {
        let target = secret.secret.clone();
        Button::new(
            gpui::ElementId::Name(format!("{prefix}-{suffix}").into()),
            label,
        )
        .secondary()
        .inline()
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.open_network_secret(target.clone(), window, cx);
        }))
    };
    let buttons: Vec<AnyElement> = match (editable, secret.state) {
        (false, _) | (true, Keyring::Unavailable) => Vec::new(),
        (true, Keyring::NotSet) => vec![set("Set…", "set", cx).into_any_element()],
        (true, Keyring::Set) => {
            let target = secret.secret.clone();
            vec![
                set("Change…", "change", cx).into_any_element(),
                Button::new(
                    gpui::ElementId::Name(format!("{prefix}-clear").into()),
                    "Clear",
                )
                .secondary()
                .inline()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.clear_network_secret(&target, cx);
                }))
                .into_any_element(),
            ]
        }
    };
    div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(part(format!("{prefix}-keyring")).child(badge))
        .child(div().flex_1())
        .children(buttons)
}

/// Trust: the CA bundle and the client certificates.
fn trust(model: &NetworkModel, editable: bool, cx: &mut Context<Shell>) -> Div {
    let ca = match &model.ca_bundle {
        Some(row) => div()
            .flex()
            .flex_col()
            .child(mono(
                "network-ca-bundle".to_owned(),
                row.path.clone(),
                row.skipped,
                cx,
            ))
            .children(warnings("network-ca-bundle", &row.warnings, cx)),
        None => div().child(
            ui::role(text_region("network-ca-bundle"), TextVariant::Caption, cx)
                .aria_label("Not set: the platform roots only")
                .child("Not set: the platform roots only"),
        ),
    };
    let rows: Vec<AnyElement> = model
        .certificates
        .iter()
        .enumerate()
        .map(|(index, row)| certificate(index, row, editable, cx))
        .collect();
    let empty = rows.is_empty().then(|| {
        ui::role(div(), TextVariant::Caption, cx)
            .py(Spacing::SM)
            .child("No client certificates.")
    });
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(section("TRUST", "CA bundle", None, cx))
        .child(field_row(
            "CA bundle".to_owned(),
            Some("PEM, trusted beside the platform roots".to_owned()),
            ca,
            cx,
        ))
        .child(section(
            "TRUST",
            "Client certificates",
            Some("first match wins · *. matches subdomains only"),
            cx,
        ))
        .child(
            region("network-certificates")
                .role(Role::List)
                .flex()
                .flex_col()
                .children(rows)
                .children(empty),
        )
}

/// The row `network-certificate-<index>`.
fn certificate(
    index: usize,
    row: &CertificateRow,
    editable: bool,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let id = format!("network-certificate-{index}");
    let skipped = row.kind == CertificateKind::Skipped;
    let port = row
        .port
        .map_or_else(|| " · any port".to_owned(), |port| format!(":{port}"));
    let (kind, tone) = match row.kind {
        CertificateKind::Pem => ("PEM", BadgeTone::Neutral),
        CertificateKind::Pkcs12 => ("PKCS#12", BadgeTone::Neutral),
        CertificateKind::Skipped => ("SKIPPED", BadgeTone::Danger),
    };
    let label = format!("{}{port} {kind} {}", row.host, row.paths.join(" "));
    let secret = match (&row.passphrase, row.kind) {
        (Some(secret), _) => keyring(&id, secret, editable, cx).into_any_element(),
        (None, CertificateKind::Pem) => ui::role(div(), TextVariant::Caption, cx)
            .child("no passphrase")
            .into_any_element(),
        (None, _) => div().into_any_element(),
    };
    let paths = row.paths.iter().map(|path| {
        div()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(path.clone())
    });
    labeled_row(id.clone(), label)
        .role(Role::ListItem)
        .flex()
        .flex_col()
        .py(Spacing::SM)
        .border_b(Borders::THIN)
        .border_color(colors.row_divider)
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::MD)
                .child(
                    div()
                        .flex_none()
                        .w(HOST_WIDTH)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_color(if skipped { colors.muted } else { colors.strong })
                        .when(skipped, |host| host.line_through())
                        .child(row.host.clone())
                        .child(div().text_color(colors.muted).child(port)),
                )
                .child(
                    div()
                        .flex_none()
                        .w(KIND_WIDTH)
                        .child(Badge::new(kind, tone)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(Spacing::XXS)
                        .font_family(bolt::fonts::editor_family(cx))
                        .text_size(Fields::TEXT)
                        .text_color(colors.muted)
                        .children(paths),
                )
                .child(div().flex_none().child(secret)),
        )
        .children(warnings(&id, &row.warnings, cx))
        .into_any_element()
}

fn mode_name(mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::System => "System",
        ProxyMode::None => "None",
        ProxyMode::Manual => "Manual",
    }
}

/// The System / None / Manual control `<id>`, drawn live: the mode lives in `[network.proxy]`
/// of config.toml, which the page does not write, so choosing one points there.
fn mode_control(id: &str, mode: ProxyMode) -> Div {
    let items = [ProxyMode::System, ProxyMode::None, ProxyMode::Manual]
        .into_iter()
        .map(|candidate| {
            let name = mode_name(candidate);
            SegmentedItem::new(name, name).element_id(format!("{id}-{}", name.to_lowercase()))
        })
        .collect();
    div().flex_none().child(segmented::control(
        id,
        Some("Proxy mode".into()),
        false,
        items,
        mode_name(mode),
        |_, _, cx| {
            Toast::info("The proxy mode is set in config.toml")
                .subtitle("Change mode under [network.proxy], then Reload.")
                .push(cx);
        },
    ))
}

/// The rows of a proxy table under the `<prefix>` ids.
fn proxy_rows(prefix: &str, row: &ProxyRow, editable: bool, cx: &mut Context<Shell>) -> Vec<Div> {
    let mut rows = vec![field_row(
        "Mode".to_owned(),
        None,
        mode_control(&format!("{prefix}-mode"), row.mode),
        cx,
    )];
    match row.mode {
        ProxyMode::System => rows.push(field_row(
            "From".to_owned(),
            None,
            ui::role(div(), TextVariant::Caption, cx).child(
                "HTTP_PROXY, HTTPS_PROXY, ALL_PROXY and NO_PROXY of the environment ReqFlux \
                 started in",
            ),
            cx,
        )),
        ProxyMode::None => rows.push(field_row(
            "Sends".to_owned(),
            None,
            ui::role(div(), TextVariant::Caption, cx)
                .child("direct, whatever the environment says"),
            cx,
        )),
        ProxyMode::Manual => {
            let url = row.url.clone().unwrap_or_else(|| "not set".to_owned());
            rows.push(field_row(
                "URL".to_owned(),
                None,
                mono(format!("{prefix}-url"), url, false, cx),
                cx,
            ));
            if let Some(username) = &row.username {
                rows.push(field_row(
                    "Username".to_owned(),
                    None,
                    mono(format!("{prefix}-username"), username.clone(), false, cx),
                    cx,
                ));
            }
            if let Some(password) = &row.password {
                let id = format!("{prefix}-password");
                rows.push(field_row(
                    "Password".to_owned(),
                    None,
                    keyring(&id, password, editable, cx),
                    cx,
                ));
            }
            let bypass = row.bypass.join(", ");
            let pills = row
                .bypass
                .iter()
                .map(|host| Badge::new(host.clone(), BadgeTone::Neutral));
            rows.push(field_row(
                "Bypass".to_owned(),
                Some("NO_PROXY syntax".to_owned()),
                read_only(labeled_row(format!("{prefix}-bypass"), bypass), cx)
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(Spacing::XS)
                    .children(pills),
                cx,
            ));
        }
    }
    rows
}

/// The app proxy and the open workspace's own one.
fn proxies(model: &NetworkModel, editable: bool, cx: &mut Context<Shell>) -> Div {
    let global = proxy_rows("network-proxy", &model.proxy, editable, cx);
    let global_warnings = warnings("network-proxy", &model.proxy.warnings, cx);
    let workspace =
        model.workspace.as_ref().map(|workspace| {
            let summary = match &workspace.own {
                Some(own) => format!("OWN PROXY {}", mode_name(own.mode)),
                None => format!("APP PROXY {}", mode_name(model.proxy.mode)),
            };
            let (badge, mode, url) = match &workspace.own {
                Some(own) => (
                    Badge::new("OWN PROXY", BadgeTone::Accent),
                    own.mode,
                    own.url.clone(),
                ),
                None => (
                    Badge::new("APP PROXY", BadgeTone::Neutral),
                    model.proxy.mode,
                    model.proxy.url.clone(),
                ),
            };
            let colors = bolt::colors(cx);
            let value = read_only(
                labeled_row("network-workspace-proxy".to_owned(), summary),
                cx,
            )
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(badge)
            .child(div().text_color(colors.strong).child(mode_name(mode)))
            .children(url.filter(|_| mode == ProxyMode::Manual).map(|url| {
                div()
                    .font_family(bolt::fonts::editor_family(cx))
                    .text_color(colors.muted)
                    .child(url)
            }));
            let own_rows = workspace.own.as_ref().map(|own| {
                let password = own.password.as_ref().map(|password| {
                    field_row(
                        "Password".to_owned(),
                        own.username.as_ref().map(|user| format!("user {user}")),
                        keyring("network-workspace-proxy-password", password, editable, cx),
                        cx,
                    )
                });
                let notes = warnings("network-workspace-proxy", &own.warnings, cx);
                (password, notes)
            });
            let (password, notes) = own_rows.unwrap_or_default();
            div()
            .flex()
            .flex_col()
            .child(section("THIS WORKSPACE", "Proxy override", None, cx))
            .child(field_row(
                workspace.name.clone(),
                Some(workspace.id.clone()),
                value,
                cx,
            ))
            .children(password)
            .children(notes)
            .child(
                ui::role(div(), TextVariant::Caption, cx).pt(Spacing::SM).child(
                    "A [network.workspaces.\"<id>\".proxy] table replaces the app proxy for \
                     one workspace. Without one, this workspace uses the proxy above.",
                ),
            )
        });
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(section("EVERY WORKSPACE", "Proxy", None, cx))
        .children(global)
        .children(global_warnings)
        .children(workspace)
}

/// The Set or Change dialog, while open.
fn secret_dialog(shell: &Shell, window: &Window, cx: &mut Context<Shell>) -> Option<AnyElement> {
    let dialog = shell.settings.network.dialog.as_ref()?;
    let field = region("network-secret-input")
        .role(Role::TextInput)
        .on_action(cx.listener(|this, _: &InputEnter, _window, cx| {
            this.save_network_secret(cx);
        }))
        .child(Input::new(&dialog.input).aria_label(dialog.title.clone()));
    let label = match dialog.secret {
        NetworkSecret::CertificatePassphrase { .. } => "Passphrase",
        NetworkSecret::ProxyPassword { .. } => "Password",
    };
    let body = div()
        .flex()
        .flex_col()
        .gap(ModalMetrics::BODY_GAP)
        .child(modal_lead(dialog.detail.clone(), cx))
        .child(modal_field(label, field, cx));
    let footer = div()
        .flex()
        .gap(ModalMetrics::FOOTER_GAP)
        .child(
            Button::new("network-secret-cancel", "Cancel")
                .secondary()
                .on_click(cx.listener(|this, _event, _window, cx| this.close_network_secret(cx))),
        )
        .child(
            Button::new("network-secret-save", "Save to keyring")
                .primary()
                .on_click(cx.listener(|this, _event, _window, cx| this.save_network_secret(cx))),
        );
    let on_close = cx.entity().downgrade();
    let on_confirm = on_close.clone();
    let modal = Modal::new(dialog.title.clone())
        .id("network-secret-backdrop")
        .icon(ui::icon(IconName::KeyRound))
        .focus_handle(shell.settings.network.focus.handle())
        .body(
            region("network-secret-dialog")
                .role(Role::Dialog)
                .aria_label(dialog.title.clone())
                .child(body),
        )
        .footer(footer)
        .on_close(move |_window, cx| {
            if let Err(error) = on_close.update(cx, |this, cx| this.close_network_secret(cx)) {
                log::warn!("network secret close: {error}");
            }
        })
        .on_confirm(move |_window, cx| {
            if let Err(error) = on_confirm.update(cx, |this, cx| this.save_network_secret(cx)) {
                log::warn!("network secret save: {error}");
            }
        });
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
