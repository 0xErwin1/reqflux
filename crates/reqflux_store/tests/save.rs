//! S7: atomic, hash-checked saves.

mod common;

use common::*;
use pretty_assertions::assert_eq;
use reqflux_core::model::{Folder, Method, Request, Scripts};
use reqflux_store::{ContentHash, Parent, ScriptHashes, ScriptOwner};

fn request(n: u32, name: &str) -> Request {
    Request::new(id(n), name, Method::GET, "https://api.test")
}

fn parent() -> Parent {
    Parent::collection("api")
}

fn conflict(relative: &str, root: &std::path::Path) -> String {
    format!(
        "{} changed on disk since it was loaded",
        path(root, relative).display()
    )
}

#[test]
fn saving_with_the_loaded_hash_writes_and_returns_the_new_hash() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &collection_toml(1, "API", ""),
    );
    write(root, "collections/api/r.req.toml", &request_toml(2, "R"));
    let loaded = load(root);
    let expected = loaded.hash(&path(root, "collections/api/r.req.toml"));

    let hash = reqflux_store::save_request(root, &parent(), "r", &request(2, "Renamed"), expected);

    let text = read(root, "collections/api/r.req.toml");
    assert_eq!(
        text,
        format!(
            "id = \"{}\"\nname = \"Renamed\"\nmethod = \"GET\"\nurl = \"https://api.test\"\n",
            id_text(2)
        )
    );
    assert_eq!(hash.ok(), Some(ContentHash::of(text.as_bytes())));
    assert_eq!(
        files(root),
        vec![
            "collections/api/collection.toml",
            "collections/api/r.req.toml",
            "reqflux.toml"
        ]
    );
}

#[test]
fn a_stale_hash_fails_and_leaves_the_file_untouched() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &collection_toml(1, "API", ""),
    );
    write(root, "collections/api/r.req.toml", &request_toml(2, "R"));
    let loaded = load(root);
    let expected = loaded.hash(&path(root, "collections/api/r.req.toml"));
    let theirs = request_toml(2, "Changed by someone else");
    write(root, "collections/api/r.req.toml", &theirs);

    let result = reqflux_store::save_request(root, &parent(), "r", &request(2, "Mine"), expected);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(conflict("collections/api/r.req.toml", root))
    );
    assert_eq!(read(root, "collections/api/r.req.toml"), theirs);
    assert_eq!(
        files(root),
        vec![
            "collections/api/collection.toml",
            "collections/api/r.req.toml",
            "reqflux.toml"
        ]
    );
}

#[test]
fn a_file_deleted_since_load_is_a_conflict() {
    let dir = minimal_workspace();
    let root = dir.path();
    let expected = Some(ContentHash::of(b"whatever"));

    let result = reqflux_store::save_request(root, &parent(), "r", &request(2, "R"), expected);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(conflict("collections/api/r.req.toml", root))
    );
    assert_eq!(files(root), vec!["reqflux.toml"]);
}

#[test]
fn creating_an_entity_whose_file_exists_fails_and_leaves_it_untouched() {
    let dir = minimal_workspace();
    let root = dir.path();
    let theirs = folder_toml(3, "Theirs", "");
    write(root, "collections/api/f/folder.toml", &theirs);
    let folder = Folder {
        description: None,
        id: id(4),
        name: "Mine".into(),
        order: Vec::new(),
        auth: reqflux_core::model::Auth::Inherit,
        extensions: toml::Table::new(),
    };

    let result = reqflux_store::save_folder(root, &parent(), "f", &folder, None);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(conflict("collections/api/f/folder.toml", root))
    );
    assert_eq!(read(root, "collections/api/f/folder.toml"), theirs);
}

#[test]
fn creating_new_entities_creates_their_directories() {
    let dir = minimal_workspace();
    let root = dir.path();
    let nested = parent().child("f");
    let collection = reqflux_core::model::Collection {
        description: None,
        id: id(1),
        name: "API".into(),
        environments: Vec::new(),
        order: vec!["f".into()],
        auth: reqflux_core::model::Auth::Inherit,
        variables: Vec::new(),
        extensions: toml::Table::new(),
    };
    let folder = Folder {
        description: None,
        id: id(2),
        name: "F".into(),
        order: Vec::new(),
        auth: reqflux_core::model::Auth::None,
        extensions: toml::Table::new(),
    };

    let results = [
        reqflux_store::save_collection(root, "api", &collection, None),
        reqflux_store::save_folder(root, &parent(), "f", &folder, None),
        reqflux_store::save_request(root, &nested, "r", &request(3, "R"), None),
    ];

    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(
        files(root),
        vec![
            "collections/api/collection.toml",
            "collections/api/f/folder.toml",
            "collections/api/f/r.req.toml",
            "reqflux.toml",
        ]
    );
    assert_eq!(
        read(root, "collections/api/f/folder.toml"),
        format!(
            "id = \"{}\"\nname = \"F\"\n\n[auth]\ntype = \"none\"\n",
            id_text(2)
        )
    );
    let loaded = load(root);
    assert_eq!(loaded.workspace.collections[0].collection, collection);
}

#[test]
fn saving_the_manifest_is_hash_checked() {
    let dir = minimal_workspace();
    let root = dir.path();
    let loaded = load(root);
    let mut manifest = loaded.workspace.manifest.clone();
    manifest.name = "Renamed".into();

    let stale = reqflux_store::save_manifest(root, &manifest, None);
    let saved =
        reqflux_store::save_manifest(root, &manifest, loaded.hash(&path(root, "reqflux.toml")));

    assert_eq!(
        stale.map_err(|e| e.to_string()),
        Err(conflict("reqflux.toml", root))
    );
    assert!(saved.is_ok(), "{saved:?}");
    assert_eq!(
        read(root, "reqflux.toml"),
        format!(
            "id = \"{}\"\nname = \"Renamed\"\nformat = 1\n",
            id_text(900)
        )
    );
}

#[test]
fn an_invalid_slug_is_rejected_without_writing() {
    let dir = minimal_workspace();
    let root = dir.path();

    for slug in ["", "..", ".hidden", "a/b", "a\\b"] {
        let result = reqflux_store::save_request(root, &parent(), slug, &request(2, "R"), None);
        assert_eq!(
            result.map_err(|e| e.to_string()),
            Err(format!("invalid slug `{slug}`")),
        );
    }
    assert_eq!(files(root), vec!["reqflux.toml"]);
}

#[test]
fn a_reserved_request_slug_is_rejected_without_writing() {
    let dir = minimal_workspace();
    let root = dir.path();
    let folder = parent().child("invoices");

    for slug in ["collection", "folder"] {
        let saved = reqflux_store::save_request(root, &folder, slug, &request(2, "R"), None);
        assert_eq!(
            saved.map_err(|e| e.to_string()),
            Err(format!("request slug `{slug}` is reserved")),
        );
        let scripts = reqflux_store::save_scripts(
            root,
            &ScriptOwner::Request(folder.clone(), slug.into()),
            &Scripts {
                pre_request: Some("pm.test()".into()),
                test: None,
            },
            ScriptHashes::default(),
        );
        assert_eq!(
            scripts.map_err(|e| e.to_string()),
            Err(format!("request slug `{slug}` is reserved")),
        );
    }
    assert_eq!(files(root), vec!["reqflux.toml"]);
}

// --- Shared slugs ------------------------------------------------------------

fn folder(id_n: u32, name: &str) -> Folder {
    Folder {
        description: None,
        id: id(id_n),
        name: name.into(),
        order: Vec::new(),
        auth: reqflux_core::model::Auth::Inherit,
        extensions: toml::Table::new(),
    }
}

#[test]
fn saving_a_request_whose_slug_is_already_a_folder_fails_without_writing() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &collection_toml(1, "API", ""),
    );
    write(
        root,
        "collections/api/invoices/folder.toml",
        &folder_toml(2, "Invoices", ""),
    );

    let result =
        reqflux_store::save_request(root, &parent(), "invoices", &request(3, "Invoices"), None);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(format!(
            "{}: a folder already uses the slug `invoices` in this parent; a request and a folder cannot share a slug",
            path(root, "collections/api/invoices").display()
        )),
    );
    assert_eq!(
        files(root),
        vec![
            "collections/api/collection.toml",
            "collections/api/invoices/folder.toml",
            "reqflux.toml"
        ]
    );
}

#[test]
fn saving_a_folder_whose_slug_is_already_a_request_fails_without_writing() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &collection_toml(1, "API", ""),
    );
    write(
        root,
        "collections/api/invoices.req.toml",
        &request_toml(2, "Invoices"),
    );

    let result =
        reqflux_store::save_folder(root, &parent(), "invoices", &folder(3, "Invoices"), None);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(format!(
            "{}: a request already uses the slug `invoices` in this parent; a request and a folder cannot share a slug",
            path(root, "collections/api/invoices.req.toml").display()
        )),
    );
    assert_eq!(
        files(root),
        vec![
            "collections/api/collection.toml",
            "collections/api/invoices.req.toml",
            "reqflux.toml"
        ]
    );
}

// --- Durability --------------------------------------------------------------

fn collection() -> reqflux_core::model::Collection {
    reqflux_core::model::Collection {
        description: None,
        id: id(1),
        name: "API".into(),
        environments: Vec::new(),
        order: Vec::new(),
        auth: reqflux_core::model::Auth::Inherit,
        variables: Vec::new(),
        extensions: toml::Table::new(),
    }
}

#[test]
fn a_save_succeeds_and_leaves_no_temporary_file_behind() {
    let dir = minimal_workspace();
    let root = dir.path();
    let collection = collection();

    let saved = reqflux_store::save_collection(root, "api", &collection, None);

    assert!(saved.is_ok(), "{saved:?}");
    // `files` lists every entry, hidden ones included, so a leftover `.tmp` file shows up.
    assert_eq!(
        files(root),
        vec!["collections/api/collection.toml", "reqflux.toml"]
    );
    assert_eq!(load(root).workspace.collections[0].collection, collection);
}

#[test]
fn a_save_into_a_fresh_directory_succeeds() {
    // The parent-directory sync cannot be observed from a test, so this pins that the
    // code path runs on a directory the save itself just created.
    let dir = minimal_workspace();
    let root = dir.path();
    let collection = collection();

    let saved = reqflux_store::save_collection(root, "fresh", &collection, None);

    assert!(saved.is_ok(), "{saved:?}");
    assert_eq!(
        read(root, "collections/fresh/collection.toml"),
        format!("id = \"{}\"\nname = \"API\"\n", id_text(1))
    );
}

// --- Scripts -----------------------------------------------------------------

fn scripts(pre: Option<&str>, test: Option<&str>) -> Scripts {
    Scripts {
        pre_request: pre.map(Into::into),
        test: test.map(Into::into),
    }
}

#[test]
fn saving_scripts_writes_each_sidecar_and_returns_their_hashes() {
    let dir = minimal_workspace();
    let root = dir.path();
    let owner = ScriptOwner::Request(parent(), "r".into());

    let hashes = reqflux_store::save_scripts(
        root,
        &owner,
        &scripts(Some("p();\n"), Some("t();\n")),
        ScriptHashes::default(),
    );

    assert_eq!(
        hashes.ok(),
        Some(ScriptHashes {
            pre_request: Some(ContentHash::of(b"p();\n")),
            test: Some(ContentHash::of(b"t();\n"))
        })
    );
    assert_eq!(read(root, "collections/api/r.pre.js"), "p();\n");
    assert_eq!(read(root, "collections/api/r.test.js"), "t();\n");
}

#[test]
fn an_empty_or_absent_script_deletes_its_sidecar() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &collection_toml(1, "API", ""),
    );
    write(root, "collections/api/collection.pre.js", "p();\n");
    write(root, "collections/api/collection.test.js", "t();\n");
    let loaded = load(root);
    let expected = ScriptHashes {
        pre_request: loaded.hash(&path(root, "collections/api/collection.pre.js")),
        test: loaded.hash(&path(root, "collections/api/collection.test.js")),
    };
    let owner = ScriptOwner::Collection("api".into());

    let hashes = reqflux_store::save_scripts(root, &owner, &scripts(Some(""), None), expected);

    assert_eq!(hashes.ok(), Some(ScriptHashes::default()));
    assert_eq!(
        files(root),
        vec!["collections/api/collection.toml", "reqflux.toml"]
    );
}

#[test]
fn a_script_conflict_writes_neither_sidecar() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/f/folder.toml",
        &folder_toml(1, "F", ""),
    );
    write(root, "collections/api/f/folder.pre.js", "old();\n");
    write(root, "collections/api/f/folder.test.js", "theirs();\n");
    let owner = ScriptOwner::Folder(parent(), "f".into());
    let expected = ScriptHashes {
        pre_request: Some(ContentHash::of(b"old();\n")),
        test: Some(ContentHash::of(b"mine-before();\n")),
    };

    let result =
        reqflux_store::save_scripts(root, &owner, &scripts(Some("new();\n"), None), expected);

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(conflict("collections/api/f/folder.test.js", root))
    );
    assert_eq!(read(root, "collections/api/f/folder.pre.js"), "old();\n");
    assert_eq!(
        read(root, "collections/api/f/folder.test.js"),
        "theirs();\n"
    );
}

#[test]
fn creating_a_script_that_already_exists_is_a_conflict() {
    let dir = minimal_workspace();
    let root = dir.path();
    write(root, "collections/api/r.test.js", "theirs();\n");
    let owner = ScriptOwner::Request(parent(), "r".into());

    let result = reqflux_store::save_scripts(
        root,
        &owner,
        &scripts(None, Some("mine();\n")),
        ScriptHashes::default(),
    );

    assert_eq!(
        result.map_err(|e| e.to_string()),
        Err(conflict("collections/api/r.test.js", root))
    );
    assert_eq!(read(root, "collections/api/r.test.js"), "theirs();\n");
}
