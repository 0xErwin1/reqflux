//! A stored request as code (v1-features S2).

use reqflux_core::codegen::{self, Language};
use reqflux_core::model::Workspace;

use crate::engine::Engine;
use crate::error::ExecuteError;

/// The code that sends the request `selector` (an id or slug path) in `language`, as written:
/// with the effective auth of its folders or collection, and every `{{variable}}`, secret or
/// not, left unresolved. Nothing is read from the keyring or the app state.
///
/// # Errors
///
/// [`ExecuteError::UnknownRequest`] when no request has this id or slug path.
pub fn code(
    workspace: &Workspace,
    selector: &str,
    language: Language,
) -> Result<String, ExecuteError> {
    let target = crate::select::find(workspace, selector)?;
    let mut request = target.request.request.clone();
    request.auth = target.effective_auth().clone();
    Ok(codegen::generate(&request, language))
}

impl Engine {
    /// [`code`] on the loaded workspace.
    ///
    /// # Errors
    ///
    /// [`ExecuteError::UnknownRequest`] when no request has this id or slug path.
    pub fn code(&self, selector: &str, language: Language) -> Result<String, ExecuteError> {
        code(self.workspace(), selector, language)
    }
}
