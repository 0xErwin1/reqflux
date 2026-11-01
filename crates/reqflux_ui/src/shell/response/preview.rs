//! The Preview body view: an image decoded and scaled down to fit, or the title and readable
//! text of an HTML page with `Open in browser`. Decoding and text extraction run on GPUI's
//! background executor when Preview is selected, never on the UI thread (S6).

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use bolt::controls::Button;
use bolt::tokens::{Borders, ResultMetrics, Spacing};
use bolt_legacy::IconName;
use gpui::{
    AnyElement, App, Context, FontWeight, ParentElement, RenderImage, Styled, div, img, prelude::*,
};

use super::preview_model::{HtmlText, ImageKind, PreviewKind, html_text, too_large};
use crate::shell::request::{Completed, Received, SendState, ui};
use crate::shell::send::RequestSend;
use crate::shell::{Shell, region, text_region};
use crate::tokens;

/// What the Preview view shows, computed once per response off the UI thread.
pub(crate) enum Preview {
    Image {
        image: Arc<RenderImage>,
        width: u32,
        height: u32,
        kind: ImageKind,
    },
    /// An image body that did not decode.
    Undecodable,
    Html(HtmlText),
    /// A body over the limit of its kind.
    TooLarge,
}

impl Preview {
    /// `<width>×<height> <format>` under an image, e.g. `2×1 PNG`.
    pub(crate) fn meta(&self) -> Option<String> {
        match self {
            Self::Image {
                width,
                height,
                kind,
                ..
            } => Some(format!("{width}×{height} {}", kind.label())),
            Self::Undecodable | Self::Html(_) | Self::TooLarge => None,
        }
    }

    /// The preview of a `kind` body, decoding or extracting it: slow for a large body.
    fn compute(kind: PreviewKind, body: &[u8]) -> Self {
        if too_large(kind, body.len()) {
            return Self::TooLarge;
        }
        match kind {
            PreviewKind::Image(kind) => decode(kind, body),
            PreviewKind::Html => Self::Html(html_text(&String::from_utf8_lossy(body))),
        }
    }
}

/// `body` as a `kind` image in GPUI's BGRA layout, or [`Preview::Undecodable`].
fn decode(kind: ImageKind, body: &[u8]) -> Preview {
    let format = match kind {
        ImageKind::Png => image::ImageFormat::Png,
        ImageKind::Jpeg => image::ImageFormat::Jpeg,
        ImageKind::Gif => image::ImageFormat::Gif,
        ImageKind::Webp => image::ImageFormat::WebP,
    };
    match image::load_from_memory_with_format(body, format) {
        Ok(decoded) => {
            let mut pixels = decoded.into_rgba8();
            let (width, height) = pixels.dimensions();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let frame = image::Frame::new(pixels);
            Preview::Image {
                image: Arc::new(RenderImage::new(vec![frame])),
                width,
                height,
                kind,
            }
        }
        Err(error) => {
            log::debug!("the response image did not decode: {error}");
            Preview::Undecodable
        }
    }
}

/// Where `Open in browser` writes the page and how it opens it: the per-user cache
/// directory and the system handler in the app; a temporary directory and a recorder in
/// tests, so no test opens a browser.
#[derive(Clone)]
pub(crate) struct PreviewHost {
    /// The directory the page files go to; `None` is the per-user cache directory.
    pub(crate) dir: Option<PathBuf>,
    pub(crate) open: Opener,
}

/// Opens a written page file.
pub(crate) type Opener = Rc<dyn Fn(&Path, &App)>;

impl Default for PreviewHost {
    fn default() -> Self {
        Self {
            dir: None,
            open: Rc::new(|path, cx| cx.open_with_system(path)),
        }
    }
}

impl PreviewHost {
    fn dir(&self) -> Result<PathBuf, String> {
        match &self.dir {
            Some(dir) => Ok(dir.clone()),
            None => cache_dir(
                std::env::var_os("XDG_CACHE_HOME")
                    .map(PathBuf::from)
                    .as_deref(),
                std::env::var_os("HOME").map(PathBuf::from).as_deref(),
            )
            .ok_or_else(|| {
                "Neither XDG_CACHE_HOME nor HOME is set; there is no cache directory".to_owned()
            }),
        }
    }
}

/// `$XDG_CACHE_HOME/reqflux/preview` when it is absolute, else `$HOME/.cache/reqflux/preview`;
/// `None` without either.
fn cache_dir(cache_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let base = match cache_home.filter(|candidate| candidate.is_absolute()) {
        Some(cache_home) => cache_home.to_path_buf(),
        None => home?.join(".cache"),
    };
    Some(base.join("reqflux/preview"))
}

/// Writes `body` to a new randomly named `.html` file in `dir`, creating the directory.
fn write_page(dir: Result<PathBuf, String>, body: &[u8]) -> Result<PathBuf, String> {
    let dir = dir?;
    let path = dir.join(format!("{}.html", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&dir)
        .and_then(|()| OpenOptions::new().write(true).create_new(true).open(&path))
        .and_then(|mut file| file.write_all(body))
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path)
}

/// The response of the last finished send, not one dimmed under a send in flight.
fn done(state: &SendState) -> Option<&Received> {
    match state {
        SendState::Done(Completed::Response(received)) => Some(received),
        SendState::Idle | SendState::Sending { .. } | SendState::Done(Completed::Failure(_)) => {
            None
        }
    }
}

impl Shell {
    /// Starts computing the preview of the response of the request `id` on the background
    /// executor, unless it is computed, in progress or the body has nothing to preview.
    pub(crate) fn ensure_preview(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        if send.preview.is_some() || send.preview_task.is_some() {
            return;
        }
        let Some((kind, body)) =
            done(&send.state).and_then(|received| Some((received.preview?, received.body.clone())))
        else {
            return;
        };
        let compute = cx
            .background_executor()
            .spawn(async move { Preview::compute(kind, &body) });
        let key = id.to_owned();
        send.preview_task = Some(cx.spawn(async move |this, cx| {
            let preview = compute.await;
            if let Err(error) = this.update(cx, |shell, cx| {
                if let Some(send) = shell.sends.get_mut(&key) {
                    send.preview = Some(Arc::new(preview));
                    send.preview_task = None;
                    cx.notify();
                }
            }) {
                log::debug!("preview of {key} ended after its shell: {error}");
            }
        }));
    }

    /// `Open in browser`: writes the HTML body of the request `id` to a new file in the
    /// preview directory off the UI thread, then opens it; a failed write shows under the
    /// title. ReqFlux fetches nothing to render it.
    pub(crate) fn open_preview_in_browser(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(send) = self.sends.get_mut(id) else {
            return;
        };
        let Some(body) = done(&send.state).map(|received| received.body.clone()) else {
            return;
        };
        send.preview_error = None;
        let dir = self.preview_host.dir();
        let open = Rc::clone(&self.preview_host.open);
        let write = cx
            .background_executor()
            .spawn(async move { write_page(dir, &body) });
        let key = id.to_owned();
        cx.spawn(async move |this, cx| {
            let written = write.await;
            if let Err(error) = this.update(cx, |shell, cx| {
                match written {
                    Ok(path) => open(&path, cx),
                    Err(message) => {
                        if let Some(send) = shell.sends.get_mut(&key) {
                            send.preview_error = Some(message);
                        }
                    }
                }
                cx.notify();
            }) {
                log::debug!("open in browser of {key} ended after its shell: {error}");
            }
        })
        .detach();
    }
}

/// The Preview view of the request `id`, element id `response-preview`.
pub(super) fn render(id: &str, send: &RequestSend, cx: &mut Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let pane = region("response-preview")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .border_t(Borders::THIN)
        .border_color(colors.line);
    let Some(preview) = send.preview.as_deref() else {
        return pane
            .child(message("Preparing the preview", cx))
            .into_any_element();
    };
    match preview {
        Preview::Image {
            image,
            width,
            height,
            ..
        } => {
            let meta = preview.meta().unwrap_or_default();
            let natural = tokens::image_size(*width, *height);
            pane.p(Spacing::MD)
                .gap(Spacing::SM)
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            // Its natural size, capped by the pane: scaled down, never up.
                            region("response-preview-image")
                                .w(natural.width)
                                .h(natural.height)
                                .max_w_full()
                                .max_h_full()
                                .child(img(Arc::clone(image)).size_full()),
                        ),
                )
                .child(
                    text_region("response-preview-meta")
                        .aria_label(meta.clone())
                        .flex_none()
                        .text_size(ResultMetrics::STATEMENT_FONT)
                        .text_color(colors.muted)
                        .child(meta),
                )
                .into_any_element()
        }
        Preview::Undecodable => pane
            .child(message("This image could not be decoded", cx))
            .into_any_element(),
        Preview::TooLarge => pane
            .child(message("This body is too large to preview", cx))
            .into_any_element(),
        Preview::Html(page) => pane.child(html(id, page, send, cx)).into_any_element(),
    }
}

/// The title row with `Open in browser`, the write error when the last open failed, and
/// the readable text, element id `response-preview-text`.
fn html(id: &str, page: &HtmlText, send: &RequestSend, cx: &mut Context<Shell>) -> AnyElement {
    let open = open_button(id, cx);
    let colors = bolt::colors(cx);
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| "Untitled page".to_owned());
    let text = text_label(&page.paragraphs);
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::MD)
                .p(Spacing::MD)
                .child(
                    text_region("response-preview-title")
                        .aria_label(title.clone())
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.strong)
                        .child(title),
                )
                .child(open),
        )
        .children(send.preview_error.clone().map(|error| {
            text_region("response-preview-error")
                .aria_label(error.clone())
                .flex_none()
                .pl(Spacing::MD)
                .pr(Spacing::MD)
                .pb(Spacing::SM)
                .text_size(bolt::density::font_xs(cx))
                .text_color(colors.danger)
                .child(error)
        }))
        .child(
            text_region("response-preview-text")
                .aria_label(text)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(Spacing::SM)
                .pl(Spacing::MD)
                .pr(Spacing::MD)
                .pb(Spacing::MD)
                .text_size(bolt::density::font_base(cx))
                .text_color(colors.foreground)
                .children(
                    page.paragraphs
                        .iter()
                        .map(|paragraph| div().child(paragraph.clone())),
                ),
        )
        .into_any_element()
}

/// The paragraphs as one line for the automation label, cut on a char boundary to fit
/// [`gpui_mcp::MAX_LABEL_BYTES`]: the bridge drops a node whose label holds a control
/// character, such as a newline, or is longer.
fn text_label(paragraphs: &[String]) -> String {
    const ELLIPSIS: char = '…';
    let text = paragraphs.join(" ");
    if text.len() <= gpui_mcp::MAX_LABEL_BYTES {
        return text;
    }
    let mut end = gpui_mcp::MAX_LABEL_BYTES - ELLIPSIS.len_utf8();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut label = text.get(..end).unwrap_or_default().trim_end().to_owned();
    label.push(ELLIPSIS);
    label
}

/// `Open in browser`, element id `response-preview-open`.
fn open_button(id: &str, cx: &mut Context<Shell>) -> AnyElement {
    let id = id.to_owned();
    ui::tagged("response-preview-open")
        .child(
            Button::new("response-preview-open", "Open in browser")
                .secondary()
                .icon(ui::icon(IconName::ExternalLink))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.open_preview_in_browser(&id, cx);
                })),
        )
        .into_any_element()
}

/// A muted line in the preview pane, element id `response-preview-message`.
fn message(text: &'static str, cx: &App) -> AnyElement {
    text_region("response-preview-message")
        .aria_label(text)
        .p(Spacing::MD)
        .text_size(bolt::density::font_base(cx))
        .text_color(bolt::colors(cx).muted)
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_label_is_one_line_cut_to_the_label_limit() {
        let short = text_label(&["Hello".to_owned(), "Fish & chips".to_owned()]);
        assert_eq!(short, "Hello Fish & chips");

        let long = text_label(&["é".repeat(gpui_mcp::MAX_LABEL_BYTES)]);
        assert!(long.len() <= gpui_mcp::MAX_LABEL_BYTES);
        assert!(long.ends_with('…'));
        assert!(long.trim_end_matches('…').chars().all(|c| c == 'é'));
    }

    #[test]
    fn the_cache_dir_prefers_an_absolute_xdg_cache_home() {
        assert_eq!(
            cache_dir(Some(Path::new("/c")), Some(Path::new("/h"))),
            Some(PathBuf::from("/c/reqflux/preview"))
        );
        assert_eq!(
            cache_dir(Some(Path::new("rel")), Some(Path::new("/h"))),
            Some(PathBuf::from("/h/.cache/reqflux/preview"))
        );
        assert_eq!(
            cache_dir(None, Some(Path::new("/h"))),
            Some(PathBuf::from("/h/.cache/reqflux/preview"))
        );
        assert_eq!(cache_dir(None, None), None);
    }

    #[test]
    fn an_image_over_the_limit_is_not_decoded() {
        let body = vec![0_u8; super::super::preview_model::IMAGE_LIMIT + 1];
        assert!(matches!(
            Preview::compute(PreviewKind::Image(ImageKind::Png), &body),
            Preview::TooLarge
        ));
        assert!(matches!(
            Preview::compute(PreviewKind::Image(ImageKind::Png), b"not a png"),
            Preview::Undecodable
        ));
    }
}
