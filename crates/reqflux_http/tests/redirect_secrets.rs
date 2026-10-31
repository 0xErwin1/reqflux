//! Redirects never carry a secret to another origin (redirect-secret-headers S1-S4).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use reqflux_http::{
    HttpClient, HttpError, PreparedBody, PreparedRequest, RedirectRules, RequestOptions, Response,
    SecretMask,
};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "s3cr3t-token";

fn request(verb: &str, url: String, headers: &[(&str, &str)]) -> PreparedRequest {
    PreparedRequest {
        method: verb.to_owned(),
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

fn mask() -> SecretMask {
    Arc::new(|text: &str| text.replace(SECRET, "****"))
}

async fn send(request: PreparedRequest) -> Result<Response, HttpError> {
    let rules = RedirectRules {
        check: None,
        mask: Some(mask()),
    };
    HttpClient::new()
        .send_with(request, CancellationToken::new(), rules)
        .await
}

/// A server answering `/echo` with 200.
async fn target() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
        .mount(&server)
        .await;
    server
}

/// A server answering `/start` with `status` and `Location: location`.
async fn redirecting(status: u16, location: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/start"))
        .respond_with(ResponseTemplate::new(status).insert_header("Location", location))
        .mount(&server)
        .await;
    server
}

async fn requests(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap()
}

#[tokio::test]
async fn a_cross_origin_redirect_drops_secret_headers_and_keeps_plain_ones() {
    let b = target().await;
    let a = redirecting(302, &format!("{}/echo", b.uri())).await;
    let bearer = format!("Bearer {SECRET}");
    let sent = request(
        "GET",
        format!("{}/start", a.uri()),
        &[
            ("X-Api-Key", SECRET),
            ("X-Signed", &format!("v1:{SECRET}")),
            ("X-Request-Id", "plain-id"),
            ("Accept", "application/json"),
            ("Authorization", &bearer),
            ("Cookie", "session=1"),
        ],
    );
    let response = send(sent).await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.final_url, format!("{}/echo", b.uri()));

    let first = &requests(&a).await[0];
    assert_eq!(first.headers.get("x-api-key").unwrap(), SECRET);
    let arrived = requests(&b).await;
    assert_eq!(arrived.len(), 1);
    let headers = &arrived[0].headers;
    for dropped in ["x-api-key", "x-signed", "authorization", "cookie"] {
        assert!(!headers.contains_key(dropped), "{dropped} reached B");
    }
    assert_eq!(headers.get("x-request-id").unwrap(), "plain-id");
    assert_eq!(headers.get("accept").unwrap(), "application/json");
}

#[tokio::test]
async fn a_same_origin_redirect_keeps_every_header() {
    let server = target().await;
    Mock::given(path("/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/echo"))
        .mount(&server)
        .await;
    let bearer = format!("Bearer {SECRET}");
    let sent = request(
        "GET",
        format!("{}/start", server.uri()),
        &[("X-Api-Key", SECRET), ("Authorization", &bearer)],
    );
    assert_eq!(send(sent).await.unwrap().status, 200);
    let arrived = requests(&server).await;
    assert_eq!(arrived.len(), 2);
    assert_eq!(arrived[1].url.path(), "/echo");
    assert_eq!(arrived[1].headers.get("x-api-key").unwrap(), SECRET);
    assert_eq!(arrived[1].headers.get("authorization").unwrap(), &bearer);
}

#[tokio::test]
async fn a_cross_origin_307_with_a_secret_body_is_refused_and_sends_nothing() {
    let b = target().await;
    let a = redirecting(307, &format!("{}/echo?k={SECRET}", b.uri())).await;
    let mut sent = request("POST", format!("{}/start", a.uri()), &[]);
    sent.body = PreparedBody::Bytes {
        content_type: Some("application/json".to_owned()),
        data: format!("{{\"token\":\"{SECRET}\"}}").into_bytes(),
    };
    let error = send(sent).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "a redirect to {}/echo?k=**** was not followed: the request body holds a secret",
            b.uri()
        )
    );
    assert!(!error.to_string().contains(SECRET));
    assert!(requests(&b).await.is_empty());
}

#[tokio::test]
async fn a_cross_origin_307_without_a_secret_resends_the_body() {
    let b = target().await;
    let a = redirecting(307, &format!("{}/echo", b.uri())).await;
    let mut sent = request("POST", format!("{}/start", a.uri()), &[]);
    sent.body = PreparedBody::Bytes {
        content_type: None,
        data: b"plain".to_vec(),
    };
    assert_eq!(send(sent).await.unwrap().status, 200);
    let arrived = requests(&b).await;
    assert_eq!(arrived[0].method.as_str(), "POST");
    assert_eq!(arrived[0].body, b"plain");
}

#[tokio::test]
async fn a_302_after_a_post_follows_with_a_get_and_no_body() {
    let b = target().await;
    let a = redirecting(302, &format!("{}/echo", b.uri())).await;
    let mut sent = request("POST", format!("{}/start", a.uri()), &[]);
    sent.body = PreparedBody::Bytes {
        content_type: Some("text/plain".to_owned()),
        data: SECRET.as_bytes().to_vec(),
    };
    assert_eq!(send(sent).await.unwrap().status, 200);
    let arrived = requests(&b).await;
    assert_eq!(arrived[0].method.as_str(), "GET");
    assert!(arrived[0].body.is_empty());
    assert!(!arrived[0].headers.contains_key("content-type"));
}

// S2: the target query is the server's; the original URL's secret query is not copied.
#[tokio::test]
async fn a_cross_origin_redirect_follows_the_location_query_as_given() {
    let b = target().await;
    let a = redirecting(302, &format!("{}/echo?page=2", b.uri())).await;
    let sent = request("GET", format!("{}/start?key={SECRET}", a.uri()), &[]);
    let response = send(sent).await.unwrap();
    assert_eq!(response.final_url, format!("{}/echo?page=2", b.uri()));
    let arrived = requests(&b).await;
    assert_eq!(arrived[0].url.query(), Some("page=2"));
    assert!(
        arrived[0]
            .headers
            .get("referer")
            .is_none_or(|referer| !referer.to_str().unwrap().contains(SECRET))
    );
}

#[tokio::test]
async fn the_redirect_limit_still_errors_at_eleven() {
    let server = MockServer::start().await;
    Mock::given(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/loop"))
        .mount(&server)
        .await;
    let result = send(request("GET", format!("{}/loop", server.uri()), &[])).await;
    assert!(matches!(result, Err(HttpError::Request(_))), "{result:?}");
    assert_eq!(requests(&server).await.len(), 11);
}

// Existing behavior: without a mask a cross-origin redirect keeps custom headers.
#[tokio::test]
async fn without_a_mask_a_redirect_keeps_custom_headers_and_reports_the_final_url() {
    let b = target().await;
    let a = redirecting(301, &format!("{}/echo", b.uri())).await;
    let sent = request(
        "GET",
        format!("{}/start", a.uri()),
        &[("X-Api-Key", "k"), ("X-Request-Id", "r")],
    );
    let response = HttpClient::new()
        .send(sent, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"arrived");
    assert_eq!(response.final_url, format!("{}/echo", b.uri()));
    let headers = &requests(&b).await[0].headers;
    assert_eq!(headers.get("x-api-key").unwrap(), "k");
    assert_eq!(headers.get("x-request-id").unwrap(), "r");
}
