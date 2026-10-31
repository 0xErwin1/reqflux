//! What the window shows of a call from its own workspace (mcp-approvals S3): the request
//! prepared with the named environment, every secret masked, and the variables it uses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::Path;
use std::sync::Arc;

use reqflux_core::model::Id;

use super::{VariableSource, breadcrumb, find_environment, preview};
use crate::{Engine, MASK, MemorySecretStore, SecretKey, SecretScope, SecretStore, SentBody};

const WORKSPACE: &str = "01J00000000000000000000900";
const COLLECTION: &str = "01J00000000000000000000011";
const FOLDER: &str = "01J00000000000000000000012";
const CREATE: &str = "01J00000000000000000000013";
const PROD: &str = "01J00000000000000000000016";
const SECRET: &str = "tok-SUPERSECRET-123";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `Billing` with collection `Billing API` (`base_url`, secret `access_token`), folder
/// `invoices` holding `POST Create invoice` with a bearer token, an `X-Api-Key` header
/// carrying the secret, `X-Tenant-Id` from the environment `prod`, a `$guid` and a JSON body.
fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{WORKSPACE}\"\nname = \"Billing\"\nformat = 1\n"),
    );
    write(
        root,
        "collections/billing/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"Billing API\"\n\n\
             [[variables]]\nkey = \"base_url\"\nvalue = \"https://api.test\"\n\n\
             [[variables]]\nkey = \"access_token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/billing/invoices/folder.toml",
        &format!("id = \"{FOLDER}\"\nname = \"invoices\"\n"),
    );
    write(
        root,
        "collections/billing/invoices/create.req.toml",
        &format!(
            "id = \"{CREATE}\"\nname = \"Create invoice\"\nmethod = \"POST\"\n\
             url = \"{{{{base_url}}}}/v1/invoices\"\n\n\
             [[headers]]\nkey = \"X-Api-Key\"\nvalue = \"{{{{access_token}}}}\"\n\n\
             [[headers]]\nkey = \"X-Tenant-Id\"\nvalue = \"{{{{tenant_id}}}}\"\n\n\
             [[headers]]\nkey = \"X-Trace\"\nvalue = \"{{{{$guid}}}}\"\n\n\
             [auth]\ntype = \"bearer\"\ntoken = \"{{{{access_token}}}}\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{\\\"currency\\\": \\\"eur\\\"}}\"\n"
        ),
    );
    write(
        root,
        "environments/prod.env.toml",
        &format!(
            "id = \"{PROD}\"\nname = \"prod\"\n\n\
             [[variables]]\nkey = \"tenant_id\"\nvalue = \"tn_prod_4821\"\n"
        ),
    );
    let store = Arc::new(MemorySecretStore::new());
    store
        .set(
            &SecretKey {
                workspace_id: WORKSPACE.parse::<Id>().unwrap(),
                scope: SecretScope::Collection(COLLECTION.parse::<Id>().unwrap()),
                key: "access_token".to_owned(),
            },
            SECRET,
        )
        .unwrap();
    let engine = Engine::open_with_secrets(root.to_path_buf(), store).unwrap();
    (dir, engine)
}

#[test]
fn a_preview_resolves_in_the_environment_and_masks_every_secret() {
    let (_dir, engine) = engine();
    let shown = preview(&engine, CREATE, Some("prod")).unwrap();
    assert_eq!(
        shown.breadcrumb,
        ["Billing API", "invoices", "Create invoice"]
    );
    assert_eq!(shown.method, "POST");
    assert_eq!(shown.inspection.url, "{{base_url}}/v1/invoices");
    assert_eq!(shown.url, "https://api.test/v1/invoices");
    assert_eq!(shown.environment.as_deref(), Some("prod"));
    let header = |name: &str| {
        shown
            .headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(header("X-Api-Key").as_deref(), Some(MASK));
    assert_eq!(header("X-Tenant-Id").as_deref(), Some("tn_prod_4821"));
    assert_eq!(header("Authorization"), Some(format!("Bearer {MASK}")));
    assert_eq!(
        shown.body,
        SentBody::Text("{\"currency\": \"eur\"}".to_owned())
    );
    assert!(!format!("{shown:?}").contains(SECRET), "{shown:?}");

    let names: Vec<&str> = shown.variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["base_url", "access_token", "tenant_id", "$guid"]);
    let variable = |name: &str| shown.variables.iter().find(|v| v.name == name).unwrap();
    assert_eq!(variable("base_url").value, "https://api.test");
    assert_eq!(
        variable("base_url").source,
        VariableSource::Collection("Billing API".to_owned())
    );
    assert_eq!(variable("access_token").value, MASK);
    assert!(variable("access_token").secret);
    assert_eq!(variable("access_token").source.label(), "keyring");
    assert_eq!(variable("tenant_id").source.label(), "prod");
    assert_eq!(variable("$guid").source, VariableSource::Dynamic);
}

#[test]
fn an_environment_is_found_by_slug_name_or_id_and_an_unknown_one_fails() {
    let (_dir, engine) = engine();
    let workspace = engine.workspace();
    for given in ["prod", PROD] {
        assert_eq!(
            find_environment(workspace, given).map(|node| node.slug.as_str()),
            Some("prod")
        );
    }
    assert!(preview(&engine, CREATE, Some(PROD)).is_ok());
    assert!(preview(&engine, CREATE, Some("staging")).is_err());
    let bare = preview(&engine, CREATE, None).unwrap();
    assert_eq!(bare.environment, None);
    let tenant = bare
        .variables
        .iter()
        .find(|v| v.name == "tenant_id")
        .unwrap();
    assert_eq!(tenant.source, VariableSource::Unresolved);
    assert!(preview(&engine, "01J00000000000000000000099", None).is_err());
}

#[test]
fn a_breadcrumb_names_the_path_to_an_item_or_an_environment() {
    let (_dir, engine) = engine();
    let workspace = engine.workspace();
    assert_eq!(breadcrumb(workspace, COLLECTION).unwrap(), ["Billing API"]);
    assert_eq!(
        breadcrumb(workspace, FOLDER).unwrap(),
        ["Billing API", "invoices"]
    );
    assert_eq!(
        breadcrumb(workspace, CREATE).unwrap(),
        ["Billing API", "invoices", "Create invoice"]
    );
    assert_eq!(breadcrumb(workspace, PROD).unwrap(), ["prod"]);
    assert_eq!(breadcrumb(workspace, "01J00000000000000000000099"), None);
}
