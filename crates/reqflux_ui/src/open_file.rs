//! Opening a file with the OS default handler (app-pending S3: the settings page's "Open
//! config.toml" and "Open file"). The handler is a seam: [`set_opener`] replaces it, so a
//! test records what would open instead of launching a program. Without a replacement the
//! app runs [`command`]; a test build refuses instead, so no test launches anything.

use std::io;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use gpui::{App, Global};

/// Opens a file, or says why it could not.
pub(crate) type Opener = Rc<dyn Fn(&Path) -> io::Result<()>>;

struct FileOpener(Opener);

impl Global for FileOpener {}

/// Replaces how files open.
#[cfg_attr(not(test), expect(dead_code, reason = "only tests replace the opener"))]
pub(crate) fn set_opener(cx: &mut App, opener: Opener) {
    cx.set_global(FileOpener(opener));
}

/// Opens `path` with the installed opener, else the OS default handler.
pub(crate) fn open(path: &Path, cx: &App) -> io::Result<()> {
    match cx.try_global::<FileOpener>() {
        Some(opener) => (opener.0)(path),
        None => system_open(path),
    }
}

#[cfg(not(test))]
fn system_open(path: &Path) -> io::Result<()> {
    let mut child = command(path).spawn()?;
    // The handler returns at once; reap it off the UI thread.
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            log::warn!("the file opener did not finish: {error}");
        }
    });
    Ok(())
}

#[cfg(test)]
fn system_open(_path: &Path) -> io::Result<()> {
    Err(io::Error::other("tests never launch the OS file handler"))
}

/// The command that opens `path` with the OS default handler: `xdg-open` on Linux and the
/// BSDs, `open` on macOS, `cmd /C start` on Windows.
pub(crate) fn command(path: &Path) -> Command {
    if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(path);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    }
}

#[cfg(test)]
#[path = "open_file_tests.rs"]
mod tests;
