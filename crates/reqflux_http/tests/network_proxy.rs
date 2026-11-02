//! Proxies (v1-features S5): a manual HTTP or SOCKS5 proxy with optional auth, its bypass
//! list, no proxy at all, and the timings of a send through a proxy.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use reqflux_http::{
    HttpClient, HttpError, ManualProxy, NetworkSettings, PreparedBody, PreparedRequest,
    ProxySetting, RequestOptions, Response,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn get(url: String) -> PreparedRequest {
    PreparedRequest {
        method: "GET".to_owned(),
        url,
        headers: Vec::new(),
        body: PreparedBody::None,
        options: RequestOptions {
            timeout: Duration::from_secs(10),
            follow_redirects: false,
            verify_tls: true,
        },
    }
}

fn manual(url: String, credentials: Option<(&str, &str)>, bypass: &[&str]) -> NetworkSettings {
    NetworkSettings {
        proxy: ProxySetting::Manual(ManualProxy {
            url,
            username: credentials.map(|(user, _)| user.to_owned()),
            password: credentials.map(|(_, password)| password.to_owned()),
            bypass: bypass.iter().map(|entry| (*entry).to_owned()).collect(),
        }),
        ..NetworkSettings::default()
    }
}

async fn send(client: &HttpClient, url: String) -> Result<Response, HttpError> {
    client.send(get(url), CancellationToken::new()).await
}

/// A port nothing listens on.
async fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// A mock acting as an HTTP proxy: it answers the absolute-form request for `/hello` when
/// it carries `Proxy-Authorization: Basic bWU6cHctMQ==` (`me:pw-1`).
async fn http_proxy() -> MockServer {
    let proxy = MockServer::start().await;
    Mock::given(path("/hello"))
        .and(header("proxy-authorization", "Basic bWU6cHctMQ=="))
        .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
        .mount(&proxy)
        .await;
    proxy
}

#[tokio::test]
async fn a_manual_http_proxy_carries_the_send_with_its_credentials() {
    let proxy = http_proxy().await;
    let client = HttpClient::new();
    client.set_network(manual(proxy.uri(), Some(("me", "pw-1")), &[]));

    let response = send(&client, "http://target.invalid/hello".to_owned())
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"via proxy");
    let hop = response.timings.last().unwrap();
    assert!(hop.connect.is_some(), "{hop:?}");
}

#[tokio::test]
async fn a_bypassed_host_is_sent_directly() {
    let target = MockServer::start().await;
    Mock::given(path("/direct"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .mount(&target)
        .await;
    let client = HttpClient::new();
    let proxy = format!("http://127.0.0.1:{}", closed_port().await);
    client.set_network(manual(proxy, None, &["127.0.0.1"]));

    let response = send(&client, format!("{}/direct", target.uri()))
        .await
        .unwrap();

    assert_eq!(response.body, b"direct");
}

#[tokio::test]
async fn an_unreachable_proxy_fails_the_send_without_its_password() {
    let client = HttpClient::new();
    let port = closed_port().await;
    client.set_network(manual(
        format!("http://me:url-secret@127.0.0.1:{port}"),
        Some(("me", "field-secret")),
        &[],
    ));

    let error = send(&client, "http://target.invalid/".to_owned())
        .await
        .unwrap_err();

    let message = error.to_string();
    assert!(!message.contains("url-secret"), "{message}");
    assert!(!message.contains("field-secret"), "{message}");
}

#[tokio::test]
async fn no_proxy_sends_directly_and_switching_settings_takes_effect() {
    let target = MockServer::start().await;
    Mock::given(path("/direct"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .mount(&target)
        .await;
    let client = HttpClient::new();
    let proxy = format!("http://127.0.0.1:{}", closed_port().await);
    client.set_network(manual(proxy, None, &[]));
    assert!(
        send(&client, format!("{}/direct", target.uri()))
            .await
            .is_err()
    );

    client.set_network(NetworkSettings {
        proxy: ProxySetting::None,
        ..NetworkSettings::default()
    });
    let response = send(&client, format!("{}/direct", target.uri()))
        .await
        .unwrap();

    assert_eq!(response.body, b"direct");
}

/// What the SOCKS5 server saw: the username and password, when the client authenticated.
type Seen = Arc<Mutex<Vec<(String, String)>>>;

/// A minimal SOCKS5 server (RFC 1928, username/password auth of RFC 1929) that tunnels every
/// CONNECT to the requested IPv4 address or domain name.
async fn socks5_server() -> (u16, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = Arc::default();
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(tunnel(stream, Arc::clone(&record)));
        }
    });
    (port, seen)
}

async fn tunnel(mut client: TcpStream, seen: Seen) -> std::io::Result<()> {
    let mut head = [0u8; 2];
    client.read_exact(&mut head).await?;
    let mut methods = vec![0u8; usize::from(head[1])];
    client.read_exact(&mut methods).await?;
    if methods.contains(&2) {
        client.write_all(&[5, 2]).await?;
        let mut version = [0u8; 2];
        client.read_exact(&mut version).await?;
        let mut user = vec![0u8; usize::from(version[1])];
        client.read_exact(&mut user).await?;
        let mut length = [0u8; 1];
        client.read_exact(&mut length).await?;
        let mut password = vec![0u8; usize::from(length[0])];
        client.read_exact(&mut password).await?;
        seen.lock().unwrap().push((
            String::from_utf8_lossy(&user).into_owned(),
            String::from_utf8_lossy(&password).into_owned(),
        ));
        client.write_all(&[1, 0]).await?;
    } else {
        client.write_all(&[5, 0]).await?;
    }
    let mut request = [0u8; 4];
    client.read_exact(&mut request).await?;
    let host = match request[3] {
        1 => {
            let mut address = [0u8; 4];
            client.read_exact(&mut address).await?;
            std::net::Ipv4Addr::from(address).to_string()
        }
        3 => {
            let mut length = [0u8; 1];
            client.read_exact(&mut length).await?;
            let mut name = vec![0u8; usize::from(length[0])];
            client.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        _ => return Ok(()),
    };
    let mut port = [0u8; 2];
    client.read_exact(&mut port).await?;
    let mut upstream = TcpStream::connect((host.as_str(), u16::from_be_bytes(port))).await?;
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

#[tokio::test]
async fn a_socks5_proxy_with_credentials_tunnels_the_send() {
    let target = MockServer::start().await;
    Mock::given(path("/tunnel"))
        .respond_with(ResponseTemplate::new(200).set_body_string("tunnelled"))
        .mount(&target)
        .await;
    let (port, seen) = socks5_server().await;
    let client = HttpClient::new();
    client.set_network(manual(
        format!("socks5://127.0.0.1:{port}"),
        Some(("sock-user", "sock-pass")),
        &[],
    ));

    let response = send(&client, format!("{}/tunnel", target.uri()))
        .await
        .unwrap();

    assert_eq!(response.body, b"tunnelled");
    assert_eq!(
        seen.lock().unwrap().clone(),
        vec![("sock-user".to_owned(), "sock-pass".to_owned())]
    );
}

#[tokio::test]
async fn an_invalid_proxy_url_fails_the_send_as_a_network_error() {
    let client = HttpClient::new();
    client.set_network(manual("not a url".to_owned(), None, &[]));

    let error = send(&client, "http://target.invalid/".to_owned())
        .await
        .unwrap_err();

    assert!(matches!(error, HttpError::Network(_)), "{error}");
}
