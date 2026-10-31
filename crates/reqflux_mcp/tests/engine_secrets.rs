//! Secrets in engine tool output (mcp-engine-tools S3, S7): load errors, history masking,
//! credential headers and the `base_hash` of `request_update`. Every state uses an
//! in-memory secret store and a temp app state, never the OS keyring.

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
    self, CollectionCreateArgs, EngineState, EntryArgs, HistoryListArgs, ItemArgs, PathArgs,
    RequestCreateArgs, UpdateArgs, VariableSetArgs, WorkspaceCreateArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::Value;
use tempfile::TempDir;

fn ok(result: Result<String, ToolError>) -> Value {
    match result {
        Ok(text) => {
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("not JSON ({error}): {text}"))
        }
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn err(result: Result<String, ToolError>) -> String {
    match result {
        Ok(text) => panic!("expected a tool error, got {text}"),
        Err(error) => error.to_string(),
    }
}

/// A state over `store` with a fresh workspace `Demo` at `<dir>/ws` open.
fn open(dir: &TempDir, store: Arc<MemorySecretStore>) -> (EngineState, PathBuf) {
    let mut state = EngineState::new(store, Some(dir.path().join("state.sqlite3")));
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

fn collection(state: &mut EngineState) -> String {
    id_of(&ok(engine_tools::collection_create(
        state,
        CollectionCreateArgs {
            name: Some("Api".to_owned()),
        },
    )))
}

fn request(state: &mut EngineState, parent: &str) -> String {
    id_of(&ok(engine_tools::request_create(
        state,
        RequestCreateArgs {
            parent_id: parent.to_owned(),
            name: Some("One".to_owned()),
            method: Some("get".to_owned()),
            url: Some("http://x/1".to_owned()),
        },
    )))
}

fn get(state: &mut EngineState, id: &str) -> Value {
    ok(engine_tools::request_get(
        state,
        ItemArgs { id: id.to_owned() },
    ))
}

fn update(
    state: &mut EngineState,
    id: &str,
    document: String,
    base_hash: &Value,
) -> Result<String, ToolError> {
    engine_tools::request_update(
        state,
        UpdateArgs {
            id: id.to_owned(),
            document,
            base_hash: base_hash.as_str().expect("a base hash").to_owned(),
        },
    )
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

/// A workspace with one request whose file ends with `tail`.
fn with_request(dir: &TempDir, tail: &str) -> (EngineState, PathBuf, String, PathBuf) {
    let (mut state, root) = open(dir, Arc::new(MemorySecretStore::new()));
    let api = collection(&mut state);
    let id = request(&mut state, &api);
    let file = request_file(&root, &id);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}\n{tail}")).unwrap();
    (state, root, id, file)
}

// B1: a parse error never quotes the source line.

#[test]
fn a_load_error_never_echoes_a_literal_secret() {
    for (tail, secret) in [
        (
            "[auth]\ntype = \"bearer\"\ntoken = \"pw-SECRET\\q9\"\n",
            "pw-SECRET",
        ),
        (
            "[auth]\ntype = \"bearer\"\ntoken = \"dup-SECRET-1\"\ntoken = \"dup-SECRET-2\"\n",
            "dup-SECRET",
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let (mut state, root, id, file) = with_request(&dir, tail);
        let messages = [
            err(engine_tools::workspace_info(&mut state)),
            err(engine_tools::request_get(
                &mut state,
                ItemArgs { id: id.clone() },
            )),
            err(engine_tools::workspace_open(
                &mut state,
                PathArgs {
                    path: root.display().to_string(),
                },
            )),
        ];
        for message in messages {
            assert!(!message.contains(secret), "{message}");
            assert!(message.contains(&file.display().to_string()), "{message}");
            assert!(message.contains("line "), "{message}");
            assert!(message.contains("column "), "{message}");
        }
    }
}

#[test]
fn a_bad_update_document_reports_line_and_column_only() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, id, _file) = with_request(&dir, "");
    let base = get(&mut state, &id)["base_hash"].clone();
    let message = err(update(
        &mut state,
        &id,
        "name = \"x\"\nsecret = \"upd-SECRET\\q\"\n".to_owned(),
        &base,
    ));
    assert!(!message.contains("upd-SECRET"), "{message}");
    assert!(message.contains("line 2"), "{message}");
}

// B3: request_update hash-checks against the agent's read.

#[test]
fn request_update_after_an_unrelated_reload_is_still_a_conflict() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, id, file) = with_request(&dir, "");
    let got = get(&mut state, &id);
    let document = got["document"]
        .as_str()
        .unwrap()
        .replace("http://x/1", "http://x/mine");
    let theirs = std::fs::read_to_string(&file)
        .unwrap()
        .replace("http://x/1", "http://x/theirs");
    std::fs::write(&file, &theirs).unwrap();
    // Any reloading tool between the read and the update.
    ok(engine_tools::workspace_info(&mut state));

    let message = err(update(&mut state, &id, document, &got["base_hash"]));
    assert!(message.contains("changed on disk"), "{message}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
}

#[test]
fn request_update_with_a_fresh_base_hash_saves_and_a_bad_one_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, id, file) = with_request(&dir, "");
    let got = get(&mut state, &id);
    let document = got["document"]
        .as_str()
        .unwrap()
        .replace("http://x/1", "http://x/mine");
    let before = std::fs::read_to_string(&file).unwrap();
    let message = err(update(
        &mut state,
        &id,
        document.clone(),
        &Value::from("not-a-hash"),
    ));
    assert!(message.contains("base_hash"), "{message}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    ok(update(&mut state, &id, document, &got["base_hash"]));
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("http://x/mine")
    );
}

#[test]
fn the_mask_never_restores_into_another_auth_type() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, id, file) = with_request(
        &dir,
        "[auth]\ntype = \"bearer\"\ntoken = \"tok-SECRET-x\"\n",
    );
    let got = get(&mut state, &id);
    let document = got["document"].as_str().unwrap().to_owned();
    let before = std::fs::read_to_string(&file).unwrap();
    for auth in [
        format!("[auth]\ntype = \"basic\"\nusername = \"u\"\npassword = \"{MASK}\"\n"),
        format!("[auth]\ntype = \"apikey\"\nkey = \"X-Key\"\nvalue = \"{MASK}\"\n"),
        format!("[auth]\ntype = \"oauth2\"\ntoken = \"{MASK}\"\n"),
    ] {
        let start = document.find("[auth]").expect("an auth table");
        let replaced = format!("{}{auth}", &document[..start]);
        let message = err(update(&mut state, &id, replaced, &got["base_hash"]));
        assert!(!message.contains("tok-SECRET-x"), "{message}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }
}

// Credential headers in documents.

#[test]
fn a_literal_authorization_header_is_masked_and_restored_from_the_mask() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root, id, file) = with_request(
        &dir,
        "[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer hdr-SECRET\"\n\n\
         [[headers]]\nkey = \"Cookie\"\nvalue = \"sid=ck-SECRET\"\n\n\
         [[headers]]\nkey = \"Accept\"\nvalue = \"text/plain\"\n",
    );
    let got = get(&mut state, &id);
    let text = got.to_string();
    assert!(!text.contains("hdr-SECRET"), "{text}");
    assert!(!text.contains("ck-SECRET"), "{text}");
    assert!(text.contains("text/plain"), "{text}");

    let document = got["document"]
        .as_str()
        .unwrap()
        .replace("http://x/1", "http://x/changed");
    ok(update(&mut state, &id, document, &got["base_hash"]));
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("Bearer hdr-SECRET"), "{stored}");
    assert!(stored.contains("sid=ck-SECRET"), "{stored}");
    assert!(stored.contains("http://x/changed"), "{stored}");
    assert!(!stored.contains(MASK), "{stored}");
}

// B2: history output is masked with the secrets in scope now.

fn response(headers: Vec<(String, String)>, body: &str) -> HistoryOutcome {
    HistoryOutcome::Response(RecordedResponse {
        status: 200,
        reason: Some("OK".to_owned()),
        headers,
        body: body.as_bytes().to_vec(),
        truncated: false,
        elapsed_ms: 5,
        size: body.len() as u64,
    })
}

fn record(dir: &TempDir, workspace_id: &str, document: &str, outcome: HistoryOutcome) -> i64 {
    let mut app = AppState::open(&dir.path().join("state.sqlite3")).unwrap();
    app.record_history(
        workspace_id,
        HistoryRecord {
            sent_at: 1,
            request_id: "r".to_owned(),
            request_name: "one".to_owned(),
            request_path: "api/one".to_owned(),
            document: document.to_owned(),
            sent: SentRequest {
                method: "GET".to_owned(),
                url: "http://x/".to_owned(),
                headers: vec![("Cookie".to_owned(), "sid=sent-cookie".to_owned())],
                body: SentBody::None,
            },
            outcome,
        },
    )
    .unwrap()
}

fn workspace_id(state: &mut EngineState) -> String {
    ok(engine_tools::workspace_info(state))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn history_get(state: &mut EngineState, entry_id: i64) -> Value {
    ok(engine_tools::history_get(state, EntryArgs { entry_id }))
}

#[test]
fn history_get_masks_the_recorded_document_auth() {
    let dir = TempDir::new().unwrap();
    let (mut state, _root) = open(&dir, Arc::new(MemorySecretStore::new()));
    let workspace = workspace_id(&mut state);
    let document = format!(
        "id = \"{}\"\nname = \"one\"\nmethod = \"GET\"\nurl = \"http://x/\"\n\n\
         [[headers]]\nkey = \"Authorization\"\nvalue = \"Basic hist-hdr-SECRET\"\n\n\
         [auth]\ntype = \"bearer\"\ntoken = \"hist-tok-SECRET\"\n",
        reqflux_core::model::Id::new()
    );
    let entry = record(&dir, &workspace, &document, response(Vec::new(), "ok"));
    let got = history_get(&mut state, entry);
    let text = got.to_string();
    assert!(got["document"].is_string(), "{text}");
    assert!(!text.contains("hist-tok-SECRET"), "{text}");
    assert!(!text.contains("hist-hdr-SECRET"), "{text}");
    assert!(!text.contains("sent-cookie"), "{text}");
}

const SECRET: &str = "resp-SECRET-v4lue";

/// A workspace whose collection declares the secret `token` with [`SECRET`] in `store`.
fn with_collection_secret(dir: &TempDir) -> (EngineState, String) {
    let store = Arc::new(MemorySecretStore::new());
    let (mut state, _root) = open(dir, store);
    let api = collection(&mut state);
    ok(engine_tools::variable_set(
        &mut state,
        VariableSetArgs {
            scope: format!("collection:{api}"),
            key: "token".to_owned(),
            value: Some(SECRET.to_owned()),
            secret: Some(true),
            enabled: None,
            description: None,
        },
    ));
    let workspace = workspace_id(&mut state);
    (state, workspace)
}

#[test]
fn history_get_masks_a_response_body_and_headers_with_the_secrets_in_scope() {
    let dir = TempDir::new().unwrap();
    let (mut state, workspace) = with_collection_secret(&dir);
    let entry = record(
        &dir,
        &workspace,
        "name = \"x\"",
        response(
            vec![
                ("x-echo".to_owned(), format!("got {SECRET}")),
                (
                    "set-cookie".to_owned(),
                    "sid=resp-cookie; Path=/".to_owned(),
                ),
                ("content-type".to_owned(), "application/json".to_owned()),
            ],
            &format!("{{\"key\":\"{SECRET}\"}}"),
        ),
    );
    let got = history_get(&mut state, entry);
    let text = got.to_string();
    assert!(!text.contains(SECRET), "{text}");
    assert!(!text.contains("resp-cookie"), "{text}");
    assert_eq!(got["response"]["body"], format!("{{\"key\":\"{MASK}\"}}"));
    assert_eq!(got["response"]["headers"][0][1], format!("got {MASK}"));
    assert_eq!(
        got["response"]["headers"][1][1],
        format!("sid={MASK}; Path=/")
    );
    assert_eq!(got["response"]["headers"][2][1], "application/json");
}

#[test]
fn history_masks_a_recorded_error_with_the_secrets_in_scope() {
    let dir = TempDir::new().unwrap();
    let (mut state, workspace) = with_collection_secret(&dir);
    let entry = record(
        &dir,
        &workspace,
        "name = \"x\"",
        HistoryOutcome::Failed {
            error: format!("could not connect to http://x/?k={SECRET}"),
            elapsed_ms: 1,
        },
    );
    let got = history_get(&mut state, entry);
    assert!(!got.to_string().contains(SECRET), "{got}");
    let list = ok(engine_tools::history_list(
        &mut state,
        HistoryListArgs { limit: None },
    ));
    assert!(!list.to_string().contains(SECRET), "{list}");
}

#[test]
fn history_get_omits_the_response_when_the_keyring_cannot_be_read() {
    let dir = TempDir::new().unwrap();
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::unavailable("no secret service")),
        Some(dir.path().join("state.sqlite3")),
    );
    let root = dir.path().join("ws");
    ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: root.display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    let manifest = root.join("reqflux.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\n[[globals]]\nkey = \"token\"\nsecret = true\n"),
    )
    .unwrap();
    let workspace = workspace_id(&mut state);
    let entry = record(
        &dir,
        &workspace,
        "name = \"x\"",
        response(
            vec![("x-echo".to_owned(), "maybe-SECRET".to_owned())],
            "maybe-SECRET body",
        ),
    );
    let got = history_get(&mut state, entry);
    let text = got.to_string();
    assert!(!text.contains("maybe-SECRET"), "{text}");
    assert!(got["response"]["body"].is_null(), "{text}");
    assert_eq!(got["response"]["headers"][0][0], "x-echo");
    assert!(
        got["response"]["omitted"]
            .as_str()
            .is_some_and(|reason| reason.contains("no secret service")),
        "{text}"
    );
}
