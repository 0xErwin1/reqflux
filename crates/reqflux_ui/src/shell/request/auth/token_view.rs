//! The Token row of an OAuth 2.0 auth, as artboard 05 draws it: the state badge, the expiry
//! and a hint, with Get new token and Clear; while Get new token runs, an info banner
//! (waiting for the browser, with Open browser again, or asking the token endpoint) with
//! Cancel; after a failure, a danger banner with Try again.

use std::time::{SystemTime, UNIX_EPOCH};

use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, BannerBlock, BannerVariant, TextVariant};
use bolt::tokens::Spacing;
use chrono::{Local, Offset};
use gpui::{AnyElement, Context, ParentElement, Role, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_core::model::{OAuth2Auth, OAuth2Grant};

use super::super::ui;
use super::form_row;
use super::token_model::{TokenTone, host, token_text};
use crate::shell::Shell;
use crate::shell::auth_edit::TokenFetching;
use crate::shell::rows::{labeled_row, part};

/// The Get new token button, `request-auth-token-get`, `primary` when there is no token to
/// send.
fn get_button(request_id: &str, primary: bool, cx: &mut Context<Shell>) -> Button {
    let request = request_id.to_owned();
    let button = Button::new("request-auth-token-get", "Get new token")
        .icon(ui::icon(if primary {
            IconName::Key
        } else {
            IconName::RotateCcw
        }))
        .on_click(cx.listener(move |this, _event, window, cx| {
            this.get_new_token(&request, window, cx);
        }));
    if primary { button.primary() } else { button }
}

/// The banner of a Get new token in flight, `request-auth-token-waiting`.
fn waiting(
    request_id: &str,
    oauth: &OAuth2Auth,
    fetching: &TokenFetching,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let browser = fetching.grant == OAuth2Grant::AuthorizationCode;
    let (title, body) = if browser {
        let listens = fetching.redirect_uri.as_deref().map_or_else(
            || "ReqFlux is starting its loopback redirect.".to_owned(),
            |uri| format!("ReqFlux listens on {uri} for 5 minutes."),
        );
        (
            "Waiting for the browser.",
            format!("Sign in at {}. {listens}", host(&oauth.auth_url)),
        )
    } else {
        (
            "Getting a new token.",
            format!("Asking {}.", host(&oauth.token_url)),
        )
    };
    let request = request_id.to_owned();
    let cancel = Button::new("request-auth-token-cancel", "Cancel").on_click(cx.listener(
        move |this, _event, _window, cx| {
            this.cancel_token_fetch(&request, cx);
        },
    ));
    let request = request_id.to_owned();
    let reopen = (browser && fetching.redirect_uri.is_some()).then(|| {
        Button::new("request-auth-token-reopen", "Open browser again")
            .icon(ui::icon(IconName::ExternalLink))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.reopen_token_browser(&request, cx);
            }))
    });
    let actions = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .children(reopen)
        .child(cancel);
    part("request-auth-token-waiting".to_owned())
        .role(Role::Status)
        .aria_label(format!("{title} {body}"))
        .w_full()
        .child(
            BannerBlock::new(BannerVariant::Info, title)
                .default_icon(ui::icon(IconName::Loader))
                .with_body(body)
                .with_actions(actions),
        )
        .into_any_element()
}

/// The banner of a failed Get new token or Clear, `request-auth-token-error`.
fn failed(request_id: &str, error: &str, cx: &mut Context<Shell>) -> AnyElement {
    let request = request_id.to_owned();
    let retry = Button::new("request-auth-token-retry", "Try again").on_click(cx.listener(
        move |this, _event, window, cx| {
            this.get_new_token(&request, window, cx);
        },
    ));
    let title = "No new token.";
    let body = format!("{error}. A token kept before is still kept.");
    part("request-auth-token-error".to_owned())
        .role(Role::Alert)
        .aria_label(format!("{title} {body}"))
        .w_full()
        .child(
            BannerBlock::new(BannerVariant::Danger, title)
                .default_icon(ui::icon(IconName::TriangleAlert))
                .with_body(body)
                .with_actions(retry),
        )
        .into_any_element()
}

/// The Token row, `request-auth-token`, of the OAuth 2.0 auth `oauth` of `request_id`.
pub(super) fn render(
    request_id: &str,
    oauth: &OAuth2Auth,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> AnyElement {
    if let Some(fetching) = shell.token_fetching(request_id) {
        let banner = waiting(request_id, oauth, &fetching, cx);
        return form_row(
            labeled_row(
                "request-auth-token".to_owned(),
                "Token: getting a new token".to_owned(),
            ),
            "Token",
            vec![banner],
            cx,
        )
        .into_any_element();
    }
    if let Some(error) = shell.token_failure(request_id) {
        let banner = failed(request_id, error, cx);
        return form_row(
            labeled_row(
                "request-auth-token".to_owned(),
                "Token: no new token".to_owned(),
            ),
            "Token",
            vec![banner],
            cx,
        )
        .into_any_element();
    }
    let Some(state) = shell.oauth_token_state(request_id) else {
        return form_row(
            labeled_row("request-auth-token".to_owned(), "Token: unknown".to_owned()),
            "Token",
            vec![get_button(request_id, true, cx).into_any_element()],
            cx,
        )
        .into_any_element();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let text = token_text(state, oauth, now, Local::now().offset().fix());
    let tone = match text.tone {
        TokenTone::Valid => BadgeTone::Success,
        TokenTone::Expired => BadgeTone::Warning,
        TokenTone::Missing => BadgeTone::Neutral,
    };
    let icon = match text.tone {
        TokenTone::Valid => Some(IconName::Check),
        TokenTone::Expired => Some(IconName::Clock),
        TokenTone::Missing => None,
    };
    let badge = icon
        .into_iter()
        .fold(Badge::new(text.badge, tone), |badge, name| {
            badge.icon(ui::icon(name))
        });
    let request = request_id.to_owned();
    let clear = text.clearable.then(|| {
        Button::new("request-auth-token-clear", "Clear")
            .icon(ui::icon(IconName::Ban))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.clear_token(&request, cx);
            }))
    });
    let line = div()
        .w_full()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            part("request-auth-token-state".to_owned())
                .role(Role::Label)
                .aria_label(text.badge)
                .flex_none()
                .child(badge),
        )
        .children(text.time.clone().map(|time| {
            ui::role(
                part("request-auth-token-time".to_owned()),
                TextVariant::Code,
                cx,
            )
            .child(time)
        }))
        .child(
            ui::role(
                part("request-auth-token-hint".to_owned()),
                TextVariant::Caption,
                cx,
            )
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .child(text.hint.clone()),
        )
        .child(div().flex_1())
        .child(get_button(request_id, text.get_is_primary, cx))
        .children(clear)
        .into_any_element();
    let label = match &text.time {
        Some(time) => format!("Token: {} {time}", text.badge),
        None => format!("Token: {}", text.badge),
    };
    form_row(
        labeled_row("request-auth-token".to_owned(), label),
        "Token",
        vec![line],
        cx,
    )
    .into_any_element()
}
