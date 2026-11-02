//! The timing breakdown of a send (v1-features S4) against a local mock server.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::time::Duration;

use pretty_assertions::assert_eq;
use reqflux_http::{HttpClient, PreparedBody, PreparedRequest, RequestOptions, Response};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(url: String) -> PreparedRequest {
    PreparedRequest {
        method: "GET".to_owned(),
        url,
        headers: Vec::new(),
        body: PreparedBody::None,
        options: RequestOptions {
            timeout: Duration::from_secs(10),
            follow_redirects: true,
            verify_tls: true,
        },
    }
}

async fn send(client: &HttpClient, url: String) -> Response {
    client
        .send(request(url), CancellationToken::new())
        .await
        .unwrap()
}

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("x".repeat(64 * 1024))
                .set_delay(Duration::from_millis(60)),
        )
        .mount(&server)
        .await;
    Mock::given(path("/hop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/slow"))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_send_to_a_host_name_measures_dns_connect_first_byte_and_download() {
    let server = server().await;
    let url = format!("http://localhost:{}/slow", server.address().port());

    let response = send(&HttpClient::new(), url).await;

    assert_eq!(response.timings.hops.len(), 1);
    let hop = response.timings.last().unwrap();
    assert!(hop.dns.is_some(), "{hop:?}");
    assert!(hop.connect.is_some(), "{hop:?}");
    assert_eq!(hop.tls, None, "the transport reports TLS inside connect");
    assert!(hop.first_byte >= Duration::from_millis(60), "{hop:?}");
    let phases = hop.dns.unwrap() + hop.connect.unwrap() + hop.first_byte + hop.download;
    assert!(
        phases <= response.elapsed,
        "{phases:?} > {:?}",
        response.elapsed
    );
}

#[tokio::test]
async fn an_ip_address_has_no_dns_phase() {
    let server = server().await;

    let response = send(&HttpClient::new(), format!("{}/slow", server.uri())).await;

    let hop = response.timings.last().unwrap();
    assert_eq!(hop.dns, None);
    assert!(hop.connect.is_some(), "{hop:?}");
}

#[tokio::test]
async fn every_redirect_hop_has_its_own_entry() {
    let server = server().await;

    let response = send(&HttpClient::new(), format!("{}/hop", server.uri())).await;

    assert_eq!(response.status, 200);
    assert_eq!(response.timings.hops.len(), 2);
    assert_eq!(response.timings.hops[0].download, Duration::ZERO);
    assert!(response.timings.hops[1].first_byte >= Duration::from_millis(60));
}

#[tokio::test]
async fn concurrent_sends_on_one_client_keep_their_own_numbers() {
    let server = server().await;
    let client = HttpClient::new();
    let host = format!("http://localhost:{}/slow", server.address().port());
    let ip = format!("{}/slow", server.uri());

    let (by_name, by_ip) = tokio::join!(send(&client, host), send(&client, ip));

    assert!(by_name.timings.last().unwrap().dns.is_some());
    assert_eq!(by_ip.timings.last().unwrap().dns, None);
}
