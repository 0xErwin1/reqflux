//! A draft's unsaved scripts (request-pane-feedback T5): sending a draft with scripts runs
//! them in place of the request's saved sidecars, and the inherited folder and collection
//! scripts resolve as before.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::path::Path;

use common::{id_text, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Id, Method, Request, Scripts};
use reqflux_engine::{Engine, ExecuteOptions, Executed};
use tokio::runtime::Runtime;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

fn server(runtime: &Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        server
    })
}

/// A workspace with `api/f/r`, a folder pre-request script setting `who` to `folder` and the
/// request's own saved sidecars setting it to `saved` and testing `saved test`.
fn workspace(root: &Path, base: &str) {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", id_text(1)),
    );
    write(
        root,
        "collections/api/f/folder.toml",
        &format!("id = \"{}\"\nname = \"f\"\n", id_text(4)),
    );
    write(
        root,
        "collections/api/f/folder.pre.js",
        "pm.collectionVariables.set(\"who\", \"folder\");\n",
    );
    write(
        root,
        "collections/api/f/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\nurl = \"{base}/q?who={{{{who}}}}\"\n",
            id_text(2)
        ),
    );
    write(
        root,
        "collections/api/f/r.pre.js",
        "pm.collectionVariables.set(\"who\", \"saved\");\n",
    );
    write(
        root,
        "collections/api/f/r.test.js",
        "pm.test(\"saved test\", function () {});\n",
    );
}

fn open(root: &Path) -> Engine {
    match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn draft(base: &str) -> Request {
    Request::new(
        Id::new(),
        "r",
        Method::GET,
        format!("{base}/q?who={{{{who}}}}"),
    )
}

fn send(engine: &Engine, draft: &Request, scripts: Option<&Scripts>) -> Executed {
    let execution = match engine.execute_draft_with_scripts(
        "api/f/r",
        draft,
        scripts,
        &ExecuteOptions::default(),
    ) {
        Ok(execution) => execution,
        Err(error) => panic!("execute draft: {error}"),
    };
    match engine.wait(execution) {
        Ok(executed) => executed,
        Err(error) => panic!("send: {error}"),
    }
}

fn test_names(executed: &Executed) -> Vec<&str> {
    executed
        .scripts
        .tests
        .iter()
        .map(|test| test.name.as_str())
        .collect()
}

#[test]
fn draft_scripts_run_in_place_of_the_saved_sidecars() {
    let rt = runtime();
    let server = server(&rt);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri());
    let engine = open(dir.path());
    let scripts = Scripts {
        pre_request: Some("pm.collectionVariables.set(\"who\", \"draft\");\n".to_owned()),
        test: Some("pm.test(\"draft test\", function () {});\n".to_owned()),
    };
    let executed = send(&engine, &draft(&server.uri()), Some(&scripts));
    assert!(executed.url.ends_with("?who=draft"), "{}", executed.url);
    assert_eq!(test_names(&executed), ["draft test"]);
}

#[test]
fn a_draft_without_request_scripts_inherits_the_folder_script() {
    let rt = runtime();
    let server = server(&rt);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri());
    let engine = open(dir.path());
    let scripts = Scripts {
        pre_request: Some(String::new()),
        test: None,
    };
    let executed = send(&engine, &draft(&server.uri()), Some(&scripts));
    assert!(executed.url.ends_with("?who=folder"), "{}", executed.url);
    assert_eq!(test_names(&executed), Vec::<&str>::new());
}

#[test]
fn a_draft_without_draft_scripts_runs_the_saved_sidecars() {
    let rt = runtime();
    let server = server(&rt);
    let dir = temp_dir();
    workspace(dir.path(), &server.uri());
    let engine = open(dir.path());
    let executed = send(&engine, &draft(&server.uri()), None);
    assert!(executed.url.ends_with("?who=saved"), "{}", executed.url);
    assert_eq!(test_names(&executed), ["saved test"]);
    let execution =
        match engine.execute_draft("api/f/r", &draft(&server.uri()), &ExecuteOptions::default()) {
            Ok(execution) => execution,
            Err(error) => panic!("execute draft: {error}"),
        };
    let executed = match engine.wait(execution) {
        Ok(executed) => executed,
        Err(error) => panic!("send: {error}"),
    };
    assert!(executed.url.ends_with("?who=saved"), "{}", executed.url);
}
