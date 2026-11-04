//! S5 and S9: loading and saving an unchanged workspace rewrites every file byte for byte.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::path::{Path, PathBuf};

use common::*;
use pretty_assertions::assert_eq;
use reqflux_core::model::{Item, Scripts, Workspace};
use reqflux_store::{Loaded, Parent, ScriptHashes, ScriptOwner};

fn fixture(root: &Path) -> Vec<(String, String)> {
    let head = |n: u32, name: &str, method: &str| {
        format!(
            "id = \"{}\"\nname = \"{name}\"\nmethod = \"{method}\"\nurl = \"https://api.test/{n}\"\n",
            id_text(n)
        )
    };
    let files = vec![
        (
            "reqflux.toml",
            format!(
                "id = \"{}\"\nname = \"Billing\"\nformat = 1\n\n[[globals]]\nkey = \"host\"\nvalue = \"api.test\"\n\n[[globals]]\nkey = \"key\"\nsecret = true\n\n[extensions]\nzeta = 1\nalpha = \"kept in file order\"\n",
                id_text(1)
            ),
        ),
        (
            "environments/dev.env.toml",
            format!(
                "id = \"{}\"\nname = \"Dev\"\n\n[[variables]]\nkey = \"base\"\nvalue = \"http://localhost\"\ndescription = \"local\"\n\n[[variables]]\nkey = \"token\"\nsecret = true\n\n[[variables]]\nkey = \"old\"\nvalue = \"x\"\nenabled = false\n",
                id_text(2)
            ),
        ),
        (
            "collections/api/collection.toml",
            format!(
                "id = \"{}\"\nname = \"API\"\ndescription = \"\"\"\n# API\n\nBilling endpoints.\n\"\"\"\nenvironments = [\"{}\"]\norder = [\"invoices\", \"raw\"]\n\n[auth]\ntype = \"bearer\"\ntoken = \"{{{{token}}}}\"\n\n[[variables]]\nkey = \"version\"\nvalue = \"v1\"\n",
                id_text(3),
                id_text(2)
            ),
        ),
        (
            "collections/api/collection.pre.js",
            "pm.variables.set(\"a\", 1);\n".to_owned(),
        ),
        (
            "collections/api/collection.test.js",
            "pm.test(\"ok\", () => {});\n".to_owned(),
        ),
        (
            "collections/api/invoices/folder.toml",
            format!(
                "id = \"{}\"\nname = \"Invoices\"\norder = [\"nested\", \"urlencoded\", \"formdata\"]\n\n[auth]\ntype = \"apikey\"\nkey = \"X-Key\"\nvalue = \"v\"\nlocation = \"query\"\n",
                id_text(4)
            ),
        ),
        (
            "collections/api/invoices/folder.pre.js",
            "// folder pre\n".to_owned(),
        ),
        (
            "collections/api/invoices/nested/folder.toml",
            format!(
                "id = \"{}\"\nname = \"Nested\"\ndescription = \"Deeply nested.\"\n\n[auth]\ntype = \"digest\"\nrealm = \"r\"\nalgorithm = \"MD5\"\n\n[extensions]\nfuture = \"kept\"\n",
                id_text(5)
            ),
        ),
        (
            "collections/api/invoices/nested/binary.req.toml",
            format!(
                "{}\n[auth]\ntype = \"none\"\n\n[body]\nmode = \"binary\"\npath = \"blob.bin\"\n",
                head(6, "Binary", "PUT")
            ),
        ),
        (
            "collections/api/invoices/nested/graphql.req.toml",
            format!(
                "{}\n[auth]\ntype = \"basic\"\nusername = \"u\"\npassword = \"p\"\n\n[body]\nmode = \"graphql\"\nquery = \"{{ me {{ id }} }}\"\nvariables = \"{{}}\"\n",
                head(7, "GraphQL", "POST")
            ),
        ),
        (
            "collections/api/invoices/nested/graphql.test.js",
            "pm.test(\"gql\");\n".to_owned(),
        ),
        (
            "collections/api/invoices/urlencoded.req.toml",
            format!(
                "{}\n[[params]]\nkey = \"page\"\nvalue = \"1\"\n\n[[params]]\nkey = \"debug\"\nvalue = \"true\"\ndescription = \"off by default\"\nenabled = false\n\n[[headers]]\nkey = \"X-Trace\"\nvalue = \"on\"\n\n[body]\nmode = \"urlencoded\"\n\n[[body.fields]]\nkey = \"a\"\nvalue = \"1\"\n\n[[body.fields]]\nkey = \"b\"\nvalue = \"2\"\nenabled = false\n\n[settings]\ntimeout_ms = 5000\nfollow_redirects = false\nverify_tls = true\n",
                head(8, "Urlencoded", "POST")
            ),
        ),
        (
            "collections/api/invoices/formdata.req.toml",
            format!(
                "{}\n[body]\nmode = \"formdata\"\n\n[[body.parts]]\nkind = \"text\"\nkey = \"name\"\nvalue = \"x\"\nenabled = false\n\n[[body.parts]]\nkind = \"file\"\nkey = \"upload\"\npath = \"files/a.png\"\ndescription = \"avatar\"\n",
                head(9, "Form data", "POST")
            ),
        ),
        (
            "collections/api/invoices/formdata.pre.js",
            "// pre\n".to_owned(),
        ),
        (
            "collections/api/invoices/formdata.test.js",
            "// test\n".to_owned(),
        ),
        (
            "collections/api/raw.req.toml",
            format!(
                "{}\n[body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"a\": 1}}'\n\n[extensions.plugin]\nlevel = 3\n",
                head(10, "Raw", "PURGE").replacen(
                    "method =",
                    "description = \"\"\"\nSends raw JSON.\n\nSecond paragraph.\n\"\"\"\nmethod =",
                    1
                )
            ),
        ),
        // Not listed in the collection order: appended by name.
        ("collections/api/plain.req.toml", head(11, "Plain", "GET")),
    ];
    let files: Vec<(String, String)> = files
        .into_iter()
        .map(|(path, text)| (path.to_owned(), text))
        .collect();
    for (relative, text) in &files {
        write(root, relative, text);
    }
    files
}

/// The checked layout path of an entity from the loaded workspace; every slug of a
/// loaded workspace is a single visible path component, so this cannot fail.
fn layout_path(path: Result<PathBuf, reqflux_store::StoreError>) -> PathBuf {
    match path {
        Ok(file) => file,
        Err(error) => panic!("layout path: {error}"),
    }
}

fn save_items(root: &Path, loaded: &Loaded, parent: &Parent, items: &[Item]) {
    for item in items {
        match item {
            Item::Folder(node) => {
                let file =
                    layout_path(reqflux_store::checked_folder_file(root, parent, &node.slug));
                let saved = reqflux_store::save_folder(
                    root,
                    parent,
                    &node.slug,
                    &node.folder,
                    loaded.hash(&file),
                );
                assert!(saved.is_ok(), "{saved:?}");
                let owner = ScriptOwner::Folder(parent.clone(), node.slug.clone());
                save_scripts(root, loaded, &owner, &node.scripts);
                save_items(root, loaded, &parent.child(&node.slug), &node.items);
            }
            Item::Request(node) => {
                let file = layout_path(reqflux_store::checked_request_file(
                    root, parent, &node.slug,
                ));
                let saved = reqflux_store::save_request(
                    root,
                    parent,
                    &node.slug,
                    &node.request,
                    loaded.hash(&file),
                );
                assert!(saved.is_ok(), "{saved:?}");
                let owner = ScriptOwner::Request(parent.clone(), node.slug.clone());
                save_scripts(root, loaded, &owner, &node.scripts);
            }
        }
    }
}

fn save_scripts(root: &Path, loaded: &Loaded, owner: &ScriptOwner, scripts: &Scripts) {
    let files = owner.files(root);
    let expected = ScriptHashes {
        pre_request: loaded.hash(&files.pre_request),
        test: loaded.hash(&files.test),
    };
    let saved = reqflux_store::save_scripts(root, owner, scripts, expected);
    assert!(saved.is_ok(), "{saved:?}");
}

fn save_all(root: &Path, loaded: &Loaded) {
    let workspace: &Workspace = &loaded.workspace;
    let manifest = reqflux_store::manifest_file(root);
    assert!(
        reqflux_store::save_manifest(root, &workspace.manifest, loaded.hash(&manifest)).is_ok()
    );
    for node in &workspace.environments {
        let file = layout_path(reqflux_store::checked_environment_file(root, &node.slug));
        let saved = reqflux_store::save_environment(
            root,
            &node.slug,
            &node.environment,
            loaded.hash(&file),
        );
        assert!(saved.is_ok(), "{saved:?}");
    }
    for node in &workspace.collections {
        let file = layout_path(reqflux_store::checked_collection_file(root, &node.slug));
        let saved =
            reqflux_store::save_collection(root, &node.slug, &node.collection, loaded.hash(&file));
        assert!(saved.is_ok(), "{saved:?}");
        save_scripts(
            root,
            loaded,
            &ScriptOwner::Collection(node.slug.clone()),
            &node.scripts,
        );
        save_items(root, loaded, &Parent::collection(&node.slug), &node.items);
    }
}

#[test]
fn load_then_save_every_entity_rewrites_every_file_byte_for_byte() {
    let dir = workspace();
    let root = dir.path();
    let written = fixture(root);
    let loaded = load(root);
    assert_eq!(warnings(&loaded), Vec::<String>::new());

    save_all(root, &loaded);

    for (relative, text) in &written {
        assert_eq!(&read(root, relative), text, "{relative}");
    }
    let mut expected: Vec<String> = written.iter().map(|(path, _)| path.clone()).collect();
    expected.sort();
    assert_eq!(files(root), expected);
    assert_eq!(load(root).workspace, loaded.workspace);
}

#[test]
fn the_fixture_loads_in_display_order() {
    let dir = workspace();
    let root = dir.path();
    fixture(root);

    let loaded = load(root);

    let collection = &loaded.workspace.collections[0];
    let top: Vec<_> = collection.items.iter().map(Item::slug).collect();
    assert_eq!(top, vec!["invoices", "raw", "plain"]);
    let Some(Item::Folder(invoices)) = collection.items.first() else {
        panic!("no folder")
    };
    let inner: Vec<_> = invoices.items.iter().map(Item::slug).collect();
    assert_eq!(inner, vec!["nested", "urlencoded", "formdata"]);
    let Some(Item::Folder(nested)) = invoices.items.first() else {
        panic!("no nested folder")
    };
    let deep: Vec<_> = nested.items.iter().map(Item::slug).collect();
    assert_eq!(deep, vec!["binary", "graphql"]);
    assert_eq!(
        nested
            .folder
            .extensions
            .get("future")
            .and_then(toml::Value::as_str),
        Some("kept")
    );
}
