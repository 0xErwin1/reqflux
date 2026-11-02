//! `postman_export`, `bundle_export`, `docs_generate` and `bundle_import` (v1-features S6).
//!
//! The exports are reads: they return the document in the result and write no file, so an
//! agent never writes outside the workspace through them; it saves the result where it
//! wants with its own tools. Secret values never reach a result: a Postman export writes
//! secret variables empty and a bundle leaves them out (no passphrase is taken over MCP).
//! `bundle_import` is a write into the open workspace, with the same rule as the CLI: nothing
//! is overwritten, and encrypted secrets in the bundle are skipped.

use reqflux_engine::{BundleImport, BundleSecrets, MemorySecretStore, PostmanKind, import_bundle};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::policy::Scope;
use super::{EngineState, encode};
use crate::tools::ToolError;

/// Arguments of `postman_export`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PostmanExportArgs {
    /// The id or slug path of a collection, folder or request, or `env:<slug>` (or an
    /// environment id) for an environment.
    pub target: String,
}

/// Arguments of `bundle_export`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BundleExportArgs {
    /// The id or slug path of a collection, folder or request, or `env:<slug>` for an
    /// environment; the whole workspace when absent.
    pub target: Option<String>,
}

/// Arguments of `bundle_import`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BundleImportArgs {
    /// The bundle file's text, as `bundle_export` returns it.
    pub bundle: String,
    /// The id or slug path of the collection or folder a folder or request bundle goes into.
    pub into: Option<String>,
}

/// Arguments of `docs_generate`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsArgs {
    /// The id or slug of the collection.
    pub collection: String,
}

fn engine_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::Engine(error.to_string())
}

/// `postman_export`: the Postman v2.1 collection or environment document of a unit, with
/// every secret variable empty and named in `empty_secrets`.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, nothing matches `target`, or an example
/// could not be read.
pub fn postman_export(
    state: &mut EngineState,
    args: PostmanExportArgs,
) -> Result<String, ToolError> {
    state.authorize("postman_export", Scope::Open, &[&args.target])?;
    let engine = state.reloaded()?;
    let export = engine
        .export_postman(&args.target, false)
        .map_err(engine_error)?;
    encode(&json!({
        "kind": match export.kind {
            PostmanKind::Collection => "collection",
            PostmanKind::Environment => "environment",
        },
        "name": export.name,
        "document": export.document,
        "empty_secrets": export.empty_secrets,
    }))
}

/// `bundle_export`: the native bundle of a unit or the whole workspace, as text, with the
/// secret variables it leaves out.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, nothing matches `target`, or a file could
/// not be read.
pub fn bundle_export(state: &mut EngineState, args: BundleExportArgs) -> Result<String, ToolError> {
    let targets: Vec<&str> = args.target.as_deref().into_iter().collect();
    state.authorize("bundle_export", Scope::Open, &targets)?;
    let engine = state.reloaded()?;
    let bundle = engine
        .bundle(args.target.as_deref(), BundleSecrets::Omit)
        .map_err(engine_error)?;
    let omitted: Vec<Value> = bundle
        .secrets_omitted
        .iter()
        .map(|(scope, key)| json!({ "scope": scope, "key": key }))
        .collect();
    encode(&json!({
        "kind": bundle.kind.name(),
        "name": bundle.name,
        "bundle": bundle.text,
        "secrets_omitted": omitted,
    }))
}

/// `docs_generate`: the Markdown and HTML docs of a collection, as files with their text.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, no collection matches, or an example could
/// not be read.
pub fn docs_generate(state: &mut EngineState, args: DocsArgs) -> Result<String, ToolError> {
    state.authorize("docs_generate", Scope::Open, &[&args.collection])?;
    let engine = state.reloaded()?;
    let docs = engine.docs(&args.collection).map_err(engine_error)?;
    let files: Vec<Value> = docs
        .files
        .iter()
        .map(|file| json!({ "name": file.name, "content": file.content }))
        .collect();
    encode(&json!({ "collection": docs.collection, "files": files }))
}

/// `bundle_import`: writes a bundle into the open workspace, never replacing a file, and
/// reloads it. Encrypted secrets are skipped.
///
/// # Errors
///
/// No workspace is open, the policy refuses it, the bundle does not read, `into` is missing
/// or unknown, a target file exists, or a write failed.
pub fn bundle_import(state: &mut EngineState, args: BundleImportArgs) -> Result<String, ToolError> {
    let targets: Vec<&str> = args.into.as_deref().into_iter().collect();
    state.authorize("bundle_import", Scope::Open, &targets)?;
    let root = state.reloaded()?.root().to_owned();
    let options = BundleImport {
        into: args.into.as_deref(),
        passphrase: None,
    };
    // No passphrase, so no secret is decrypted or stored: the store is never touched.
    let summary = import_bundle(&root, &args.bundle, &options, &MemorySecretStore::new())
        .map_err(engine_error)?;
    state.reloaded()?;
    encode(&json!({
        "kind": summary.kind.name(),
        "collections": summary.collections,
        "folders": summary.folders,
        "requests": summary.requests,
        "examples": summary.examples,
        "environments": summary.environments,
        "renewed_ids": summary.renewed_ids,
        "secrets_skipped": summary.secrets_skipped,
    }))
}
