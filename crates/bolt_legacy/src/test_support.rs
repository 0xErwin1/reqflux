//! Helpers shared by the unit tests.

use gpui::{Hsla, Rgba};

/// `0xRRGGBB` and the alpha of `color`, rounded to 8 bits per channel.
pub(crate) fn rgba_of(color: Hsla) -> (u32, f32) {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value * 255.0).round() as u32;
    (
        (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b),
        rgba.a,
    )
}

/// Records the accessibility tree of every frame a window draws, so a test can read what
/// UI automation sees.
#[derive(Default)]
pub(crate) struct A11yRecorder {
    frame: std::sync::Mutex<Option<std::sync::Arc<gpui::AccessibilityFrame>>>,
}

impl gpui::FrameObserver for A11yRecorder {
    fn accessibility_frame(&self, frame: &std::sync::Arc<gpui::AccessibilityFrame>) {
        if let Ok(mut last) = self.frame.lock() {
            *last = Some(frame.clone());
        }
    }
}

impl A11yRecorder {
    /// Starts recording the window of `cx` and draws one frame.
    pub(crate) fn observe(cx: &mut gpui::VisualTestContext) -> std::sync::Arc<Self> {
        let recorder = std::sync::Arc::new(Self::default());
        cx.update(|window, _| window.observe_frames(&recorder));
        cx.run_until_parked();
        recorder
    }

    /// The accessible label of the element with id `id` in the last frame.
    pub(crate) fn label_of(&self, id: &str) -> Option<String> {
        let last = self.frame.lock().ok()?;
        let frame = last.as_ref()?;
        let (_, node) = frame.nodes().find(|(_, node)| node.id() == id)?;
        frame
            .accessibility_node(node)
            .and_then(|node| node.label().map(str::to_owned))
    }
}
