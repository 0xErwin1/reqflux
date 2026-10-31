//! The engine tools (mcp-engine-tools S1-S5, S7), called directly on temp workspaces with
//! an in-memory secret store and a temp app state.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqflux_engine::state::{AppState, HistoryOutcome, HistoryRecord, RecordedResponse};
use reqflux_engine::{MASK, MemorySecretStore, SentBody, SentRequest};
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, EngineState, EntryArgs, ExecuteArgs, FolderCreateArgs,
    HistoryListArgs, ItemArgs, MoveArgs, PathArgs, RenameArgs, RequestCreateArgs, UpdateArgs,
    WorkspaceCreateArgs,
};
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A server state over a temp app state, with no workspace open.
fn state(dir: &TempDir) -> EngineState {
    EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(dir.path().join("state.sqlite3")),
    )
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("not JSON ({error}): {text}"))
}

fn ok(result: Result<String, reqflux_mcp::tools::ToolError>) -> Value {
    match result {
        Ok(text) => json(&text),
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn err(result: Result<String, reqflux_mcp::tools::ToolError>) -> String {
    match result {
        Ok(text) => panic!("expected a tool error, got {text}"),
        Err(error) => error.to_string(),
    }
}

/// A state with a fresh workspace `Demo` created at `<dir>/ws` and opened.
fn with_workspace(dir: &TempDir) -> (EngineState, PathBuf) {
    let mut state = state(dir);
    let root = dir.path().join("ws");
    ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: root.display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    (state, root)
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().expect("an id").to_owned()
}

fn collection(state: &mut EngineState, name: &str) -> String {
    id_of(&ok(engine_tools::collection_create(
        state,
        CollectionCreateArgs {
            name: Some(name.to_owned()),
        },
    )))
}

fn request(state: &mut EngineState, parent: &str, name: &str, url: &str) -> String {
    id_of(&ok(engine_tools::request_create(
        state,
        RequestCreateArgs {
            parent_id: parent.to_owned(),
            name: Some(name.to_owned()),
            method: Some("post".to_owned()),
            url: Some(url.to_owned()),
        },
    )))
}

fn info(state: &mut EngineState) -> Value {
    ok(engine_tools::workspace_info(state))
}

fn names(info: &Value) -> Vec<String> {
    info["tree"]
        .as_array()
        .expect("a tree")
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn get(state: &mut EngineState, id: &str) -> Value {
    ok(engine_tools::request_get(
        state,
        ItemArgs { id: id.to_owned() },
    ))
}

/// The request file holding `needle`, found by walking the workspace.
fn request_file(root: &Path, needle: &str) -> PathBuf {
    fn walk(dir: &Path, needle: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, needle) {
                    return Some(found);
                }
            } else if std::fs::read_to_string(&path)
                .is_ok_and(|text| text.contains(needle) && text.contains("method"))
            {
                return Some(path);
            }
        }
        None
    }
    walk(root, needle).expect("the request file")
}

// S1

#[test]
fn a_tool_without_an_open_workspace_names_workspace_open() {
    let dir = TempDir::new().unwrap();
    let mut state = state(&dir);
    let message = err(engine_tools::workspace_info(&mut state));
    assert!(message.contains("workspace_open"), "{message}");
}

#[test]
fn workspace_create_opens_the_new_workspace_and_info_describes_it() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let created = collection(&mut state, "Api");
    let folder = id_of(&ok(engine_tools::folder_create(
        &mut state,
        FolderCreateArgs {
            parent_id: created.clone(),
            name: Some("Users".to_owned()),
        },
    )));
    let request = request(&mut state, &folder, "List", "http://x/users");

    let info = info(&mut state);
    assert_eq!(info["name"], "Demo");
    assert!(info["id"].as_str().is_some_and(|id| id.len() == 26));
    assert_eq!(info["path"], root.display().to_string());
    assert_eq!(info["warnings"], Value::Array(Vec::new()));
    let tree = info["tree"].as_array().unwrap();
    assert_eq!(tree.len(), 3);
    assert_eq!(tree[0]["kind"], "collection");
    assert_eq!(tree[0]["id"], created.as_str());
    assert_eq!(tree[0]["parent"], Value::Null);
    assert_eq!(tree[1]["kind"], "folder");
    assert_eq!(tree[1]["parent"], created.as_str());
    assert_eq!(tree[2]["kind"], "request");
    assert_eq!(tree[2]["id"], request.as_str());
    assert_eq!(tree[2]["parent"], folder.as_str());
    assert_eq!(tree[2]["method"], "POST");
}

#[test]
fn workspace_open_switches_and_a_bad_path_is_an_error_that_keeps_the_open_one() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let other = dir.path().join("other");
    reqflux_engine::create_workspace(&other, "Other").unwrap();

    let message = err(engine_tools::workspace_open(
        &mut state,
        PathArgs {
            path: dir.path().join("missing").display().to_string(),
        },
    ));
    assert!(!message.is_empty());
    assert_eq!(info(&mut state)["path"], root.display().to_string());

    let opened = ok(engine_tools::workspace_open(
        &mut state,
        PathArgs {
            path: other.display().to_string(),
        },
    ));
    assert_eq!(opened["name"], "Other");
    assert_eq!(info(&mut state)["name"], "Other");
}

#[test]
fn startup_prefers_the_flag_then_a_workspace_in_the_current_directory() {
    let dir = TempDir::new().unwrap();
    let flagged = dir.path().join("flagged");
    let current = dir.path().join("current");
    reqflux_engine::create_workspace(&flagged, "Flagged").unwrap();
    reqflux_engine::create_workspace(&current, "Current").unwrap();

    let mut by_flag = state(&dir);
    by_flag.open_startup(Some(&flagged), &current).unwrap();
    assert_eq!(info(&mut by_flag)["name"], "Flagged");

    let mut by_cwd = state(&dir);
    by_cwd.open_startup(None, &current).unwrap();
    assert_eq!(info(&mut by_cwd)["name"], "Current");

    let mut none = state(&dir);
    none.open_startup(None, dir.path()).unwrap();
    assert!(err(engine_tools::workspace_info(&mut none)).contains("workspace_open"));

    let mut bad_flag = state(&dir);
    assert!(
        bad_flag
            .open_startup(Some(&dir.path().join("nope")), &current)
            .is_err()
    );
}

#[test]
fn workspace_info_sees_a_change_made_on_disk_since_the_last_call() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    collection(&mut state, "Api");
    assert_eq!(names(&info(&mut state)), ["Api"]);
    let mut other =
        reqflux_engine::Engine::open_with_secrets(&root, Arc::new(MemorySecretStore::new()))
            .unwrap();
    other.create_collection().unwrap();
    assert_eq!(names(&info(&mut state)).len(), 2);
}

// S2

#[test]
fn tree_tools_create_rename_duplicate_move_and_delete_by_id() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    let other = collection(&mut state, "Other");
    let first = request(&mut state, &api, "First", "http://x/1");

    let renamed = ok(engine_tools::item_rename(
        &mut state,
        RenameArgs {
            id: first.clone(),
            name: "Renamed".to_owned(),
        },
    ));
    assert_eq!(id_of(&renamed), first);
    let copy = id_of(&ok(engine_tools::item_duplicate(
        &mut state,
        ItemArgs { id: first.clone() },
    )));
    assert_ne!(copy, first);
    let moved = ok(engine_tools::item_move(
        &mut state,
        MoveArgs {
            id: copy.clone(),
            parent_id: other.clone(),
            index: 0,
        },
    ));
    assert_eq!(id_of(&moved), copy);
    let tree = info(&mut state)["tree"].clone();
    let copy_node = tree
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == copy.as_str())
        .unwrap()
        .clone();
    assert_eq!(copy_node["parent"], other.as_str());

    let deleted = ok(engine_tools::item_delete(
        &mut state,
        ItemArgs { id: copy.clone() },
    ));
    assert_eq!(id_of(&deleted), copy);
    assert_eq!(names(&info(&mut state)), ["Api", "Renamed", "Other"]);
}

#[test]
fn a_refused_tree_operation_is_an_error_and_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    request(&mut state, &api, "One", "http://x/1");
    let two = request(&mut state, &api, "Two", "http://x/2");
    let before = info(&mut state);

    let message = err(engine_tools::item_rename(
        &mut state,
        RenameArgs {
            id: two,
            name: "One".to_owned(),
        },
    ));
    assert!(!message.is_empty());
    assert_eq!(info(&mut state), before);

    let message = err(engine_tools::item_delete(
        &mut state,
        ItemArgs {
            id: "not-an-id".to_owned(),
        },
    ));
    assert!(message.contains("not-an-id"), "{message}");
    assert_eq!(info(&mut state), before);
}

#[test]
fn a_collection_created_with_a_taken_name_is_removed_again() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = with_workspace(&dir);
    collection(&mut state, "Api");
    let before = info(&mut state);
    err(engine_tools::collection_create(
        &mut state,
        CollectionCreateArgs {
            name: Some("Api".to_owned()),
        },
    ));
    assert_eq!(info(&mut state), before);
}

// S3, S7

/// A request whose file holds a literal bearer token, returned with its id.
fn bearer_request(state: &mut EngineState, root: &Path, token: &str) -> String {
    let api = collection(state, "Api");
    let id = request(state, &api, "Secret", "http://x/s");
    let file = request_file(root, &id);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        format!("{text}\n[auth]\ntype = \"bearer\"\ntoken = \"{token}\"\n"),
    )
    .unwrap();
    id
}

#[test]
fn request_get_masks_a_literal_bearer_token_and_names_the_parent_path() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let id = bearer_request(&mut state, &root, "tok-literal-123");
    let got = get(&mut state, &id);
    let document = got["document"].as_str().unwrap();
    assert!(!got.to_string().contains("tok-literal-123"), "{got}");
    assert!(document.contains(MASK), "{document}");
    assert_eq!(got["parent_path"], "api");
    assert_eq!(got["id"], id.as_str());
}

#[test]
fn request_get_masks_a_basic_password_and_an_api_key_value() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    let id = request(&mut state, &api, "Basic", "http://x/b");
    let file = request_file(&root, &id);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        format!(
            "{text}\n[auth]\ntype = \"basic\"\nusername = \"ana\"\npassword = \"pw-literal-9\"\n"
        ),
    )
    .unwrap();
    let got = get(&mut state, &id);
    assert!(!got.to_string().contains("pw-literal-9"), "{got}");
    assert!(got["document"].as_str().unwrap().contains("ana"));

    std::fs::write(
        &file,
        format!("{text}\n[auth]\ntype = \"apikey\"\nkey = \"X-Key\"\nvalue = \"key-literal-7\"\n"),
    )
    .unwrap();
    let got = get(&mut state, &id);
    assert!(!got.to_string().contains("key-literal-7"), "{got}");
}

#[test]
fn request_update_with_the_mask_keeps_the_stored_secret() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let id = bearer_request(&mut state, &root, "tok-keep-456");
    let got = get(&mut state, &id);
    let base_hash = got["base_hash"].as_str().unwrap().to_owned();
    let document = got["document"]
        .as_str()
        .unwrap()
        .replace("http://x/s", "http://x/changed");

    let updated = ok(engine_tools::request_update(
        &mut state,
        UpdateArgs {
            id: id.clone(),
            document,
            base_hash,
        },
    ));
    assert_eq!(id_of(&updated), id);
    let stored = std::fs::read_to_string(request_file(&root, &id)).unwrap();
    assert!(stored.contains("tok-keep-456"), "{stored}");
    assert!(stored.contains("http://x/changed"), "{stored}");
    assert!(!stored.contains(MASK), "{stored}");
}

#[test]
fn request_update_after_a_change_on_disk_is_a_conflict_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    let id = request(&mut state, &api, "One", "http://x/1");
    let got = get(&mut state, &id);
    let base_hash = got["base_hash"].as_str().unwrap().to_owned();
    let document = got["document"]
        .as_str()
        .unwrap()
        .replace("http://x/1", "http://x/mine");
    let file = request_file(&root, &id);
    let theirs = std::fs::read_to_string(&file)
        .unwrap()
        .replace("http://x/1", "http://x/theirs");
    std::fs::write(&file, &theirs).unwrap();

    let message = err(engine_tools::request_update(
        &mut state,
        UpdateArgs {
            id: id.clone(),
            document,
            base_hash,
        },
    ));
    assert!(message.contains("changed on disk"), "{message}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
}

#[test]
fn request_update_refuses_another_id_and_a_bad_document() {
    let dir = TempDir::new().unwrap();
    let (mut state, root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    let id = request(&mut state, &api, "One", "http://x/1");
    let file = request_file(&root, &id);
    let before = std::fs::read_to_string(&file).unwrap();
    let other_id = reqflux_core::model::Id::new().to_string();
    let got = get(&mut state, &id);
    let base_hash = got["base_hash"].as_str().unwrap().to_owned();
    let document = got["document"].as_str().unwrap().replace(&id, &other_id);

    let message = err(engine_tools::request_update(
        &mut state,
        UpdateArgs {
            id: id.clone(),
            document,
            base_hash: base_hash.clone(),
        },
    ));
    assert!(message.contains(&id), "{message}");
    err(engine_tools::request_update(
        &mut state,
        UpdateArgs {
            id: id.clone(),
            document: "not = [toml".to_owned(),
            base_hash,
        },
    ));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

// S4, S7

fn mock_server(runtime: &tokio::runtime::Runtime, secret: &str) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("x-echo", format!("got {secret}").as_str())
                    .insert_header("set-cookie", "session=live-sess-41; Path=/; HttpOnly")
                    .set_body_string(format!("{{\"token\":\"{secret}\"}}")),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(300 * 1024)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/bytes"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xff, 0xfe, 0x00]))
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn request_execute_returns_the_response_with_secrets_masked_and_records_nothing() {
    let dir = TempDir::new().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = mock_server(&runtime, "tok-echo-789");
    let (mut state, root) = with_workspace(&dir);
    let id = bearer_request(&mut state, &root, "tok-echo-789");
    let file = request_file(&root, &id);
    let text = std::fs::read_to_string(&file)
        .unwrap()
        .replace("http://x/s", &format!("{}/echo", server.uri()));
    std::fs::write(&file, text).unwrap();

    let result = ok(engine_tools::request_execute(
        &mut state,
        ExecuteArgs {
            id: id.clone(),
            environment: None,
            vars: Some([("unused".to_owned(), "v".to_owned())].into()),
        },
    ));
    assert_eq!(result["status"], 201);
    assert_eq!(result["reason"], "Created");
    assert!(result["elapsed_ms"].is_u64());
    assert_eq!(result["truncated"], false);
    assert!(result["size"].as_u64().is_some_and(|size| size > 0));
    assert!(result["tests"].is_array());
    assert!(result["unresolved"].is_array());
    assert!(!result.to_string().contains("tok-echo-789"), "{result}");
    assert_eq!(
        result["body"].as_str().unwrap(),
        format!("{{\"token\":\"{MASK}\"}}")
    );
    let echo = result["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == "x-echo")
        .unwrap()
        .clone();
    assert_eq!(echo[1], format!("got {MASK}"));
    let cookie = result["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == "set-cookie")
        .unwrap()
        .clone();
    assert_eq!(cookie[1], format!("session={MASK}; Path=/; HttpOnly"));

    let history = ok(engine_tools::history_list(
        &mut state,
        HistoryListArgs { limit: None },
    ));
    assert_eq!(history["entries"], Value::Array(Vec::new()));
}

#[test]
fn request_execute_cuts_a_long_body_and_counts_bytes_that_are_not_text() {
    let dir = TempDir::new().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = mock_server(&runtime, "unused");
    let (mut state, _root) = with_workspace(&dir);
    let api = collection(&mut state, "Api");
    let big = request(&mut state, &api, "Big", &format!("{}/big", server.uri()));
    let bytes = request(
        &mut state,
        &api,
        "Bytes",
        &format!("{}/bytes", server.uri()),
    );

    let result = ok(engine_tools::request_execute(
        &mut state,
        ExecuteArgs {
            id: big,
            environment: None,
            vars: None,
        },
    ));
    assert_eq!(result["truncated"], true);
    assert_eq!(result["body"].as_str().unwrap().len(), 256 * 1024);
    assert_eq!(result["size"], 300 * 1024);

    let result = ok(engine_tools::request_execute(
        &mut state,
        ExecuteArgs {
            id: bytes,
            environment: None,
            vars: None,
        },
    ));
    assert_eq!(result["body"], Value::Null);
    assert_eq!(result["body_bytes"], 3);
}

#[test]
fn request_execute_of_an_unknown_request_is_an_error() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = with_workspace(&dir);
    let id = reqflux_core::model::Id::new().to_string();
    let message = err(engine_tools::request_execute(
        &mut state,
        ExecuteArgs {
            id: id.clone(),
            environment: None,
            vars: None,
        },
    ));
    assert!(message.contains(&id), "{message}");
}

// S5

fn record(app: &mut AppState, workspace_id: &str, name: &str) -> i64 {
    app.record_history(
        workspace_id,
        HistoryRecord {
            sent_at: 1,
            request_id: "r".to_owned(),
            request_name: name.to_owned(),
            request_path: format!("api/{name}"),
            document: "name = \"x\"".to_owned(),
            sent: SentRequest {
                method: "GET".to_owned(),
                url: "http://x/".to_owned(),
                headers: vec![("authorization".to_owned(), MASK.to_owned())],
                body: SentBody::None,
            },
            outcome: HistoryOutcome::Response(RecordedResponse {
                status: 200,
                reason: Some("OK".to_owned()),
                headers: Vec::new(),
                body: b"hello".to_vec(),
                truncated: false,
                elapsed_ms: 5,
                size: 5,
            }),
        },
    )
    .unwrap()
}

#[test]
fn history_reads_the_open_workspace_entries_only() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = with_workspace(&dir);
    let workspace_id = info(&mut state)["id"].as_str().unwrap().to_owned();
    let mut app = AppState::open(&dir.path().join("state.sqlite3")).unwrap();
    let first = record(&mut app, &workspace_id, "first");
    let second = record(&mut app, &workspace_id, "second");
    let foreign = record(&mut app, "01ARZ3NDEKTSV4RRFFQ69G5FAV", "foreign");
    drop(app);

    let list = ok(engine_tools::history_list(
        &mut state,
        HistoryListArgs { limit: None },
    ));
    let entries = list["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["entry_id"], second);
    assert_eq!(entries[0]["request_name"], "second");
    assert_eq!(entries[0]["status"], 200);
    let limited = ok(engine_tools::history_list(
        &mut state,
        HistoryListArgs { limit: Some(1) },
    ));
    assert_eq!(limited["entries"].as_array().unwrap().len(), 1);

    let entry = ok(engine_tools::history_get(
        &mut state,
        EntryArgs { entry_id: first },
    ));
    assert_eq!(entry["entry_id"], first);
    assert_eq!(entry["response"]["body"], "hello");
    assert_eq!(entry["request"]["headers"][0][1], MASK);

    let message = err(engine_tools::history_get(
        &mut state,
        EntryArgs { entry_id: foreign },
    ));
    assert!(message.contains("not found"), "{message}");
}

#[test]
fn history_fails_alone_when_the_app_state_cannot_open() {
    let dir = TempDir::new().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "x").unwrap();
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(blocker.join("state.sqlite3")),
    );
    let root = dir.path().join("ws");
    ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: root.display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    err(engine_tools::history_list(
        &mut state,
        HistoryListArgs { limit: None },
    ));
    info(&mut state);
}
