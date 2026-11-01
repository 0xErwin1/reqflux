//! `Island`, a cut pane floating on the desk.

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, Interactivity, StyleRefinement, Window, div};

use crate::chamfer::Chamfer;
use crate::theme::colors;
use crate::tokens::{border, cut};

/// A pane of the window shell on the desk: the panel fill cut at the top-left and
/// bottom-right corners, an island-edge hairline along the whole outline, and no shadow.
///
/// It lays out as a flex column that clips its overflow; size and lay it out like a `div`
/// and add content with `.child()`. The fill paints behind the content, and the cut corners
/// (in the desk color) and the hairline paint over it, so content that spans the full width
/// still reads as cut. Do not set `.bg()` on it. The content stays ordinary children, so the
/// UI automation tree sees it.
#[derive(IntoElement)]
pub struct Island {
    base: Div,
    children: Vec<AnyElement>,
}

impl Island {
    /// An empty island.
    pub fn new() -> Self {
        Self {
            base: div().relative().flex().flex_col().overflow_hidden(),
            children: Vec::new(),
        }
    }
}

impl std::fmt::Debug for Island {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Island")
            .field("children", &self.children.len())
            .finish_non_exhaustive()
    }
}

impl Default for Island {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Island {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Island {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl ParentElement for Island {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Island {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let fill = Chamfer::new(cut::CARD).fill(colors.panel);
        let mask = Chamfer::new(cut::CARD)
            .outside_fill(colors.desk)
            .edge(colors.island_edge, border::THIN);

        self.base.child(fill).children(self.children).child(mask)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Context, TestAppContext, px, size};

    use super::*;

    struct Host;

    impl Render for Host {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().p(px(8.0)).child(
                Island::new()
                    .debug_selector(|| "island".into())
                    .w(px(290.0))
                    .h(px(400.0))
                    .child(div().debug_selector(|| "child".into()).h(px(26.0))),
            )
        }
    }

    #[gpui::test]
    fn island_lays_out_its_child_inside_the_size_it_was_given(cx: &mut TestAppContext) {
        let (_host, cx) = cx.add_window_view(|_, _| Host);
        cx.run_until_parked();

        let Some(island) = cx.debug_bounds("island") else {
            panic!("island was not laid out");
        };
        let Some(child) = cx.debug_bounds("child") else {
            panic!("the island's child is missing from the element tree");
        };

        assert_eq!(island.size, size(px(290.0), px(400.0)));
        assert_eq!(island.origin, gpui::point(px(8.0), px(8.0)));
        assert_eq!(child.origin, island.origin);
        assert_eq!(child.size, size(px(290.0), px(26.0)));
    }
}
