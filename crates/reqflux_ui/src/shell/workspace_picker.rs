//! The seam over the OS dialogs the workspace menu opens (recent-workspaces S3): the app asks
//! the platform, tests answer with preset paths, since no test can drive an OS dialog.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use gpui::{App, AsyncWindowContext, PathPromptOptions};

use super::toasts;

/// The path a dialog answered with, `Ok(None)` when it was dismissed, or the OS dialog's
/// error text when it failed.
pub(crate) type Picked = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// The path a dialog answered with: a failed dialog shows an error toast titled `title`
/// with the OS error, and it and a dismissed one give `None`.
pub(super) fn picked_path(
    answer: Result<Option<PathBuf>, String>,
    title: &str,
    cx: &mut AsyncWindowContext,
) -> Option<PathBuf> {
    answer.unwrap_or_else(|error| {
        if let Err(closed) = cx.update(|_, cx| toasts::failure(title.to_owned(), error, cx)) {
            log::debug!("the dialog failed after the window closed: {closed}");
        }
        None
    })
}

/// Asks the user for a workspace directory.
pub(crate) trait WorkspacePicker {
    /// An existing directory to open.
    fn pick_folder(&self, cx: &App) -> Picked;
    /// A new directory to create a workspace in.
    fn pick_new(&self, cx: &App) -> Picked;
}

/// The platform's dialogs.
pub(crate) struct OsPicker;

impl WorkspacePicker for OsPicker {
    fn pick_folder(&self, cx: &App) -> Picked {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open workspace".into()),
        });
        Box::pin(async move {
            match answer.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            }
        })
    }

    fn pick_new(&self, cx: &App) -> Picked {
        let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let answer = cx.prompt_for_new_path(&start, Some("workspace"));
        Box::pin(async move {
            match answer.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            }
        })
    }
}

/// Answers every dialog with a preset path, `None` meaning dismissed, or with `failure`, an
/// OS dialog error, when it is set.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestPicker {
    pub(crate) folder: Option<PathBuf>,
    pub(crate) new: Option<PathBuf>,
    pub(crate) failure: Option<String>,
}

#[cfg(test)]
impl WorkspacePicker for TestPicker {
    fn pick_folder(&self, _cx: &App) -> Picked {
        let answer = self.failure.clone().map_or(Ok(self.folder.clone()), Err);
        Box::pin(async move { answer })
    }

    fn pick_new(&self, _cx: &App) -> Picked {
        let answer = self.failure.clone().map_or(Ok(self.new.clone()), Err);
        Box::pin(async move { answer })
    }
}
