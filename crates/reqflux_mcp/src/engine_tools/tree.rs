//! The tree tools (S2): each goes through the engine's tree API by id and returns the
//! affected id. A refused operation is a tool error with the store's message.

use reqflux_core::model::{Id, Method};
use reqflux_engine::{Engine, TreeError};

use super::policy::Scope;
use super::request::find_request;
use super::{
    CollectionCreateArgs, EngineState, FolderCreateArgs, ItemArgs, MoveArgs, RenameArgs,
    RequestCreateArgs, id_result, parse_id,
};
use crate::tools::ToolError;

fn tree_error(error: TreeError) -> ToolError {
    ToolError::Engine(error.to_string())
}

/// `collection_create`: a new collection, named `name` when passed.
///
/// # Errors
///
/// The store refused the collection or the name; nothing is left behind.
pub fn collection_create(
    state: &mut EngineState,
    args: CollectionCreateArgs,
) -> Result<String, ToolError> {
    state.authorize("collection_create", Scope::Open, &[])?;
    let engine = state.reloaded()?;
    let id = engine.create_collection().map_err(tree_error)?;
    finish_create(engine, id, |engine| name(engine, id, args.name.as_deref()))
}

/// `folder_create`: a new folder at the end of `parent_id`, named `name` when passed.
///
/// # Errors
///
/// A bad id, or the store refused the folder or the name; nothing is left behind.
pub fn folder_create(state: &mut EngineState, args: FolderCreateArgs) -> Result<String, ToolError> {
    state.authorize("folder_create", Scope::Open, &[&args.parent_id])?;
    let parent = parse_id(&args.parent_id)?;
    let engine = state.reloaded()?;
    let id = engine.create_folder(parent).map_err(tree_error)?;
    finish_create(engine, id, |engine| name(engine, id, args.name.as_deref()))
}

/// `request_create`: a new request at the end of `parent_id`, with `method` and `url`
/// written and `name` given when passed.
///
/// # Errors
///
/// A bad id or method, or the store refused the request or the name; nothing is left
/// behind.
pub fn request_create(
    state: &mut EngineState,
    args: RequestCreateArgs,
) -> Result<String, ToolError> {
    state.authorize("request_create", Scope::Open, &[&args.parent_id])?;
    let parent = parse_id(&args.parent_id)?;
    let method = args
        .method
        .as_deref()
        .map(Method::new)
        .transpose()
        .map_err(|error| ToolError::Invalid(error.to_string()))?;
    let engine = state.reloaded()?;
    let id = engine.create_request(parent).map_err(tree_error)?;
    finish_create(engine, id, |engine| {
        if method.is_some() || args.url.is_some() {
            set_method_and_url(engine, id, method, args.url)?;
        }
        name(engine, id, args.name.as_deref())
    })
}

/// Runs `rest` on the entity `id` just created, deleting it again when `rest` fails so a
/// refused create leaves nothing behind.
fn finish_create(
    engine: &mut Engine,
    id: Id,
    rest: impl FnOnce(&mut Engine) -> Result<(), ToolError>,
) -> Result<String, ToolError> {
    if let Err(error) = rest(engine) {
        if let Err(cleanup) = engine.delete(id) {
            log::warn!(
                target: "reqflux_mcp",
                "could not delete {id} after a refused create: {cleanup}"
            );
        }
        return Err(error);
    }
    id_result(id)
}

fn name(engine: &mut Engine, id: Id, name: Option<&str>) -> Result<(), ToolError> {
    if let Some(name) = name {
        engine.rename(id, name).map_err(tree_error)?;
    }
    Ok(())
}

fn set_method_and_url(
    engine: &mut Engine,
    id: Id,
    method: Option<Method>,
    url: Option<String>,
) -> Result<(), ToolError> {
    let selector = id.to_string();
    let mut request = find_request(engine.workspace(), id)
        .map(|found| found.request.request.clone())
        .ok_or_else(|| ToolError::Engine(format!("the new request {id} did not load")))?;
    if let Some(method) = method {
        request.method = method;
    }
    if let Some(url) = url {
        request.url = url;
    }
    let base = engine
        .request_base_hash(&selector)
        .map_err(|error| ToolError::Engine(error.to_string()))?;
    engine
        .save_request(&selector, &request, base)
        .map_err(|error| ToolError::Engine(error.to_string()))?;
    engine
        .reload()
        .map_err(|error| ToolError::Engine(format!("the workspace does not load: {error}")))
}

/// `item_rename`: renames the collection, folder or request `id`.
///
/// # Errors
///
/// A bad id, or the store refused the name.
pub fn item_rename(state: &mut EngineState, args: RenameArgs) -> Result<String, ToolError> {
    state.authorize("item_rename", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    id_result(engine.rename(id, &args.name).map_err(tree_error)?)
}

/// `item_duplicate`: copies the folder or request `id` right after it; returns the copy's
/// id.
///
/// # Errors
///
/// A bad id, or the store refused the copy.
pub fn item_duplicate(state: &mut EngineState, args: ItemArgs) -> Result<String, ToolError> {
    state.authorize("item_duplicate", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    id_result(engine.duplicate(id).map_err(tree_error)?)
}

/// `item_move`: moves the folder or request `id` into `parent_id` at `index`.
///
/// # Errors
///
/// A bad id, or the store refused the move.
pub fn item_move(state: &mut EngineState, args: MoveArgs) -> Result<String, ToolError> {
    state.authorize("item_move", Scope::Open, &[&args.id, &args.parent_id])?;
    let id = parse_id(&args.id)?;
    let parent = parse_id(&args.parent_id)?;
    let engine = state.reloaded()?;
    id_result(
        engine
            .move_item(id, parent, args.index)
            .map_err(tree_error)?,
    )
}

/// `item_delete`: deletes the collection, folder or request `id` with everything under it.
///
/// # Errors
///
/// A bad id, or the store refused the delete.
pub fn item_delete(state: &mut EngineState, args: ItemArgs) -> Result<String, ToolError> {
    state.authorize("item_delete", Scope::Open, &[&args.id])?;
    let id = parse_id(&args.id)?;
    let engine = state.reloaded()?;
    engine.delete(id).map_err(tree_error)?;
    id_result(id)
}
