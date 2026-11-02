//! Custom CA bundles and per-host client certificates (v1-features S5) against a local TLS
//! server.
//!
//! The fixtures are throwaway test certificates generated with OpenSSL for these tests: a CA
//! (`fixtures/ca-cert.pem`), a server certificate for `127.0.0.1` it signed
//! (`server-cert.pem`, key `server-key.pem`) and a client certificate it signed
//! (`client-cert.pem`, key `client-key.pem`, and both as `client.p12` with the passphrase
//! `fixture-pass`).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use reqflux_http::{
    CertificateFile, ClientCertificate, HttpClient, HttpError, NetworkSettings, PreparedBody,
    PreparedRequest, RequestOptions,
};
use rustls_pemfile::{certs, private_key};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{RootCertStore, ServerConfig};
use tokio_util::sync::CancellationToken;

const CA_PEM: &str = include_str!("fixtures/ca-cert.pem");
const SERVER_CERT_PEM: &str = include_str!("fixtures/server-cert.pem");
const SERVER_KEY_PEM: &str = include_str!("fixtures/server-key.pem");
const PASSPHRASE: &str = "fixture-pass";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Starts a TLS server with the CA-signed certificate, requiring a client certificate the CA
/// signed when `client_auth` is set, and returns its port.
async fn start_server(client_auth: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let chain = certs(&mut SERVER_CERT_PEM.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = private_key(&mut SERVER_KEY_PEM.as_bytes())
        .unwrap()
        .unwrap();
    let provider = Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider());
    let builder = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .unwrap();
    let mut config = if client_auth {
        let mut roots = RootCertStore::empty();
        for ca in certs(&mut CA_PEM.as_bytes()) {
            roots.add(ca.unwrap()).unwrap();
        }
        let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .unwrap();
        builder
            .with_client_cert_verifier(verifier)
            .with_single_cert(chain, key)
            .unwrap()
    } else {
        builder
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap()
    };
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(config));
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve_once(acceptor.clone(), stream));
        }
    });
    port
}

async fn serve_once(acceptor: TlsAcceptor, stream: TcpStream) -> io::Result<()> {
    let mut tls = acceptor.accept(stream).await?;
    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    while !request.ends_with(b"\r\n\r\n") {
        let read = tls.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
    tls.write_all(response).await?;
    tls.shutdown().await
}

fn get(port: u16) -> PreparedRequest {
    PreparedRequest {
        method: "GET".to_owned(),
        url: format!("https://127.0.0.1:{port}/"),
        headers: Vec::new(),
        body: PreparedBody::None,
        options: RequestOptions {
            timeout: Duration::from_secs(10),
            follow_redirects: false,
            verify_tls: true,
        },
    }
}

fn with_ca() -> NetworkSettings {
    NetworkSettings {
        ca_bundle: Some(fixture("ca-cert.pem")),
        ..NetworkSettings::default()
    }
}

fn pem_certificate(host: &str, port: Option<u16>) -> ClientCertificate {
    ClientCertificate {
        host: host.to_owned(),
        port,
        file: CertificateFile::Pem {
            cert: fixture("client-cert.pem"),
            key: fixture("client-key.pem"),
        },
    }
}

async fn send(client: &HttpClient, port: u16) -> Result<reqflux_http::Response, HttpError> {
    client.send(get(port), CancellationToken::new()).await
}

#[tokio::test]
async fn a_server_signed_by_an_unknown_ca_is_refused_without_the_bundle() {
    let port = start_server(false).await;

    let error = send(&HttpClient::new(), port).await.unwrap_err();

    assert!(matches!(error, HttpError::Tls(_)), "{error}");
}

#[tokio::test]
async fn the_custom_ca_bundle_verifies_the_server() {
    let port = start_server(false).await;
    let client = HttpClient::new();
    client.set_network(with_ca());

    let response = send(&client, port).await.unwrap();

    assert_eq!(response.body, b"ok");
}

#[tokio::test]
async fn a_missing_ca_bundle_fails_the_send_naming_the_file() {
    let port = start_server(false).await;
    let client = HttpClient::new();
    let missing = fixture("missing-ca.pem");
    client.set_network(NetworkSettings {
        ca_bundle: Some(missing.clone()),
        ..NetworkSettings::default()
    });

    let error = send(&client, port).await.unwrap_err();

    assert!(matches!(error, HttpError::Network(_)), "{error}");
    assert!(
        error.to_string().contains(&missing.display().to_string()),
        "{error}"
    );
}

#[tokio::test]
async fn a_server_requiring_a_client_certificate_refuses_a_send_without_one() {
    let port = start_server(true).await;
    let client = HttpClient::new();
    client.set_network(with_ca());

    assert!(send(&client, port).await.is_err());
}

#[tokio::test]
async fn a_pem_client_certificate_matching_the_host_and_port_is_presented() {
    let port = start_server(true).await;
    let client = HttpClient::new();
    client.set_network(NetworkSettings {
        certificates: vec![pem_certificate("127.0.0.1", Some(port))],
        ..with_ca()
    });

    let response = send(&client, port).await.unwrap();

    assert_eq!(response.body, b"ok");
}

#[tokio::test]
async fn a_client_certificate_for_another_port_or_host_is_not_presented() {
    let port = start_server(true).await;
    let client = HttpClient::new();
    client.set_network(NetworkSettings {
        certificates: vec![
            pem_certificate("127.0.0.1", Some(port.wrapping_add(1))),
            pem_certificate("example.test", None),
        ],
        ..with_ca()
    });

    assert!(send(&client, port).await.is_err());
}

#[tokio::test]
async fn a_pkcs12_client_certificate_with_its_passphrase_is_presented() {
    let port = start_server(true).await;
    let client = HttpClient::new();
    client.set_network(NetworkSettings {
        certificates: vec![ClientCertificate {
            host: "127.0.0.1".to_owned(),
            port: None,
            file: CertificateFile::Pkcs12 {
                path: fixture("client.p12"),
                passphrase: Some(PASSPHRASE.to_owned()),
            },
        }],
        ..with_ca()
    });

    let response = send(&client, port).await.unwrap();

    assert_eq!(response.body, b"ok");
}

#[tokio::test]
async fn a_wrong_pkcs12_passphrase_fails_without_echoing_it() {
    let port = start_server(true).await;
    let client = HttpClient::new();
    let wrong = "wrong-passphrase-value";
    client.set_network(NetworkSettings {
        certificates: vec![ClientCertificate {
            host: "127.0.0.1".to_owned(),
            port: None,
            file: CertificateFile::Pkcs12 {
                path: fixture("client.p12"),
                passphrase: Some(wrong.to_owned()),
            },
        }],
        ..with_ca()
    });

    let error = send(&client, port).await.unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, HttpError::Network(_)), "{message}");
    assert!(message.contains("client.p12"), "{message}");
    assert!(!message.contains(wrong), "{message}");
}

#[test]
fn settings_debug_never_shows_a_passphrase_or_a_password() {
    let settings = NetworkSettings {
        certificates: vec![ClientCertificate {
            host: "h".to_owned(),
            port: None,
            file: CertificateFile::Pkcs12 {
                path: fixture("client.p12"),
                passphrase: Some("p12-secret".to_owned()),
            },
        }],
        proxy: reqflux_http::ProxySetting::Manual(reqflux_http::ManualProxy {
            url: "http://proxy.test:3128".to_owned(),
            username: Some("me".to_owned()),
            password: Some("proxy-secret".to_owned()),
            bypass: Vec::new(),
        }),
        ..NetworkSettings::default()
    };

    let debug = format!("{settings:?}");

    assert!(!debug.contains("p12-secret"), "{debug}");
    assert!(!debug.contains("proxy-secret"), "{debug}");
}
