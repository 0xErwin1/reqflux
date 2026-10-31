use pretty_assertions::assert_eq;

use super::*;

const ID_A: &str = "01JBQ3Z7W4X9V8N2K5M6P7R8S9";
const ID_B: &str = "01JBQ3Z7W4X9V8N2K5M6P7R8SA";

fn id(text: &str) -> Id {
    match text.parse() {
        Ok(id) => id,
        Err(error) => panic!("invalid test id {text}: {error}"),
    }
}

fn to_toml<T: serde::Serialize>(value: &T) -> String {
    match toml::to_string(value) {
        Ok(text) => text,
        Err(error) => panic!("serialize failed: {error}"),
    }
}

fn from_toml<T: serde::de::DeserializeOwned>(text: &str) -> T {
    match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("deserialize failed: {error}\n{text}"),
    }
}

/// Deserializing then serializing must give back the same bytes (S5).
fn assert_round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(text: &str) {
    let value: T = from_toml(text);
    assert_eq!(to_toml(&value), text);
}

// --- Id ---------------------------------------------------------------------

#[test]
fn id_displays_and_parses_as_ulid_string() {
    let parsed = id(ID_A);
    assert_eq!(parsed.to_string(), ID_A);
    assert!("not-a-ulid".parse::<Id>().is_err());
    assert_ne!(Id::new(), Id::new());
}

// --- Method (S3) ------------------------------------------------------------

#[test]
fn method_normalizes_to_uppercase_and_accepts_custom_verbs() {
    assert_eq!(
        Method::new("get").map(|m| m.to_string()),
        Ok("GET".to_owned())
    );
    assert_eq!(
        Method::new("Purge").map(|m| m.as_str().to_owned()),
        Ok("PURGE".to_owned())
    );
    assert_eq!(Method::new("post"), Ok(Method::POST));
}

#[test]
fn method_rejects_empty_and_whitespace_verbs() {
    assert_eq!(Method::new(""), Err(MethodError::Empty));
    assert_eq!(
        Method::new("GE T"),
        Err(MethodError::Whitespace("GE T".to_owned()))
    );
    assert!(
        toml::from_str::<Request>(&format!(
            "id = \"{ID_A}\"\nname = \"x\"\nmethod = \"\"\nurl = \"\"\n"
        ))
        .is_err()
    );
}

// --- Workspace manifest (S1, S5) --------------------------------------------

#[test]
fn workspace_manifest_serializes_in_fixed_order_with_defaults_omitted() {
    let manifest = WorkspaceManifest {
        id: id(ID_A),
        name: "Billing".to_owned(),
        format: 1,
        globals: vec![Variable {
            key: "host".to_owned(),
            value: Some("example.com".to_owned()),
            description: None,
            secret: false,
            enabled: true,
        }],
        mcp: None,
        extensions: toml::Table::new(),
    };
    assert_eq!(
        to_toml(&manifest),
        format!(
            "id = \"{ID_A}\"\nname = \"Billing\"\nformat = 1\n\n[[globals]]\nkey = \"host\"\nvalue = \"example.com\"\n"
        )
    );
}

#[test]
fn workspace_manifest_round_trips() {
    assert_round_trip::<WorkspaceManifest>(&format!(
        "id = \"{ID_A}\"\nname = \"Billing\"\nformat = 1\n\n[[globals]]\nkey = \"host\"\nvalue = \"example.com\"\ndescription = \"API host\"\n\n[[globals]]\nkey = \"old\"\nvalue = \"x\"\nenabled = false\n"
    ));
}

// --- Environment (S1, S5, S10) ----------------------------------------------

#[test]
fn environment_serializes_and_round_trips() {
    let environment = Environment {
        id: id(ID_A),
        name: "Dev".to_owned(),
        variables: vec![
            Variable::new("base_url", "http://localhost"),
            Variable {
                key: "token".to_owned(),
                value: None,
                description: None,
                secret: true,
                enabled: true,
            },
        ],
        extensions: toml::Table::new(),
    };
    let expected = format!(
        "id = \"{ID_A}\"\nname = \"Dev\"\n\n[[variables]]\nkey = \"base_url\"\nvalue = \"http://localhost\"\n\n[[variables]]\nkey = \"token\"\nsecret = true\n"
    );
    assert_eq!(to_toml(&environment), expected);
    assert_round_trip::<Environment>(&expected);
}

#[test]
fn secret_variable_never_serializes_its_value() {
    let variable = Variable {
        key: "token".to_owned(),
        value: Some("s3cr3t".to_owned()),
        description: None,
        secret: true,
        enabled: true,
    };
    let text = to_toml(&Environment {
        id: id(ID_A),
        name: "Dev".to_owned(),
        variables: vec![variable],
        extensions: toml::Table::new(),
    });
    assert!(!text.contains("s3cr3t"), "{text}");
    assert!(!text.contains("value"), "{text}");
}

#[test]
fn strip_secret_values_drops_values_and_reports_keys() {
    let mut environment: Environment = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"Dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"s3cr3t\"\nsecret = true\n\n[[variables]]\nkey = \"empty\"\nsecret = true\n\n[[variables]]\nkey = \"host\"\nvalue = \"h\"\n"
    ));
    assert_eq!(environment.strip_secret_values(), vec!["token".to_owned()]);
    let values: Vec<Option<&str>> = environment
        .variables
        .iter()
        .map(|v| v.value.as_deref())
        .collect();
    assert_eq!(values, vec![None, None, Some("h")]);
    assert_eq!(environment.strip_secret_values(), Vec::<String>::new());

    let mut manifest: WorkspaceManifest = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"W\"\nformat = 1\n\n[[globals]]\nkey = \"k\"\nvalue = \"v\"\nsecret = true\n"
    ));
    assert_eq!(manifest.strip_secret_values(), vec!["k".to_owned()]);

    let mut collection: Collection = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"C\"\n\n[[variables]]\nkey = \"c\"\nvalue = \"v\"\nsecret = true\n"
    ));
    assert_eq!(collection.strip_secret_values(), vec!["c".to_owned()]);
}

// --- Collection and folder (S2, S5) -----------------------------------------

#[test]
fn collection_serializes_minimal_with_defaults_omitted() {
    let collection = Collection {
        id: id(ID_A),
        name: "Billing API".to_owned(),
        environments: Vec::new(),
        order: Vec::new(),
        auth: Auth::Inherit,
        variables: Vec::new(),
        extensions: toml::Table::new(),
    };
    assert_eq!(
        to_toml(&collection),
        format!("id = \"{ID_A}\"\nname = \"Billing API\"\n")
    );
}

#[test]
fn collection_round_trips_with_allowlist_order_auth_and_variables() {
    assert_round_trip::<Collection>(&format!(
        "id = \"{ID_A}\"\nname = \"Billing API\"\nenvironments = [\"{ID_B}\"]\norder = [\"invoices\", \"health\"]\n\n[auth]\ntype = \"bearer\"\ntoken = \"{{{{token}}}}\"\n\n[[variables]]\nkey = \"version\"\nvalue = \"v1\"\n"
    ));
}

#[test]
fn folder_serializes_and_round_trips() {
    let folder = Folder {
        id: id(ID_A),
        name: "Invoices".to_owned(),
        order: vec!["list".to_owned(), "create".to_owned()],
        auth: Auth::None,
        extensions: toml::Table::new(),
    };
    let expected = format!(
        "id = \"{ID_A}\"\nname = \"Invoices\"\norder = [\"list\", \"create\"]\n\n[auth]\ntype = \"none\"\n"
    );
    assert_eq!(to_toml(&folder), expected);
    assert_round_trip::<Folder>(&expected);
}

// --- Request (S3, S5) -------------------------------------------------------

#[test]
fn request_serializes_minimal_with_defaults_omitted() {
    let request = Request::new(id(ID_A), "Health", Method::GET, "{{base_url}}/health");
    assert_eq!(
        to_toml(&request),
        format!(
            "id = \"{ID_A}\"\nname = \"Health\"\nmethod = \"GET\"\nurl = \"{{{{base_url}}}}/health\"\n"
        )
    );
}

#[test]
fn request_serializes_full_document_in_fixed_order() {
    let request = Request {
        id: id(ID_A),
        name: "Create".to_owned(),
        method: Method::POST,
        url: "https://api.test/invoices".to_owned(),
        params: vec![KeyValue::new("page", "1")],
        headers: vec![KeyValue {
            key: "X-Trace".to_owned(),
            value: "on".to_owned(),
            description: Some("debug".to_owned()),
            enabled: false,
        }],
        auth: Auth::Basic {
            username: "u".to_owned(),
            password: "p".to_owned(),
        },
        body: Body::Raw {
            language: RawLanguage::Json,
            text: "{}".to_owned(),
        },
        settings: Settings {
            timeout_ms: Some(5000),
            follow_redirects: Some(false),
            verify_tls: None,
        },
        extensions: toml::Table::new(),
    };
    assert_eq!(
        to_toml(&request),
        format!(
            "id = \"{ID_A}\"\nname = \"Create\"\nmethod = \"POST\"\nurl = \"https://api.test/invoices\"\n\n[[params]]\nkey = \"page\"\nvalue = \"1\"\n\n[[headers]]\nkey = \"X-Trace\"\nvalue = \"on\"\ndescription = \"debug\"\nenabled = false\n\n[auth]\ntype = \"basic\"\nusername = \"u\"\npassword = \"p\"\n\n[body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{}}\"\n\n[settings]\ntimeout_ms = 5000\nfollow_redirects = false\n"
        )
    );
}

#[test]
fn request_round_trips_every_body_mode() {
    let head = format!("id = \"{ID_A}\"\nname = \"R\"\nmethod = \"PATCH\"\nurl = \"u\"\n\n");
    let bodies = [
        "[body]\nmode = \"raw\"\nlanguage = \"xml\"\ntext = \"<a/>\"\n".to_owned(),
        "[body]\nmode = \"urlencoded\"\n\n[[body.fields]]\nkey = \"a\"\nvalue = \"1\"\n\n[[body.fields]]\nkey = \"b\"\nvalue = \"2\"\nenabled = false\n".to_owned(),
        "[body]\nmode = \"formdata\"\n\n[[body.parts]]\nkind = \"text\"\nkey = \"name\"\nvalue = \"x\"\n\n[[body.parts]]\nkind = \"file\"\nkey = \"upload\"\npath = \"files/a.png\"\ndescription = \"avatar\"\n".to_owned(),
        "[body]\nmode = \"binary\"\npath = \"blob.bin\"\n".to_owned(),
        "[body]\nmode = \"graphql\"\nquery = \"{ me { id } }\"\nvariables = \"{}\"\n".to_owned(),
    ];
    for body in bodies {
        assert_round_trip::<Request>(&format!("{head}{body}"));
    }
}

#[test]
fn body_modes_deserialize_to_typed_variants() {
    let request: Request = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"R\"\nmethod = \"put\"\nurl = \"u\"\n\n[body]\nmode = \"binary\"\npath = \"a.bin\"\n"
    ));
    assert_eq!(request.method, Method::PUT);
    assert_eq!(
        request.body,
        Body::Binary {
            path: "a.bin".to_owned()
        }
    );
    let none: Request = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n"
    ));
    assert_eq!(none.body, Body::None);
    assert_eq!(none.auth, Auth::Inherit);
    assert_eq!(none.settings, Settings::default());
}

// --- Auth (S3) --------------------------------------------------------------

#[test]
fn auth_round_trips_explicit_types() {
    let head = format!("id = \"{ID_A}\"\nname = \"F\"\n\n");
    for auth in [
        "[auth]\ntype = \"none\"\n",
        "[auth]\ntype = \"basic\"\nusername = \"u\"\npassword = \"p\"\n",
        "[auth]\ntype = \"bearer\"\ntoken = \"t\"\n",
        "[auth]\ntype = \"apikey\"\nkey = \"X-Key\"\nvalue = \"v\"\nlocation = \"header\"\n",
        "[auth]\ntype = \"apikey\"\nkey = \"k\"\nvalue = \"v\"\nlocation = \"query\"\n",
    ] {
        assert_round_trip::<Folder>(&format!("{head}{auth}"));
    }
}

#[test]
fn auth_apikey_deserializes_location() {
    let folder: Folder = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"F\"\n\n[auth]\ntype = \"apikey\"\nkey = \"k\"\nvalue = \"v\"\nlocation = \"query\"\n"
    ));
    assert_eq!(
        folder.auth,
        Auth::ApiKey {
            key: "k".to_owned(),
            value: "v".to_owned(),
            location: ApiKeyLocation::Query
        }
    );
}

#[test]
fn auth_unknown_type_is_preserved_unchanged() {
    let text = format!(
        "id = \"{ID_A}\"\nname = \"F\"\n\n[auth]\ntype = \"digest\"\nalgorithm = \"MD5\"\nrealm = \"r\"\nusername = \"u\"\n"
    );
    let folder: Folder = from_toml(&text);
    let Auth::Other { kind, fields } = &folder.auth else {
        panic!("expected Auth::Other, got {:?}", folder.auth);
    };
    assert_eq!(kind, "digest");
    assert_eq!(fields.get("realm").and_then(toml::Value::as_str), Some("r"));
    assert!(!fields.contains_key("type"));
    assert_eq!(to_toml(&folder), text);
}

#[test]
fn auth_missing_type_is_an_error() {
    assert!(
        toml::from_str::<Folder>(&format!(
            "id = \"{ID_A}\"\nname = \"F\"\n\n[auth]\ntoken = \"t\"\n"
        ))
        .is_err()
    );
}

// --- Extensions (S9) --------------------------------------------------------

#[test]
fn extensions_table_round_trips_unchanged() {
    assert_round_trip::<Request>(&format!(
        "id = \"{ID_A}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n\n[extensions]\nfuture = \"kept\"\nwhen = 1979-05-27T07:32:00Z\n\n[extensions.plugin]\nlevel = 3\n"
    ));
    assert_round_trip::<Environment>(&format!(
        "id = \"{ID_A}\"\nname = \"E\"\n\n[extensions]\nx = 1\n"
    ));
    assert_round_trip::<WorkspaceManifest>(&format!(
        "id = \"{ID_A}\"\nname = \"W\"\nformat = 1\n\n[extensions]\nx = 1\n"
    ));
    assert_round_trip::<Collection>(&format!(
        "id = \"{ID_A}\"\nname = \"C\"\n\n[extensions]\nx = 1\n"
    ));
    assert_round_trip::<Folder>(&format!(
        "id = \"{ID_A}\"\nname = \"F\"\n\n[extensions]\nx = 1\n"
    ));
}

#[test]
fn unknown_top_level_keys_are_collected_into_extensions() {
    let request: Request = from_toml(&format!(
        "id = \"{ID_A}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\nfuture = \"kept\"\n"
    ));
    assert_eq!(
        request
            .extensions
            .get("future")
            .and_then(toml::Value::as_str),
        Some("kept")
    );
    assert_eq!(
        to_toml(&request),
        format!(
            "id = \"{ID_A}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n\n[extensions]\nfuture = \"kept\"\n"
        )
    );
}

// --- Tree (S1, S2) ----------------------------------------------------------

#[test]
fn tree_holds_nested_folders_and_requests_with_scripts() {
    let request = RequestNode {
        slug: "health".to_owned(),
        request: Request::new(id(ID_B), "Health", Method::GET, "u"),
        scripts: Scripts {
            pre_request: None,
            test: Some("pm.test()".to_owned()),
        },
    };
    let folder = FolderNode {
        slug: "ops".to_owned(),
        folder: Folder {
            id: id(ID_A),
            name: "Ops".to_owned(),
            order: Vec::new(),
            auth: Auth::Inherit,
            extensions: toml::Table::new(),
        },
        scripts: Scripts::default(),
        items: vec![Item::Request(request)],
    };
    let mut collection = CollectionNode {
        slug: "api".to_owned(),
        collection: from_toml(&format!("id = \"{ID_A}\"\nname = \"API\"\n")),
        scripts: Scripts::default(),
        items: vec![Item::Folder(folder)],
    };
    let names: Vec<&str> = collection
        .requests()
        .map(|r| r.request.name.as_str())
        .collect();
    assert_eq!(names, vec!["Health"]);
    assert!(collection.collection.allows_environment(id(ID_B)));
    collection.collection.environments = vec![id(ID_A)];
    assert!(collection.collection.allows_environment(id(ID_A)));
    assert!(!collection.collection.allows_environment(id(ID_B)));
}

/// An `apikey` without `location` is a header API key, not an unsupported type: the default
/// location is simply not written to disk.
#[test]
fn an_apikey_without_location_is_a_header_api_key() {
    let document: Request = match toml::from_str(
        "id = \"01JB7X3K9Q0000000000000003\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n\n\
         [auth]\ntype = \"apikey\"\nkey = \"X-Key\"\nvalue = \"v\"\n",
    ) {
        Ok(document) => document,
        Err(error) => panic!("{error}"),
    };
    assert_eq!(
        document.auth,
        Auth::ApiKey {
            key: "X-Key".to_owned(),
            value: "v".to_owned(),
            location: ApiKeyLocation::Header,
        }
    );
}
