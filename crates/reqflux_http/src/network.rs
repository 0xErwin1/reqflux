//! The network settings every send of a client applies (v1-features S5): a custom CA bundle,
//! client certificates matched by host and port (Postman's model), and the proxy.
//!
//! The settings carry no ReqFlux model types; `reqflux_engine` builds them from the user's
//! config file and the OS keyring. A passphrase or a proxy password is held in memory only,
//! and the `Debug` output of every type here hides it.

use std::fmt;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use url::Url;

use crate::error::HttpError;

/// What replaces a network secret in an error text, the mask history uses.
const MASK: &str = "••••••••";

/// The TLS and proxy settings of a client. [`Default`] is no custom CA, no client
/// certificate and the system proxy, the transport's behavior before these settings existed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct NetworkSettings {
    /// A PEM file of CA certificates trusted in addition to the platform's roots.
    pub ca_bundle: Option<PathBuf>,
    /// Client certificates; the first whose host and port match the request is presented.
    pub certificates: Vec<ClientCertificate>,
    /// How sends reach the network.
    pub proxy: ProxySetting,
}

impl fmt::Debug for NetworkSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkSettings")
            .field("ca_bundle", &self.ca_bundle)
            .field("certificates", &self.certificates)
            .field("proxy", &self.proxy)
            .finish()
    }
}

impl NetworkSettings {
    /// The index of the first certificate matching `url`'s host and port, if any.
    pub(crate) fn certificate_for(&self, url: &Url) -> Option<usize> {
        let host = url.host_str()?;
        let port = url.port_or_known_default()?;
        self.certificates
            .iter()
            .position(|certificate| certificate.matches(host, port))
    }

    /// `text` with every passphrase and proxy password of these settings masked, in the
    /// forms an error can carry them: as written and percent-encoded.
    pub(crate) fn mask(&self, text: &str) -> String {
        let mut secrets: Vec<String> = Vec::new();
        for certificate in &self.certificates {
            if let CertificateFile::Pkcs12 {
                passphrase: Some(passphrase),
                ..
            } = &certificate.file
            {
                secrets.push(passphrase.clone());
            }
        }
        if let ProxySetting::Manual(proxy) = &self.proxy {
            secrets.extend(proxy.password.clone());
            if let Ok(url) = Url::parse(&proxy.url)
                && let Some(password) = url.password()
            {
                secrets.push(password.to_owned());
                if let Ok(decoded) = urlencoding_decode(password) {
                    secrets.push(decoded);
                }
            }
        }
        secrets.retain(|secret| !secret.is_empty());
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        secrets
            .iter()
            .fold(text.to_owned(), |text, secret| text.replace(secret, MASK))
    }
}

/// `text` percent-decoded, as a URL's userinfo is.
fn urlencoding_decode(text: &str) -> Result<String, std::str::Utf8Error> {
    url::form_urlencoded::parse(format!("x={text}").as_bytes())
        .next()
        .map_or(Ok(String::new()), |(_, value)| Ok(value.into_owned()))
}

/// A client certificate presented to the hosts it matches.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientCertificate {
    /// The host it applies to, in any case; `*.example.com` matches every subdomain of
    /// `example.com` but not `example.com` itself.
    pub host: String,
    /// The port it applies to; `None` matches every port.
    pub port: Option<u16>,
    /// Where the certificate and its key are.
    pub file: CertificateFile,
}

impl fmt::Debug for ClientCertificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientCertificate")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("file", &self.file)
            .finish()
    }
}

impl ClientCertificate {
    fn matches(&self, host: &str, port: u16) -> bool {
        if self.port.is_some_and(|wanted| wanted != port) {
            return false;
        }
        let pattern = self.host.trim().trim_end_matches('.').to_ascii_lowercase();
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        match pattern.strip_prefix("*.") {
            Some(parent) => host
                .strip_suffix(parent)
                .is_some_and(|label| label.len() > 1 && label.ends_with('.')),
            None => pattern == host,
        }
    }
}

/// The files of a [`ClientCertificate`].
#[derive(Clone, PartialEq, Eq)]
pub enum CertificateFile {
    /// A PEM certificate (chain) and a PEM private key.
    Pem {
        /// The certificate file.
        cert: PathBuf,
        /// The private key file.
        key: PathBuf,
    },
    /// A PKCS#12 archive holding the key and its certificate chain.
    Pkcs12 {
        /// The archive.
        path: PathBuf,
        /// The archive passphrase; `None` reads it with an empty one.
        passphrase: Option<String>,
    },
}

impl fmt::Debug for CertificateFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pem { cert, key } => formatter
                .debug_struct("Pem")
                .field("cert", cert)
                .field("key", key)
                .finish(),
            Self::Pkcs12 { path, passphrase } => formatter
                .debug_struct("Pkcs12")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| MASK))
                .finish(),
        }
    }
}

/// How a client reaches the network.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum ProxySetting {
    /// The system proxy: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` (in either
    /// case) from the environment, plus the platform's settings on macOS and Windows.
    #[default]
    System,
    /// Every send goes directly, whatever the environment says.
    None,
    /// The proxy given.
    Manual(ManualProxy),
}

impl fmt::Debug for ProxySetting {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => formatter.write_str("System"),
            Self::None => formatter.write_str("None"),
            Self::Manual(proxy) => formatter.debug_tuple("Manual").field(proxy).finish(),
        }
    }
}

/// A proxy set by hand.
#[derive(Clone, PartialEq, Eq)]
pub struct ManualProxy {
    /// `http://`, `https://`, `socks5://` (names resolved locally) or `socks5h://` (names
    /// resolved by the proxy), with the host and port; credentials in the URL also work.
    pub url: String,
    /// The proxy username.
    pub username: Option<String>,
    /// The proxy password, sent only with a username.
    pub password: Option<String>,
    /// Hosts sent directly: a host name matches itself and its subdomains, `*` matches
    /// every host, and an IP address or CIDR range matches those addresses. The syntax of
    /// `NO_PROXY`.
    pub bypass: Vec<String>,
}

impl fmt::Debug for ManualProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = Url::parse(&self.url).map_or_else(
            |_| "<invalid>".to_owned(),
            |mut url| {
                if url.password().is_some() && url.set_password(Some(MASK)).is_err() {
                    return "<redacted>".to_owned();
                }
                url.to_string()
            },
        );
        formatter
            .debug_struct("ManualProxy")
            .field("url", &url)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| MASK))
            .field("bypass", &self.bypass)
            .finish()
    }
}

/// Where a client keeps cookies between sends: every hop of a send asks it for the `Cookie`
/// header of its URL and gives it the `Set-Cookie` headers it received.
pub trait CookieJar: fmt::Debug + Send + Sync {
    /// The `Cookie` header value for a request to `url`, or `None` when no cookie matches.
    fn cookie_header(&self, url: &Url) -> Option<String>;

    /// Stores the `Set-Cookie` header values a response from `url` carried.
    fn store(&self, url: &Url, set_cookies: &[String]);
}

/// Applies `settings` to `builder`, presenting `certificate` (an index into
/// `settings.certificates`) when given.
pub(crate) fn configure(
    mut builder: reqwest::ClientBuilder,
    settings: &NetworkSettings,
    certificate: Option<usize>,
) -> Result<reqwest::ClientBuilder, HttpError> {
    if let Some(path) = &settings.ca_bundle {
        let pem = read(path)?;
        let roots = reqwest::Certificate::from_pem_bundle(&pem)
            .map_err(|error| invalid(path, "is not a PEM certificate bundle", &error))?;
        if roots.is_empty() {
            return Err(HttpError::Network(format!(
                "the CA bundle {} holds no certificate",
                path.display()
            )));
        }
        builder = builder.tls_certs_merge(roots);
    }
    if let Some(certificate) = certificate.and_then(|index| settings.certificates.get(index)) {
        builder = builder.identity(identity(&certificate.file)?);
    }
    builder = match &settings.proxy {
        ProxySetting::System => builder,
        ProxySetting::None => builder.no_proxy(),
        ProxySetting::Manual(proxy) => builder.proxy(manual_proxy(proxy)?),
    };
    Ok(builder)
}

fn manual_proxy(proxy: &ManualProxy) -> Result<reqwest::Proxy, HttpError> {
    let url = Url::parse(proxy.url.trim())
        .map_err(|error| HttpError::Network(format!("the proxy URL is not valid: {error}")))?;
    let mut built = reqwest::Proxy::all(url.as_str())
        .map_err(|error| HttpError::Network(format!("the proxy URL is not usable: {error}")))?;
    if let Some(username) = proxy.username.as_deref().filter(|name| !name.is_empty()) {
        built = built.basic_auth(username, proxy.password.as_deref().unwrap_or_default());
    }
    let bypass: Vec<&str> = proxy
        .bypass
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect();
    Ok(built.no_proxy(reqwest::NoProxy::from_string(&bypass.join(","))))
}

/// The identity rustls presents: the private key and the certificate chain as one PEM.
fn identity(file: &CertificateFile) -> Result<reqwest::Identity, HttpError> {
    let (pem, path) = match file {
        CertificateFile::Pem { cert, key } => {
            let mut pem = read(key)?;
            pem.push(b'\n');
            pem.extend(read(cert)?);
            (pem, cert)
        }
        CertificateFile::Pkcs12 { path, passphrase } => (
            pkcs12_pem(path, passphrase.as_deref().unwrap_or_default())?,
            path,
        ),
    };
    reqwest::Identity::from_pem(&pem)
        .map_err(|error| invalid(path, "is not a usable client certificate and key", &error))
}

/// The key and certificate chain of the PKCS#12 archive at `path` as PEM. The archive
/// library's error is not shown: it is described, so it can never carry the passphrase.
fn pkcs12_pem(path: &Path, passphrase: &str) -> Result<Vec<u8>, HttpError> {
    let data = read(path)?;
    let store = p12_keystore::KeyStore::from_pkcs12(
        &data,
        passphrase,
        p12_keystore::Pkcs12ImportPolicy::Strict,
    )
    .map_err(|_| {
        HttpError::Network(format!(
            "cannot read the PKCS#12 file {}: the passphrase is wrong or the file is damaged",
            path.display()
        ))
    })?;
    let chain = store.entries().find_map(|(_, entry)| match entry {
        p12_keystore::KeyStoreEntry::PrivateKeyChain(chain) => Some(chain),
        _ => None,
    });
    let Some(chain) = chain else {
        return Err(HttpError::Network(format!(
            "the PKCS#12 file {} holds no private key",
            path.display()
        )));
    };
    let mut pem = pem_block("PRIVATE KEY", chain.key().as_der());
    for certificate in chain.certs() {
        pem.push_str(&pem_block("CERTIFICATE", certificate.as_der()));
    }
    Ok(pem.into_bytes())
}

fn pem_block(label: &str, der: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut block = format!("-----BEGIN {label}-----\n");
    let mut rest = encoded.as_str();
    while !rest.is_empty() {
        let (line, tail) = rest.split_at(rest.len().min(64));
        block.push_str(line);
        block.push('\n');
        rest = tail;
    }
    block.push_str(&format!("-----END {label}-----\n"));
    block
}

fn read(path: &Path) -> Result<Vec<u8>, HttpError> {
    std::fs::read(path)
        .map_err(|error| HttpError::Network(format!("cannot read {}: {error}", path.display())))
}

fn invalid(path: &Path, what: &str, error: &reqwest::Error) -> HttpError {
    HttpError::Network(format!("{} {what}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certificate(host: &str, port: Option<u16>) -> ClientCertificate {
        ClientCertificate {
            host: host.to_owned(),
            port,
            file: CertificateFile::Pem {
                cert: PathBuf::new(),
                key: PathBuf::new(),
            },
        }
    }

    #[test]
    fn a_certificate_matches_its_host_in_any_case_and_its_port() {
        let exact = certificate("API.example.com", Some(8443));
        assert!(exact.matches("api.example.com", 8443));
        assert!(!exact.matches("api.example.com", 443));
        assert!(!exact.matches("other.example.com", 8443));
        assert!(certificate("api.example.com", None).matches("api.example.com", 1));
    }

    #[test]
    fn a_wildcard_matches_subdomains_only() {
        let wildcard = certificate("*.example.com", None);
        assert!(wildcard.matches("a.example.com", 443));
        assert!(wildcard.matches("a.b.example.com", 443));
        assert!(!wildcard.matches("example.com", 443));
        assert!(!wildcard.matches("badexample.com", 443));
    }

    #[test]
    fn the_first_matching_certificate_wins_and_the_default_port_counts() {
        let settings = NetworkSettings {
            certificates: vec![
                certificate("other.test", None),
                certificate("api.test", Some(443)),
                certificate("api.test", None),
            ],
            ..NetworkSettings::default()
        };
        let url = Url::parse("https://api.test/x").ok();
        assert_eq!(url.and_then(|url| settings.certificate_for(&url)), Some(1));
    }

    #[test]
    fn mask_hides_passwords_in_the_field_and_the_url() {
        let settings = NetworkSettings {
            proxy: ProxySetting::Manual(ManualProxy {
                url: "http://me:p%40ss@proxy.test:1".to_owned(),
                username: Some("me".to_owned()),
                password: Some("field-pw".to_owned()),
                bypass: Vec::new(),
            }),
            ..NetworkSettings::default()
        };
        let masked = settings.mask("a field-pw b p%40ss c p@ss");
        assert_eq!(masked, format!("a {MASK} b {MASK} c {MASK}"));
    }
}
