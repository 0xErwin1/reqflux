//! The cookie jar hook (v1-features S5): every hop stores the `Set-Cookie` headers it
//! received and sends what the jar returns for its URL.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use reqflux_http::{CookieJar, HttpClient, PreparedBody, PreparedRequest, RequestOptions};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A jar that keeps every stored `name=value` pair and returns all of them for any URL.
#[derive(Debug, Default)]
struct RecordingJar {
    stored: Mutex<Vec<(String, String)>>,
}

impl CookieJar for RecordingJar {
    fn cookie_header(&self, _url: &Url) -> Option<String> {
        let stored = self.stored.lock().unwrap();
        let pairs: Vec<&str> = stored
            .iter()
            .map(|(_, value)| value.split(';').next().unwrap_or_default())
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    fn store(&self, url: &Url, set_cookies: &[String]) {
        let mut stored = self.stored.lock().unwrap();
        for value in set_cookies {
            stored.push((url.path().to_owned(), value.clone()));
        }
    }
}

fn get(url: String, headers: &[(&str, &str)]) -> PreparedRequest {
    PreparedRequest {
        method: "GET".to_owned(),
        url,
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: PreparedBody::None,
        options: RequestOptions {
            timeout: Duration::from_secs(10),
            follow_redirects: true,
            verify_tls: true,
        },
    }
}

#[tokio::test]
async fn a_cookie_set_by_a_redirect_is_sent_on_the_next_hop_beside_the_request_cookie() {
    let server = MockServer::start().await;
    Mock::given(path("/login"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", "/home")
                .insert_header("set-cookie", "session=abc; Path=/"),
        )
        .mount(&server)
        .await;
    Mock::given(path("/home"))
        .and(header("cookie", "own=1; session=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("home"))
        .mount(&server)
        .await;
    let jar = Arc::new(RecordingJar::default());
    let client = HttpClient::new();
    let installed: Arc<dyn CookieJar> = jar.clone();
    client.set_cookie_jar(Some(installed));

    let response = client
        .send(
            get(format!("{}/login", server.uri()), &[("Cookie", "own=1")]),
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(response.body, b"home");
    assert_eq!(
        jar.stored.lock().unwrap().clone(),
        vec![("/login".to_owned(), "session=abc; Path=/".to_owned())]
    );
}

#[tokio::test]
async fn without_a_jar_no_cookie_is_added() {
    let server = MockServer::start().await;
    Mock::given(path("/plain"))
        .respond_with(ResponseTemplate::new(200).set_body_string("plain"))
        .mount(&server)
        .await;

    let client = HttpClient::new();
    let response = client
        .send(
            get(format!("{}/plain", server.uri()), &[]),
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(response.body, b"plain");
    let received = server.received_requests().await.unwrap();
    assert!(received[0].headers.get("cookie").is_none());
}
