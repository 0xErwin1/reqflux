//! Postman collection v2.1 and environment import into the ReqFlux model, and export back
//! (v1-features S6).
//!
//! The crate maps JSON values only (S1): it never touches the filesystem. Reading the
//! documents belongs to the CLI and writing the workspace to `reqflux_engine`. The export
//! writes what it is given: clearing secret variable values is the engine's part.

mod collection;
mod environment;
mod error;
mod examples;
mod export;
mod slug;

pub use collection::{Imported, auth, collection, events, key_values, variables};
pub use environment::{environment, environment_in};
pub use error::PostmanError;
pub use export::{COLLECTION_SCHEMA, export_collection, export_environment};
pub use slug::{slug, unique};
