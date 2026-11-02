//! Network settings (v1-features S5): `[network]` of the user's config file, with its
//! passwords read from the secret store, becomes the [`NetworkSettings`] an engine's client
//! applies to every send: the main request, its auth requests and the scripts'
//! `pm.sendRequest`.
//!
//! The settings are per user and machine, so they live in the config file and the OS keyring,
//! never in a workspace file. A proxy password is kept under
//! [`NetworkSecret::ProxyPassword`] and a PKCS#12 passphrase under
//! [`NetworkSecret::CertificatePassphrase`], in the keyring service every ReqFlux secret uses.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqflux_core::model::Id;
use reqflux_http::{
    CertificateFile, ClientCertificate, ManualProxy, NetworkSettings, ProxySetting,
};

use crate::config::{CertificateConfig, NetworkConfig, ProxyConfig, ProxyMode};
use crate::engine::Engine;
use crate::secrets::SecretStore;

/// A network secret kept in the secret store, never in a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NetworkSecret {
    /// The password of `[network.proxy]` (`workspace: None`) or of the proxy of
    /// `[network.workspaces."<id>"]`.
    ProxyPassword {
        /// The workspace whose own proxy it belongs to; `None` for the global proxy.
        workspace: Option<Id>,
    },
    /// The passphrase of the PKCS#12 certificate entry for `host` and `port`, as the
    /// `[[network.certificates]]` entry writes them.
    CertificatePassphrase {
        /// The entry's `host`, in any case.
        host: String,
        /// The entry's `port`.
        port: Option<u16>,
    },
}

impl NetworkSecret {
    /// The keyring account: `network/proxy`, `network/workspace/<id>/proxy` or
    /// `network/certificate/<host>[:<port>]` with the host lowercase. A secret variable's
    /// account starts with a workspace id, so the two never collide.
    #[must_use]
    pub fn account(&self) -> String {
        match self {
            Self::ProxyPassword { workspace: None } => "network/proxy".to_owned(),
            Self::ProxyPassword {
                workspace: Some(id),
            } => format!("network/workspace/{id}/proxy"),
            Self::CertificatePassphrase { host, port } => {
                let host = host.trim().to_ascii_lowercase();
                match port {
                    Some(port) => format!("network/certificate/{host}:{port}"),
                    None => format!("network/certificate/{host}"),
                }
            }
        }
    }
}

/// The transport settings `config` means for the workspace `workspace`, its passwords and
/// passphrases read from `secrets`, and a warning for every entry that was skipped or
/// whose secret could not be read. A warning names the entry, never a secret.
///
/// A path must be absolute (`~/` expands to the home directory); a relative one is skipped.
/// A proxy `mode = "manual"` without a `url` falls back to the system proxy.
#[must_use]
pub fn network_settings(
    config: &NetworkConfig,
    workspace: Id,
    secrets: &dyn SecretStore,
) -> (NetworkSettings, Vec<String>) {
    let mut warnings = Vec::new();
    let ca_bundle = config.ca_bundle.as_deref().and_then(|path| {
        absolute(path).or_else(|| {
            warnings.push(format!(
                "[network] ca_bundle {} is not an absolute path; it is ignored",
                path.display()
            ));
            None
        })
    });
    let own = config.workspaces.iter().find_map(|(key, entry)| {
        let matches = key.parse::<Id>().is_ok_and(|key| key == workspace);
        matches.then_some(entry.proxy.as_ref()).flatten()
    });
    let proxy = match own {
        Some(proxy) => proxy_setting(proxy, Some(workspace), secrets, &mut warnings),
        None => proxy_setting(&config.proxy, None, secrets, &mut warnings),
    };
    let certificates = config
        .certificates
        .iter()
        .filter_map(|entry| certificate(entry, secrets, &mut warnings))
        .collect();
    (
        NetworkSettings {
            ca_bundle,
            certificates,
            proxy,
        },
        warnings,
    )
}

fn proxy_setting(
    proxy: &ProxyConfig,
    workspace: Option<Id>,
    secrets: &dyn SecretStore,
    warnings: &mut Vec<String>,
) -> ProxySetting {
    let table = match workspace {
        Some(id) => format!("[network.workspaces.\"{id}\".proxy]"),
        None => "[network.proxy]".to_owned(),
    };
    match proxy.mode {
        ProxyMode::System => ProxySetting::System,
        ProxyMode::None => ProxySetting::None,
        ProxyMode::Manual => {
            let Some(url) = proxy.url.as_deref().filter(|url| !url.trim().is_empty()) else {
                warnings.push(format!(
                    "{table} mode = \"manual\" has no url; the system proxy is used"
                ));
                return ProxySetting::System;
            };
            let username = proxy.username.clone().filter(|name| !name.is_empty());
            let password = match username {
                Some(_) => read_secret(
                    secrets,
                    &NetworkSecret::ProxyPassword { workspace },
                    &format!("{table}: cannot read its password"),
                    warnings,
                ),
                None => None,
            };
            ProxySetting::Manual(ManualProxy {
                url: url.trim().to_owned(),
                username,
                password,
                bypass: proxy.bypass.clone(),
            })
        }
    }
}

fn certificate(
    entry: &CertificateConfig,
    secrets: &dyn SecretStore,
    warnings: &mut Vec<String>,
) -> Option<ClientCertificate> {
    const TABLE: &str = "[[network.certificates]]";
    let host = entry.host.trim();
    if host.is_empty() {
        warnings.push(format!("{TABLE} an entry has no host; it is ignored"));
        return None;
    }
    let path = |path: &Path, warnings: &mut Vec<String>| {
        absolute(path).or_else(|| {
            warnings.push(format!(
                "{TABLE} {host}: {} is not an absolute path; it is ignored",
                path.display()
            ));
            None
        })
    };
    let file = match (&entry.cert, &entry.key, &entry.pkcs12) {
        (Some(cert), Some(key), None) => CertificateFile::Pem {
            cert: path(cert, warnings)?,
            key: path(key, warnings)?,
        },
        (None, None, Some(archive)) => {
            let archive = path(archive, warnings)?;
            let passphrase = read_secret(
                secrets,
                &NetworkSecret::CertificatePassphrase {
                    host: host.to_owned(),
                    port: entry.port,
                },
                &format!("{TABLE} {host}: cannot read its passphrase"),
                warnings,
            );
            CertificateFile::Pkcs12 {
                path: archive,
                passphrase,
            }
        }
        (_, _, Some(_)) => {
            warnings.push(format!(
                "{TABLE} {host} sets both cert and pkcs12; it is ignored"
            ));
            return None;
        }
        _ => {
            warnings.push(format!(
                "{TABLE} {host} needs cert and key, or pkcs12; it is ignored"
            ));
            return None;
        }
    };
    Some(ClientCertificate {
        host: host.to_owned(),
        port: entry.port,
        file,
    })
}

/// The secret, or `None` with a warning starting with `context` when the store failed.
fn read_secret(
    secrets: &dyn SecretStore,
    secret: &NetworkSecret,
    context: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    match secrets.get_account(&secret.account()) {
        Ok(value) => value,
        Err(error) => {
            warnings.push(format!("{context} from the OS keyring: {error}"));
            None
        }
    }
}

/// `path` when it is absolute, with a leading `~/` expanded to the home directory.
fn absolute(path: &Path) -> Option<PathBuf> {
    if let Ok(rest) = path.strip_prefix("~") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        return home.is_absolute().then(|| home.join(rest));
    }
    path.is_absolute().then(|| path.to_path_buf())
}

impl Engine {
    /// Applies the `[network]` settings `config` means for this workspace to every send
    /// that starts after this call, reading passwords and passphrases from the engine's
    /// secret store. Returns the warnings of [`network_settings`]; the entries they name
    /// are skipped.
    #[must_use = "the warnings name the settings that were skipped"]
    pub fn apply_network(&self, config: &NetworkConfig) -> Vec<String> {
        let (settings, warnings) =
            network_settings(config, self.workspace().manifest.id, self.secrets.as_ref());
        self.client.set_network(settings);
        warnings
    }

    /// The network settings sends use: the default (system proxy, no custom CA, no client
    /// certificate) until [`Engine::apply_network`] sets others.
    #[must_use]
    pub fn network_settings(&self) -> Arc<NetworkSettings> {
        self.client.network()
    }
}
