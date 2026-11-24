//! The shell opens files through `crate::open_file`, never GPUI's `open_with_system`.

use std::path::Path;

/// Installs a recording opener and returns what it records.
fn record_opens(cx: &mut gpui::App) -> std::rc::Rc<std::cell::RefCell<Vec<std::path::PathBuf>>> {
    let opened: std::rc::Rc<std::cell::RefCell<Vec<std::path::PathBuf>>> = std::rc::Rc::default();
    let record = opened.clone();
    crate::open_file::set_opener(
        cx,
        std::rc::Rc::new(move |path: &Path| {
            record.borrow_mut().push(path.to_owned());
            Ok(())
        }),
    );
    opened
}

// GPUI's `open_with_system` keeps the Vulkan driver pin in its child, so the export and the
// preview open through this module, which drops it.
#[gpui::test]
fn the_export_open_goes_through_the_file_opener(cx: &mut gpui::TestAppContext) {
    use super::export::{ExportHost as _, OsExportHost};
    cx.update(|cx| {
        let opened = record_opens(cx);
        OsExportHost.open(Path::new("/tmp/export.json"), cx);
        assert_eq!(
            *opened.borrow(),
            vec![Path::new("/tmp/export.json").to_owned()]
        );
    });
}

#[gpui::test]
fn the_default_preview_open_goes_through_the_file_opener(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let opened = record_opens(cx);
        let host = super::response::PreviewHost::default();
        (host.open)(Path::new("/tmp/page.html"), cx);
        assert_eq!(
            *opened.borrow(),
            vec![Path::new("/tmp/page.html").to_owned()]
        );
    });
}
