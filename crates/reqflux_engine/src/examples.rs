//! Saved examples (v1-features S4): a request and the response it got, kept beside the
//! request in the workspace, and saving a response body to a file.
//!
//! An example keeps the request as written, `{{variables}}` unresolved, and the response
//! with every secret of the send masked: header values and a text body through the send's
//! [`Masker`], a `Set-Cookie` with its cookie value masked, since the file is shared. A body
//! that is not UTF-8 is kept as received in a body file beside the example.

use std::path::Path;

use reqflux_core::model::{
    Example, ExampleBody, ExampleRequest, ExampleResponse, Id, KeyValue, Request,
};
use reqflux_http::Response;

use crate::engine::Engine;
use crate::error::ExampleError;
use crate::sent::Masker;

/// A saved example of a request.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedExample {
    /// The file slug: the example is `<request-slug>.examples/<slug>.example.toml`.
    pub slug: String,
    /// The example document.
    pub example: Example,
    /// The response body bytes: the text, or the content of the body file.
    pub body: Vec<u8>,
}

impl Engine {
    /// The saved examples of the request `selector`, ordered by name.
    ///
    /// # Errors
    ///
    /// No request matches `selector`, or an example file is unreadable.
    pub fn examples(&self, selector: &str) -> Result<Vec<SavedExample>, ExampleError> {
        let request = self.example_owner(selector)?;
        Ok(reqflux_store::examples(&self.root, &self.loaded, request)?
            .into_iter()
            .map(|stored| SavedExample {
                slug: stored.slug,
                example: stored.example,
                body: stored.body,
            })
            .collect())
    }

    /// Saves `request` (the document as written, such as the draft that was sent) and the
    /// `response` it got as a new example of the request `selector` named `name` (numbered
    /// when taken), secrets masked with `masker`, the send's. Returns the example id.
    ///
    /// # Errors
    ///
    /// No request matches `selector`, the name is empty, or the write failed.
    pub fn save_example(
        &self,
        selector: &str,
        name: &str,
        request: &Request,
        response: &Response,
        masker: &Masker,
    ) -> Result<Id, ExampleError> {
        let owner = self.example_owner(selector)?;
        let (body, body_file) = match std::str::from_utf8(&response.body) {
            Ok("") => (ExampleBody::None, None),
            Ok(text) => (ExampleBody::Text(masker.mask(text)), None),
            Err(_) => (ExampleBody::None, Some(response.body.as_slice())),
        };
        let example = Example {
            id: Id::new(),
            name: name.to_owned(),
            request: ExampleRequest {
                method: request.method.clone(),
                url: request.url.clone(),
                params: request.params.clone(),
                headers: request.headers.clone(),
                auth: request.auth.clone(),
                body: request.body.clone(),
            },
            response: ExampleResponse {
                status: response.status,
                reason: response.reason.as_deref().map(|reason| masker.mask(reason)),
                headers: response
                    .headers
                    .iter()
                    .map(|(name, value)| {
                        let (name, value) = masker.header_for_agent(name, value);
                        KeyValue::new(name, value)
                    })
                    .collect(),
                body,
            },
            extensions: toml::Table::new(),
        };
        let id = example.id;
        reqflux_store::create_example(&self.root, &self.loaded, owner, example, body_file)?;
        Ok(id)
    }

    /// Renames the example `example` of the request `selector`; its file follows the name.
    ///
    /// # Errors
    ///
    /// No request or example matches, the name is empty, or the write failed.
    pub fn rename_example(
        &self,
        selector: &str,
        example: Id,
        name: &str,
    ) -> Result<(), ExampleError> {
        let owner = self.example_owner(selector)?;
        reqflux_store::rename_example(&self.root, &self.loaded, owner, example, name)?;
        Ok(())
    }

    /// Deletes the example `example` of the request `selector`.
    ///
    /// # Errors
    ///
    /// No request or example matches, or the delete failed.
    pub fn delete_example(&self, selector: &str, example: Id) -> Result<(), ExampleError> {
        let owner = self.example_owner(selector)?;
        reqflux_store::delete_example(&self.root, &self.loaded, owner, example)?;
        Ok(())
    }

    /// Writes `response`'s body, the bytes as received, to `path`, replacing the file.
    ///
    /// # Errors
    ///
    /// [`ExampleError::Write`] when the file cannot be written.
    pub fn save_response_body(&self, response: &Response, path: &Path) -> Result<(), ExampleError> {
        std::fs::write(path, &response.body).map_err(|source| ExampleError::Write {
            path: path.to_owned(),
            source,
        })
    }

    /// The id of the request `selector` names.
    fn example_owner(&self, selector: &str) -> Result<Id, ExampleError> {
        Ok(crate::select::find(self.workspace(), selector)?
            .request
            .request
            .id)
    }
}
