//! Writing an export to a file (v1-features S6, v1-close S4): the one rule `reqflux export`,
//! `reqflux bundle` and the window's Export… follow, so a file is never replaced by surprise
//! and secret values in clear are never left readable by others.

use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::export::PostmanExport;

/// Why an export file was not written. No message carries the file's content.
#[derive(Debug, thiserror::Error)]
pub enum ExportFileError {
    /// The file exists and the write was not forced.
    #[error("{} already exists", path.display())]
    Exists {
        /// The file.
        path: PathBuf,
    },
    /// The file could not be created or written.
    #[error("could not write {}: {source}", path.display())]
    Write {
        /// The file.
        path: PathBuf,
        /// The IO error.
        source: io::Error,
    },
}

/// Writes `text` to `path`: a new file unless `force`, which replaces one; readable and
/// writable by its owner only (mode 0600 on unix) when `private`, as a file with secret
/// values in clear must be.
///
/// # Errors
///
/// The file exists and `force` is off; the file could not be created or written.
#[cfg_attr(
    not(unix),
    expect(unused_variables, reason = "file modes are unix only")
)]
pub fn write_export_file(
    path: &Path,
    text: &str,
    force: bool,
    private: bool,
) -> Result<(), ExportFileError> {
    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let write = |source| ExportFileError::Write {
        path: path.to_owned(),
        source,
    };
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            ExportFileError::Exists {
                path: path.to_owned(),
            }
        } else {
            write(error)
        }
    })?;
    // `mode` applies only to a file the open creates; a replaced file keeps its own mode,
    // so a private one is narrowed before any secret is written into it.
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(write)?;
    }
    file.write_all(text.as_bytes())
        .and_then(|()| file.flush())
        .map_err(write)
}

impl PostmanExport {
    /// The document as its file holds it: pretty JSON ending in a newline.
    ///
    /// # Errors
    ///
    /// The document cannot be written as JSON.
    pub fn text(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(&self.document)?;
        text.push('\n');
        Ok(text)
    }
}
