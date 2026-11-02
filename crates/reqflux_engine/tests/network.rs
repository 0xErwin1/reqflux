//! Network settings (v1-features S5): `[network]` of the user's config file, the passwords in
//! the secret store, and an engine sending through them. Every test uses a temporary config
//! file and an in-memory secret store.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use common::{id, temp_dir};
use pretty_assertions::assert_eq;
use reqflux_engine::config::{
    self, CertificateConfig, NetworkConfig, ProxyConfig, ProxyMode, WorkspaceNetworkConfig,
};
use reqflux_engine::{
    CertificateFile, ClientCertificate, Engine, ExecuteOptions, ManualProxy, MemorySecretStore,
    NetworkSecret, NetworkSettings, ProxySetting, SecretStore, network_settings,
};
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CONFIG: &str = r#"
[network]
ca_bundle = "/etc/ssl/corp-ca.pem"

[network.proxy]
mode = "manual"
url = "http://proxy.corp:3128"
username = "me"
bypass = ["localhost", ".corp"]

[[network.certificates]]
host = "api.example.com"
port = 8443
cert = "/certs/client.pem"
key = "/certs/client-key.pem"

[[network.certificates]]
host = "*.internal"
pkcs12 = "/certs/client.p12"

[network.workspaces."01J00000000000000000000900".proxy]
mode = "none"
"#;

fn manual(url: &str, username: Option<&str>) -> ProxyConfig {
    ProxyConfig {
        mode: ProxyMode::Manual,
        url: Some(url.to_owned()),
        username: username.map(str::to_owned),
        bypass: Vec::new(),
    }
}

#[test]
fn the_network_table_parses_from_the_config_file() {
    let dir = temp_dir();
    let file = dir.path().join("config.toml");
    fs::write(&file, CONFIG).unwrap();

    let loaded = config::load(&file).unwrap().network;

    let mut workspaces = std::collections::BTreeMap::new();
    workspaces.insert(
        "01J00000000000000000000900".to_owned(),
        WorkspaceNetworkConfig {
            proxy: Some(ProxyConfig {
                mode: ProxyMode::None,
                ..ProxyConfig::default()
            }),
        },
    );
    assert_eq!(
        loaded,
        NetworkConfig {
            ca_bundle: Some(PathBuf::from("/etc/ssl/corp-ca.pem")),
            proxy: ProxyConfig {
                bypass: vec!["localhost".to_owned(), ".corp".to_owned()],
                ..manual("http://proxy.corp:3128", Some("me"))
            },
            certificates: vec![
                CertificateConfig {
                    host: "api.example.com".to_owned(),
                    port: Some(8443),
                    cert: Some(PathBuf::from("/certs/client.pem")),
                    key: Some(PathBuf::from("/certs/client-key.pem")),
                    pkcs12: None,
                },
                CertificateConfig {
                    host: "*.internal".to_owned(),
                    pkcs12: Some(PathBuf::from("/certs/client.p12")),
                    ..CertificateConfig::default()
                },
            ],
            workspaces,
        }
    );
}

#[test]
fn a_config_without_network_is_the_system_proxy_and_no_certificates() {
    let dir = temp_dir();
    let file = dir.path().join("config.toml");
    fs::write(&file, "theme = \"dark\"\n").unwrap();

    let loaded = config::load(&file).unwrap();

    assert_eq!(loaded.network, NetworkConfig::default());
    let (settings, warnings) =
        network_settings(&loaded.network, id(900), &MemorySecretStore::new());
    assert_eq!(settings, NetworkSettings::default());
    assert_eq!(settings.proxy, ProxySetting::System);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn passwords_and_passphrases_come_from_the_secret_store() {
    let store = MemorySecretStore::new();
    store
        .set_account(
            &NetworkSecret::ProxyPassword { workspace: None }.account(),
            "proxy-pw",
        )
        .unwrap();
    let passphrase = NetworkSecret::CertificatePassphrase {
        host: "*.Internal".to_owned(),
        port: None,
    };
    store.set_account(&passphrase.account(), "p12-pw").unwrap();
    let config = NetworkConfig {
        proxy: manual("http://proxy.corp:3128", Some("me")),
        certificates: vec![CertificateConfig {
            host: "*.internal".to_owned(),
            pkcs12: Some(PathBuf::from("/certs/client.p12")),
            ..CertificateConfig::default()
        }],
        ..NetworkConfig::default()
    };

    let (settings, warnings) = network_settings(&config, id(900), &store);

    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        settings,
        NetworkSettings {
            ca_bundle: None,
            certificates: vec![ClientCertificate {
                host: "*.internal".to_owned(),
                port: None,
                file: CertificateFile::Pkcs12 {
                    path: PathBuf::from("/certs/client.p12"),
                    passphrase: Some("p12-pw".to_owned()),
                },
            }],
            proxy: ProxySetting::Manual(ManualProxy {
                url: "http://proxy.corp:3128".to_owned(),
                username: Some("me".to_owned()),
                password: Some("proxy-pw".to_owned()),
                bypass: Vec::new(),
            }),
        }
    );
}

#[test]
fn network_secret_accounts_are_stable_and_never_collide_with_variables() {
    assert_eq!(
        NetworkSecret::ProxyPassword { workspace: None }.account(),
        "network/proxy"
    );
    assert_eq!(
        NetworkSecret::ProxyPassword {
            workspace: Some(id(900))
        }
        .account(),
        format!("network/workspace/{}/proxy", id(900))
    );
    assert_eq!(
        NetworkSecret::CertificatePassphrase {
            host: "API.example.com".to_owned(),
            port: Some(8443),
        }
        .account(),
        "network/certificate/api.example.com:8443"
    );
}

#[test]
fn a_workspace_proxy_replaces_the_global_one_with_its_own_password() {
    let store = MemorySecretStore::new();
    store
        .set_account(
            &NetworkSecret::ProxyPassword { workspace: None }.account(),
            "global-pw",
        )
        .unwrap();
    store
        .set_account(
            &NetworkSecret::ProxyPassword {
                workspace: Some(id(900)),
            }
            .account(),
            "workspace-pw",
        )
        .unwrap();
    let mut config = NetworkConfig {
        proxy: manual("http://global.corp:1", Some("g")),
        ..NetworkConfig::default()
    };
    config.workspaces.insert(
        id(900).to_string().to_lowercase(),
        WorkspaceNetworkConfig {
            proxy: Some(manual("socks5://ws.corp:2", Some("w"))),
        },
    );

    let (mine, _) = network_settings(&config, id(900), &store);
    let (other, _) = network_settings(&config, id(901), &store);

    let ProxySetting::Manual(mine) = mine.proxy else {
        panic!("{:?}", mine.proxy)
    };
    assert_eq!(mine.url, "socks5://ws.corp:2");
    assert_eq!(mine.password.as_deref(), Some("workspace-pw"));
    let ProxySetting::Manual(other) = other.proxy else {
        panic!("{:?}", other.proxy)
    };
    assert_eq!(other.url, "http://global.corp:1");
    assert_eq!(other.password.as_deref(), Some("global-pw"));
}

#[test]
fn broken_entries_are_skipped_with_a_warning_that_never_holds_a_secret() {
    let store = MemorySecretStore::unavailable("no keyring here");
    let config = NetworkConfig {
        ca_bundle: Some(PathBuf::from("relative/ca.pem")),
        proxy: ProxyConfig {
            mode: ProxyMode::Manual,
            ..ProxyConfig::default()
        },
        certificates: vec![
            CertificateConfig {
                host: "a.test".to_owned(),
                ..CertificateConfig::default()
            },
            CertificateConfig {
                host: "b.test".to_owned(),
                cert: Some(PathBuf::from("/c.pem")),
                key: Some(PathBuf::from("/k.pem")),
                pkcs12: Some(PathBuf::from("/c.p12")),
                ..CertificateConfig::default()
            },
            CertificateConfig {
                host: String::new(),
                pkcs12: Some(PathBuf::from("/c.p12")),
                ..CertificateConfig::default()
            },
            CertificateConfig {
                host: "c.test".to_owned(),
                pkcs12: Some(PathBuf::from("/c.p12")),
                ..CertificateConfig::default()
            },
        ],
        ..NetworkConfig::default()
    };

    let (settings, warnings) = network_settings(&config, id(900), &store);

    assert_eq!(settings.ca_bundle, None);
    assert_eq!(settings.proxy, ProxySetting::System);
    assert_eq!(
        settings
            .certificates
            .iter()
            .map(|certificate| certificate.host.as_str())
            .collect::<Vec<_>>(),
        vec!["c.test"]
    );
    assert_eq!(
        warnings,
        vec![
            "[network] ca_bundle relative/ca.pem is not an absolute path; it is ignored".to_owned(),
            "[network.proxy] mode = \"manual\" has no url; the system proxy is used".to_owned(),
            "[[network.certificates]] a.test needs cert and key, or pkcs12; it is ignored"
                .to_owned(),
            "[[network.certificates]] b.test sets both cert and pkcs12; it is ignored".to_owned(),
            "[[network.certificates]] an entry has no host; it is ignored".to_owned(),
            "[[network.certificates]] c.test: cannot read its passphrase from the OS keyring: \
             the OS keyring is not available: no keyring here"
                .to_owned(),
        ]
    );
}

#[test]
fn an_engine_sends_through_the_proxy_it_was_given() {
    let runtime = common::runtime();
    // `me:pw-1` as basic credentials.
    let proxy = runtime.block_on(async {
        let proxy = MockServer::start().await;
        Mock::given(path("/hello"))
            .and(header("proxy-authorization", "Basic bWU6cHctMQ=="))
            .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
            .mount(&proxy)
            .await;
        proxy
    });
    let dir = temp_dir();
    common::write_get_workspace(dir.path(), "http://target.invalid/hello");
    let store = Arc::new(MemorySecretStore::new());
    store
        .set_account(
            &NetworkSecret::ProxyPassword { workspace: None }.account(),
            "pw-1",
        )
        .unwrap();
    let engine = Engine::open_with_secrets(dir.path(), store).unwrap();
    let config = NetworkConfig {
        proxy: manual(&proxy.uri(), Some("me")),
        ..NetworkConfig::default()
    };

    let warnings = engine.apply_network(&config);
    let executed = engine
        .execute_blocking("api/r", &ExecuteOptions::default())
        .unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(executed.response.body, b"via proxy");
    let ProxySetting::Manual(applied) = &engine.network_settings().proxy else {
        panic!("the manual proxy was not applied")
    };
    assert_eq!(applied.password.as_deref(), Some("pw-1"));
}

#[test]
fn a_failed_send_through_a_proxy_never_shows_its_password() {
    let dir = temp_dir();
    common::write_get_workspace(dir.path(), "http://target.invalid/hello");
    let store = Arc::new(MemorySecretStore::new());
    store
        .set_account(
            &NetworkSecret::ProxyPassword { workspace: None }.account(),
            "pw-secret-1",
        )
        .unwrap();
    let engine = Engine::open_with_secrets(dir.path(), store).unwrap();
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let warnings = engine.apply_network(&NetworkConfig {
        proxy: manual(
            &format!("http://me:pw-secret-1@127.0.0.1:{port}"),
            Some("me"),
        ),
        ..NetworkConfig::default()
    });
    assert!(warnings.is_empty(), "{warnings:?}");

    let error = engine
        .execute_blocking("api/r", &ExecuteOptions::default())
        .unwrap_err()
        .to_string();

    assert!(!error.contains("pw-secret-1"), "{error}");
    assert!(!format!("{:?}", engine.network_settings()).contains("pw-secret-1"));
}
