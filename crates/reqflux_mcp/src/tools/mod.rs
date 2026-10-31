//! The `ui_*` tool handlers over the bridge session.
//!
//! Handlers are plain async functions over [`ServerState`] returning the JSON
//! text result, so they can be tested against the fake bridge without rmcp;
//! the server in [`crate::server`] maps [`ToolError`] to an `isError` result.

#[cfg(test)]
mod tests;

use std::str::FromStr as _;
use std::time::Duration;

use gpui_mcp_protocol::{
    BridgeResult, InputCommand, MouseButton, Operation, Point, PointerCommand, PointerScrollDelta,
    Rect, Role, UiNode, UiTree,
};
use schemars::JsonSchema;
use serde::Deserialize;
use thiserror::Error;

use crate::bridge::{BridgeClient, BridgeSession, CallError, SessionError, TargetId};

/// The session one MCP server drives: the bridge target choice lives here.
#[derive(Debug)]
pub struct ServerState {
    session: BridgeSession,
}

impl ServerState {
    /// A state that looks for `app_id` targets through `session`.
    #[must_use]
    pub fn new(session: BridgeSession) -> Self {
        Self { session }
    }
}

/// Why a tool call failed. The message becomes the tool's `isError` text.
#[derive(Debug, Error)]
pub enum ToolError {
    /// The session could not pick a target.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The bridge refused or failed the operation, or the transport broke.
    #[error(transparent)]
    Bridge(#[from] CallError),
    /// A tool result could not be encoded as JSON.
    #[error("could not encode the tool result: {0}")]
    Json(#[from] serde_json::Error),
    /// The arguments name something the tree does not have.
    #[error("{0}")]
    Invalid(String),
    /// No element matched the query within the wait budget.
    #[error("no element matching the query appeared within {timeout_ms} ms")]
    Timeout {
        /// The wait budget that elapsed.
        timeout_ms: u64,
    },
    /// An engine tool needs a workspace and none is open.
    #[error("no workspace is open; call workspace_open or workspace_create first")]
    NoWorkspace,
    /// The engine, the store or the app state refused or failed the operation.
    #[error("{0}")]
    Engine(String),
    /// The MCP policy refused the call, or its audit line could not be written.
    #[error("{0}")]
    Refused(String),
    /// The bridge answered with a result the tool cannot use.
    #[error("the bridge returned {0} where the tool expected another result")]
    UnexpectedResult(&'static str),
}

/// Arguments of `ui_select_target`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SelectTargetArgs {
    /// Target id from `ui_list_targets`.
    pub target_id: String,
}

/// Arguments of `ui_find`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindArgs {
    /// Label or id substring; the whole label when `exact` is true.
    pub query: Option<String>,
    /// Role name, matched case-insensitively, such as `button` or `text_input`.
    pub role: Option<String>,
    /// Require the whole label to equal the query, case-sensitively.
    pub exact: Option<bool>,
    /// Skip invisible elements; true unless false is passed.
    pub visible_only: Option<bool>,
    /// Maximum matches; 50 unless passed, capped at 200.
    pub limit: Option<u16>,
}

/// Arguments of `ui_element`, `ui_element_bounds` and `ui_focus`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdArgs {
    /// Stable element id from the tree.
    pub id: String,
}

/// Arguments of `ui_wait_for_element`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitArgs {
    /// Label or id substring; the whole label when `exact` is true.
    pub query: Option<String>,
    /// Role name, matched case-insensitively.
    pub role: Option<String>,
    /// Require the whole label to equal the query, case-sensitively.
    pub exact: Option<bool>,
    /// Wait budget in milliseconds; 5000 unless passed.
    pub timeout_ms: Option<u64>,
}

/// Arguments of `ui_click`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClickArgs {
    /// Element to click at its center.
    pub id: String,
    /// `left`, `right` or `middle`; `left` unless passed.
    pub button: Option<String>,
    /// Click count from 1 to 3; 1 unless passed.
    pub count: Option<u8>,
}

/// Arguments of `ui_type`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TextArgs {
    /// Text to type into the focused field.
    pub text: String,
}

/// Arguments of `ui_key`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct KeyArgs {
    /// GPUI keystroke, such as `enter` or `ctrl-a`.
    pub keystroke: String,
}

/// Arguments of `ui_scroll`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScrollArgs {
    /// Element to scroll at its center, instead of `x` and `y`.
    pub id: Option<String>,
    /// Window-relative x coordinate, with `y`, when `id` is absent.
    pub x: Option<f32>,
    /// Window-relative y coordinate, with `x`, when `id` is absent.
    pub y: Option<f32>,
    /// Horizontal pixel delta; 0 unless passed.
    pub delta_x: Option<f32>,
    /// Vertical pixel delta; 0 unless passed.
    pub delta_y: Option<f32>,
}

/// Default and maximum number of `ui_find` matches.
const FIND_DEFAULT_LIMIT: u16 = 50;
const FIND_MAX_LIMIT: u16 = 200;

/// How long `ui_wait_for_element` waits by default.
const WAIT_DEFAULT_MS: u64 = 5_000;

/// How often the wait polls the tree.
const WAIT_POLL_MS: u64 = 50;

/// List every live target, each marked when it is the session's (S3).
///
/// # Errors
///
/// Returns a [`ToolError`] when discovery fails.
pub async fn list_targets(state: &mut ServerState) -> Result<String, ToolError> {
    let targets = state.session.list_targets().await?;
    Ok(serde_json::to_string(
        &serde_json::json!({ "targets": targets }),
    )?)
}

/// Select the target the session's tools talk to (S4).
///
/// # Errors
///
/// Returns a [`ToolError`] for an unknown target or a discovery failure.
pub async fn select_target(
    state: &mut ServerState,
    args: SelectTargetArgs,
) -> Result<String, ToolError> {
    let target_id = TargetId::from_str(&args.target_id)
        .map_err(|_| ToolError::Invalid(format!("{} is not a target id", args.target_id)))?;
    state.session.select_target(&target_id).await?;
    let summary = state
        .session
        .list_targets()
        .await?
        .into_iter()
        .find(|summary| summary.target_id == target_id)
        .ok_or_else(|| ToolError::Invalid(format!("no live target has the id {target_id}")))?;
    Ok(serde_json::to_string(&summary)?)
}

/// Ping the selected target.
///
/// # Errors
///
/// Returns a [`ToolError`] when no target is selected or the call fails.
pub async fn ping(state: &mut ServerState) -> Result<String, ToolError> {
    let client = state.session.client().await?;
    match client.call(Operation::Ping).await? {
        BridgeResult::Pong {
            app_id,
            pid,
            protocol_version: _,
        } => Ok(serde_json::to_string(&serde_json::json!({
            "ok": true,
            "app_id": app_id.as_str(),
            "pid": pid.get(),
        }))?),
        _ => Err(ToolError::UnexpectedResult("a pong")),
    }
}

/// Return the latest semantic tree with its generation, as the bridge sent it.
///
/// # Errors
///
/// Returns a [`ToolError`] when no target is selected or the call fails.
pub async fn tree(state: &mut ServerState) -> Result<String, ToolError> {
    let text = serde_json::to_string(&fetch_tree(state).await?)?;
    Ok(text)
}

/// Find elements by label or id substring and role, newest generation only.
///
/// # Errors
///
/// Returns a [`ToolError`] when no target is selected or the call fails.
pub async fn find(state: &mut ServerState, args: FindArgs) -> Result<String, ToolError> {
    let tree = fetch_tree(state).await?;
    let matches = matches(
        &tree,
        &args.query,
        &args.role,
        &args.exact,
        &args.visible_only,
    );
    let limit = limit(args.limit);
    let elements: Vec<_> = matches
        .into_iter()
        .take(usize::from(limit))
        .map(element_summary)
        .collect();
    Ok(serde_json::to_string(&serde_json::json!({
        "count": elements.len(),
        "elements": elements,
    }))?)
}

/// Return one element with its parent, children, state, text and value.
///
/// # Errors
///
/// Returns a [`ToolError`] for an unknown id or a failing call.
pub async fn element(state: &mut ServerState, args: IdArgs) -> Result<String, ToolError> {
    let tree = fetch_tree(state).await?;
    let node = lookup(&tree, &args.id)?;
    Ok(serde_json::to_string(&without_redacted_text(node))?)
}

/// A node with the text the bridge marked redacted blanked out.
///
/// The bridge already withholds those values, so this is defence in depth: a bridge that
/// changes its mind, or a node built by something else, still cannot leak a secret through a
/// tool.
fn without_redacted_text(node: &UiNode) -> serde_json::Value {
    let mut value = serde_json::to_value(node).unwrap_or(serde_json::Value::Null);
    for key in ["text", "value"] {
        let Some(field) = value.get_mut(key).and_then(|field| field.as_object_mut()) else {
            continue;
        };
        if field.get("redacted").and_then(serde_json::Value::as_bool) != Some(true) {
            continue;
        }
        for secret in ["text", "value"] {
            if let Some(slot) = field.get_mut(secret) {
                *slot = serde_json::Value::String(String::new());
            }
        }
    }
    value
}

/// Return one element's bounds.
///
/// # Errors
///
/// Returns a [`ToolError`] for an unknown id, missing bounds or a failing
/// call.
pub async fn element_bounds(state: &mut ServerState, args: IdArgs) -> Result<String, ToolError> {
    let tree = fetch_tree(state).await?;
    let node = lookup(&tree, &args.id)?;
    let bounds = bounds(node, &args.id)?;
    Ok(serde_json::to_string(&serde_json::json!({
        "id": args.id,
        "bounds": bounds,
    }))?)
}

/// Poll the tree until a visible element matches, then return the match.
///
/// # Errors
///
/// Returns [`ToolError::Timeout`] with a clear message when the budget
/// elapses, and a [`ToolError`] for failing calls.
pub async fn wait_for_element(
    state: &mut ServerState,
    args: WaitArgs,
) -> Result<String, ToolError> {
    let timeout_ms = args.timeout_ms.unwrap_or(WAIT_DEFAULT_MS);
    let started = tokio::time::Instant::now();
    loop {
        let tree = fetch_tree(state).await?;
        let visible_only = Some(true);
        let matches = matches(&tree, &args.query, &args.role, &args.exact, &visible_only);
        if let Some(node) = matches.first() {
            return Ok(serde_json::to_string(&serde_json::json!({
                "elapsed_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "element": element_summary(node),
            }))?);
        }
        if started.elapsed() >= tokio::time::Duration::from_millis(timeout_ms) {
            return Err(ToolError::Timeout { timeout_ms });
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(WAIT_POLL_MS)).await;
    }
}

/// Click an element at its center through the pointer pipeline.
///
/// # Errors
///
/// Returns a [`ToolError`] for an unknown id, missing bounds, a bad button or
/// count, or a failing call.
pub async fn click(state: &mut ServerState, args: ClickArgs) -> Result<String, ToolError> {
    let button = parse_button(args.button.as_deref())?;
    let count = args.count.unwrap_or(1);
    if !(1..=3).contains(&count) {
        return Err(ToolError::Invalid(format!(
            "click count must be between 1 and 3, got {count}"
        )));
    }
    let tree = fetch_tree(state).await?;
    let node = lookup(&tree, &args.id)?;
    let center = bounds(node, &args.id)?.center();

    let client = state.session.client().await?;
    for click_count in 1..=count {
        for command in [
            PointerCommand::MouseDown {
                point: center,
                button,
                click_count,
            },
            PointerCommand::MouseUp {
                point: center,
                button,
                click_count,
            },
        ] {
            expect_ack(
                client.call(Operation::PointerInput { command }).await?,
                "ack",
            )?;
        }
    }
    let settled = settle(&client).await;
    Ok(ack("clicked", settled))
}

/// Type text into the focused field; the bridge refuses redacted ones (S7).
///
/// # Errors
///
/// Returns a [`ToolError`] with the bridge's message when it refuses.
pub async fn type_text(state: &mut ServerState, args: TextArgs) -> Result<String, ToolError> {
    let client = state.session.client().await?;
    expect_ack(
        client
            .call(Operation::Input {
                command: InputCommand::TypeText { text: args.text },
            })
            .await?,
        "ack",
    )?;
    let settled = settle(&client).await;
    Ok(ack("typed", settled))
}

/// Dispatch one GPUI keystroke.
///
/// # Errors
///
/// Returns a [`ToolError`] when no target is selected or the call fails.
pub async fn key(state: &mut ServerState, args: KeyArgs) -> Result<String, ToolError> {
    let client = state.session.client().await?;
    expect_ack(
        client
            .call(Operation::Input {
                command: InputCommand::Key {
                    keystroke: args.keystroke,
                },
            })
            .await?,
        "ack",
    )?;
    let settled = settle(&client).await;
    Ok(ack("key", settled))
}

/// Move focus to one element.
///
/// # Errors
///
/// Returns a [`ToolError`] when no target is selected or the call fails.
pub async fn focus(state: &mut ServerState, args: IdArgs) -> Result<String, ToolError> {
    let client = state.session.client().await?;
    expect_ack(
        client.call(Operation::Focus { node_id: args.id }).await?,
        "ack",
    )?;
    let settled = settle(&client).await;
    Ok(ack("focused", settled))
}

/// Scroll at an element's center or at coordinates.
///
/// # Errors
///
/// Returns a [`ToolError`] when neither an id nor both coordinates are given,
/// or when the call fails.
pub async fn scroll(state: &mut ServerState, args: ScrollArgs) -> Result<String, ToolError> {
    let tree = fetch_tree(state).await?;
    let point = match (&args.id, args.x, args.y) {
        (Some(id), _, _) => {
            let node = lookup(&tree, id)?;
            bounds(node, id)?.center()
        }
        (None, Some(x), Some(y)) => Point { x, y },
        _ => {
            return Err(ToolError::Invalid(
                "provide either id or both x and y".to_owned(),
            ));
        }
    };
    let client = state.session.client().await?;
    expect_ack(
        client
            .call(Operation::PointerInput {
                command: PointerCommand::ScrollWheel {
                    point,
                    delta: PointerScrollDelta::Pixels {
                        delta_x: args.delta_x.unwrap_or(0.0),
                        delta_y: args.delta_y.unwrap_or(0.0),
                    },
                },
            })
            .await?,
        "ack",
    )?;
    let settled = settle(&client).await;
    Ok(ack("scrolled", settled))
}

/// How long an input may take to reach a completed frame before the tool stops waiting.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

/// How many frames the UI may draw before an input counts as settled.
const MAX_SETTLE_FRAMES: usize = 8;

/// Wait until the app has drawn the frames an input queued, as upstream's server does:
/// without it a tool answers before the app has processed the click or the keystroke, so
/// the next read still sees the state from before the input.
///
/// Reports whether the UI settled. A failure here never fails the tool: the input was
/// already delivered, and an error would invite a retry that duplicates the click or the
/// text.
async fn settle(client: &BridgeClient) -> bool {
    for _ in 0..MAX_SETTLE_FRAMES {
        let pending = match client.call(Operation::GetPendingFrame).await {
            Ok(BridgeResult::PendingFrame(pending)) => pending,
            Ok(_) | Err(_) => return false,
        };
        if !pending.pending {
            return true;
        }
        let timeout_ms = u64::try_from(SETTLE_TIMEOUT.as_millis()).unwrap_or(0);
        match client
            .call(Operation::WaitForFrame {
                after_frame_count: pending.completed.frame_count,
                timeout_ms,
            })
            .await
        {
            Ok(BridgeResult::FrameStats(_)) => {}
            Ok(_) | Err(_) => return false,
        }
    }
    false
}

/// The JSON text of a successful input action, saying whether the UI settled afterwards.
fn ack(action: &str, settled: bool) -> String {
    serde_json::to_string(&serde_json::json!({
        "ok": true,
        "action": action,
        "settled": settled,
    }))
    .unwrap_or_else(|_| {
        format!("{{\"ok\": true, \"action\": \"{action}\", \"settled\": {settled}}}")
    })
}

/// Require the bridge's result to be a plain acknowledgement.
fn expect_ack(result: BridgeResult, expected: &'static str) -> Result<(), ToolError> {
    match result {
        BridgeResult::Ack => Ok(()),
        _ => Err(ToolError::UnexpectedResult(expected)),
    }
}

/// The find limit, capped at [`FIND_MAX_LIMIT`].
fn limit(requested: Option<u16>) -> u16 {
    requested.unwrap_or(FIND_DEFAULT_LIMIT).min(FIND_MAX_LIMIT)
}

/// A node's bounds, or an error naming the element.
fn bounds(node: &UiNode, id: &str) -> Result<Rect, ToolError> {
    node.bounds.ok_or_else(|| {
        ToolError::Invalid(format!("element {id} has no bounds in the current frame"))
    })
}

/// The mouse button a click asked for.
fn parse_button(button: Option<&str>) -> Result<MouseButton, ToolError> {
    match button.unwrap_or("left").to_ascii_lowercase().as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        other => Err(ToolError::Invalid(format!(
            "button must be left, right or middle, got {other}"
        ))),
    }
}

/// The compact summary `ui_find` and `ui_wait_for_element` return.
fn element_summary(node: &UiNode) -> serde_json::Value {
    serde_json::json!({
        "id": node.id,
        "label": node.label,
        "role": node.role,
        "bounds": node.bounds,
        "state": node.state,
    })
}

/// Fetch the latest tree from the selected target.
async fn fetch_tree(state: &mut ServerState) -> Result<UiTree, ToolError> {
    let client = state.session.client().await?;
    match client.call(Operation::GetTree).await? {
        BridgeResult::Tree(tree) => Ok(tree),
        _ => Err(ToolError::UnexpectedResult("a tree")),
    }
}

/// One node of `tree` by id.
fn lookup<'a>(tree: &'a UiTree, id: &str) -> Result<&'a UiNode, ToolError> {
    tree.nodes
        .get(id)
        .ok_or_else(|| ToolError::Invalid(format!("no element with id {id}")))
}

/// The nodes of `tree` that pass every filter of the query, in tree order.
fn matches<'a>(
    tree: &'a UiTree,
    query: &Option<String>,
    role: &Option<String>,
    exact: &Option<bool>,
    visible_only: &Option<bool>,
) -> Vec<&'a UiNode> {
    let exact = exact.unwrap_or(false);
    let visible_only = visible_only.unwrap_or(true);
    tree.nodes
        .values()
        .filter(|node| is_visible(node, visible_only))
        .filter(|node| role_matches(node.role, role))
        .filter(|node| text_matches(node, query, exact))
        .collect()
}

/// The visibility filter of a find query.
fn is_visible(node: &UiNode, visible_only: bool) -> bool {
    !visible_only || node.state.visible
}

/// Whether a node's role matches the query, case-insensitively.
fn role_matches(node_role: Role, query: &Option<String>) -> bool {
    match query {
        None => true,
        Some(role) => {
            let name = serde_json::to_value(node_role)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            name.eq_ignore_ascii_case(role)
        }
    }
}

/// Whether a node's label or id matches the text query.
fn text_matches(node: &UiNode, query: &Option<String>, exact: bool) -> bool {
    let Some(query) = query else {
        return true;
    };
    let label = node.label.as_deref().unwrap_or_default();
    if exact {
        label == query
    } else {
        let query = query.to_lowercase();
        label.to_lowercase().contains(&query) || node.id.to_lowercase().contains(&query)
    }
}
