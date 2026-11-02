//! Domain types, validation, variable scopes and `{{}}` templating.
//!
//! This crate is pure: it never touches the filesystem, the network or GPUI.

pub mod codegen;
pub mod curl;
pub mod http_document;
pub mod jsonpath;
pub mod model;
pub mod query;
pub mod template;

#[cfg(test)]
mod http_document_tests;
