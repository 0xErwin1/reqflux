//! Domain types, validation, variable scopes and `{{}}` templating.
//!
//! This crate is pure: it never touches the filesystem, the network or GPUI.

pub mod model;
pub mod query;
pub mod template;
