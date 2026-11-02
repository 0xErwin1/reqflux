//! The on-disk workspace format: load, save, atomic writes and conflict detection.
//!
//! [`load`] reads a workspace directory into a [`Workspace`](reqflux_core::model::Workspace)
//! tree, with non-fatal [`Warning`]s and the [`ContentHash`] of every file it read. The
//! `save_*` functions write one entity file each, atomically, and refuse to overwrite a file
//! whose bytes no longer match the hash the caller loaded. The tree operations
//! ([`create_request`], [`rename`], [`duplicate`], [`delete`], [`move_item`], [`reorder`],
//! ...) change the collection tree by id against a [`Loaded`] workspace, and
//! [`create_environment`], [`rename_environment`] and [`delete_environment`] the
//! environments. [`plan_variables`] edits the variables of one scope.

mod error;
mod hash;
mod layout;
mod load;
mod save;
mod tree_ops;
mod variables;

pub use error::{StoreError, Warning, WarningKind, toml_error_location};
pub use hash::{ContentHash, ParseHashError};
pub use layout::{
    Parent, RESERVED_REQUEST_SLUGS, ScriptFiles, ScriptOwner, checked_collection_file,
    checked_environment_file, checked_folder_file, checked_request_file, is_reserved_request_slug,
    manifest_file,
};
pub use load::{Loaded, load};
pub use save::{
    ScriptHashes, save_collection, save_environment, save_folder, save_manifest, save_request,
    save_scripts,
};
pub use tree_ops::{
    Placed, StoredExample, create_collection, create_environment, create_example,
    create_example_at, create_folder, create_request, delete, delete_environment, delete_example,
    duplicate, example_files_at, examples, move_item, rename, rename_environment, rename_example,
    reorder,
};
pub use variables::{ScopeFile, VariableChange, VariableWrite, plan_variables, scope_variables};

/// The workspace format this version of ReqFlux reads and writes.
pub const FORMAT: u32 = 1;
