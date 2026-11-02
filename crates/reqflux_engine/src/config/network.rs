//! `[network]` (v1-features S5): the CA bundle, the client certificates and the proxy, per
//! user and machine. They live in the user's config file, never in a workspace file, since a
//! certificate path or a corporate proxy belongs to one machine, not to everyone who shares a
//! workspace. Passwords and passphrases are never in this file: they are in the OS keyring
//! under the accounts [`NetworkSecret`](crate::NetworkSecret) names.
//!
//! ```toml
//! [network]
//! ca_bundle = "/etc/ssl/corp-ca.pem"
//!
//! [network.proxy]
//! mode = "manual"            # "system" (the default), "none" or "manual"
//! url = "http://proxy.corp:3128"
//! username = "me"
//! bypass = ["localhost", ".corp"]
//!
//! [[network.certificates]]
//! host = "api.example.com"   # `*.example.com` matches its subdomains
//! port = 8443                # optional: every port when absent
//! cert = "/home/me/certs/client.pem"
//! key = "/home/me/certs/client-key.pem"
//!
//! [[network.certificates]]
//! host = "*.internal"
//! pkcs12 = "/home/me/certs/client.p12"
//!
//! [network.workspaces."<workspace id>".proxy]
//! mode = "none"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

/// `[network]`. [`Default`] is no CA bundle, no client certificate and the system proxy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// `ca_bundle`: a PEM file of CA certificates trusted beside the platform's roots.
    pub ca_bundle: Option<PathBuf>,
    /// `[network.proxy]`: the proxy of every workspace without its own.
    pub proxy: ProxyConfig,
    /// `[[network.certificates]]`, in match order.
    pub certificates: Vec<CertificateConfig>,
    /// `[network.workspaces."<workspace id>"]`: settings of one workspace, by workspace id.
    pub workspaces: BTreeMap<String, WorkspaceNetworkConfig>,
}

/// `[network.proxy]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    /// `mode`.
    pub mode: ProxyMode,
    /// `url`: `http://`, `https://`, `socks5://` or `socks5h://` with the host and port;
    /// used when `mode = "manual"`.
    pub url: Option<String>,
    /// `username`; the password is in the keyring.
    pub username: Option<String>,
    /// `bypass`: hosts sent directly, in `NO_PROXY` syntax.
    pub bypass: Vec<String>,
}

/// `mode` of `[network.proxy]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// The system proxy: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`.
    #[default]
    System,
    /// No proxy, whatever the environment says.
    None,
    /// The proxy `url`.
    Manual,
}

/// One `[[network.certificates]]` entry: `cert` and `key` (PEM), or `pkcs12`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CertificateConfig {
    /// `host`, in any case; `*.example.com` matches every subdomain of `example.com`.
    pub host: String,
    /// `port`; every port when absent.
    pub port: Option<u16>,
    /// `cert`: the PEM certificate (chain).
    pub cert: Option<PathBuf>,
    /// `key`: the PEM private key.
    pub key: Option<PathBuf>,
    /// `pkcs12`: a PKCS#12 archive; its passphrase is in the keyring.
    pub pkcs12: Option<PathBuf>,
}

/// `[network.workspaces."<workspace id>"]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct WorkspaceNetworkConfig {
    /// `proxy`: replaces `[network.proxy]` for this workspace.
    pub proxy: Option<ProxyConfig>,
}
