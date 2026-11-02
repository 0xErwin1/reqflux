//! Postman collection v2.1 and environment import into the ReqFlux model.
//!
//! The crate maps JSON values only (S1): it never touches the filesystem. Reading the
//! documents belongs to the CLI and writing the workspace to `reqflux_engine`.

mod collection;
mod environment;
mod error;
mod examples;
mod slug;

pub use collection::{Imported, auth, collection, events, key_values, variables};
pub use environment::{environment, environment_in};
pub use error::PostmanError;
pub use slug::{slug, unique};
