//! Transport behavior against a local mock server (S1-S3).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use reqflux_http::{
    HttpClient, HttpError, MultipartPart, MultipartValue, PreparedBody, PreparedRequest,
    RequestOptions, Response,
};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn options() -> RequestOptions {
    RequestOptions {
        timeout: Duration::from_secs(10),
        follow_redirects: true,
        verify_tls: true,
    }
}

fn request(verb: &str, url: String) -> PreparedRequest {
    PreparedRequest {
        method: verb.to_owned(),
        url,
        headers: Vec::new(),
        body: PreparedBody::None,
        options: options(),
    }
}

async fn send(request: PreparedRequest) -> Result<Response, HttpError> {
    HttpClient::new()
        .send(request, CancellationToken::new())
        .await
}

async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    server
}

async fn only_request(server: &MockServer) -> wiremock::Request {
    let mut requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    requests.remove(0)
}

fn header_values(request: &wiremock::Request, name: &str) -> Vec<String> {
    request
        .headers
        .get_all(name)
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect()
}

// S1: method.

#[tokio::test]
async fn sends_standard_and_custom_methods() {
    for verb in ["PATCH", "PURGE"] {
        let server = MockServer::start().await;
        Mock::given(method(verb))
            .and(path("/echo"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let response = send(request(verb, format!("{}/echo", server.uri())))
            .await
            .unwrap();
        assert_eq!(response.status, 204);
        assert_eq!(only_request(&server).await.method.as_str(), verb);
    }
}

// S1: ordered headers with duplicates.

#[tokio::test]
async fn sends_ordered_and_duplicate_headers() {
    let server = ok_server().await;
    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![
        ("X-First".to_owned(), "1".to_owned()),
        ("X-Dup".to_owned(), "a".to_owned()),
        ("X-Dup".to_owned(), "b".to_owned()),
    ];
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    assert_eq!(header_values(&received, "x-first"), vec!["1"]);
    assert_eq!(header_values(&received, "x-dup"), vec!["a", "b"]);
    let names: Vec<&str> = received
        .headers
        .keys()
        .map(|name| name.as_str())
        .filter(|name| name.starts_with("x-"))
        .collect();
    assert_eq!(names, vec!["x-first", "x-dup"]);
}

#[tokio::test]
async fn invalid_header_name_and_value_fail_with_typed_error() {
    let server = ok_server().await;
    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![("bad name".to_owned(), "v".to_owned())];
    let error = send(prepared).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid header bad name: invalid HTTP header name"
    );

    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    // The transport itself now refuses the line break before reqwest sees it, so the message
    // names the rule instead of the parser failure it used to surface.
    prepared.headers = vec![("X-Ok".to_owned(), "line\nbreak".to_owned())];
    let error = send(prepared).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid header X-Ok: the header value contains CR or LF"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn crlf_injection_in_a_header_value_is_refused() {
    let server = ok_server().await;
    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![("X-Tenant".to_owned(), "a\r\nX-Evil: 1".to_owned())];
    let error = send(prepared).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid header X-Tenant: the header value contains CR or LF"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn non_ascii_header_name_and_value_are_refused() {
    let server = ok_server().await;
    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![("X-Tenant".to_owned(), "caf\u{e9}".to_owned())];
    let error = send(prepared).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid header X-Tenant: the header value has a non-ASCII character"
    );

    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![("X-Ten\u{e1}nt".to_owned(), "v".to_owned())];
    let error = send(prepared).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid header X-Tenánt: the header name has a non-ASCII character"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn tab_in_a_header_value_is_sent() {
    let server = ok_server().await;
    let mut prepared = request("GET", format!("{}/echo", server.uri()));
    prepared.headers = vec![("X-Tenant".to_owned(), "a\tb".to_owned())];
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    assert_eq!(header_values(&received, "x-tenant"), vec!["a\tb"]);
}

// S1: bodies.

#[tokio::test]
async fn sends_bytes_with_content_type() {
    let server = ok_server().await;
    let mut prepared = request("POST", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::Bytes {
        content_type: Some("application/json".to_owned()),
        data: b"{\"a\":1}".to_vec(),
    };
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    assert_eq!(received.body, b"{\"a\":1}");
    assert_eq!(
        header_values(&received, "content-type"),
        vec!["application/json"]
    );
}

#[tokio::test]
async fn explicit_content_type_header_wins_over_bytes_content_type() {
    let server = ok_server().await;
    let mut prepared = request("POST", format!("{}/echo", server.uri()));
    prepared.headers = vec![("Content-Type".to_owned(), "text/csv".to_owned())];
    prepared.body = PreparedBody::Bytes {
        content_type: Some("application/json".to_owned()),
        data: b"a,b".to_vec(),
    };
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    assert_eq!(header_values(&received, "content-type"), vec!["text/csv"]);
}

#[tokio::test]
async fn sends_urlencoded_form() {
    let server = ok_server().await;
    let mut prepared = request("POST", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::Form(vec![
        ("name".to_owned(), "a b".to_owned()),
        ("x".to_owned(), "1&2".to_owned()),
    ]);
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    assert_eq!(
        String::from_utf8(received.body.clone()).unwrap(),
        "name=a+b&x=1%262"
    );
    assert_eq!(
        header_values(&received, "content-type"),
        vec!["application/x-www-form-urlencoded"]
    );
}

#[tokio::test]
async fn sends_multipart_text_and_file_parts() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "file contents").unwrap();
    let server = ok_server().await;
    let mut prepared = request("POST", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::Multipart(vec![
        MultipartPart {
            name: "field".to_owned(),
            value: MultipartValue::Text("hello".to_owned()),
        },
        MultipartPart {
            name: "upload".to_owned(),
            value: MultipartValue::File(file),
        },
    ]);
    send(prepared).await.unwrap();
    let received = only_request(&server).await;
    let content_type = header_values(&received, "content-type").remove(0);
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    let body = String::from_utf8(received.body.clone()).unwrap();
    assert!(body.contains("Content-Disposition: form-data; name=\"field\"\r\n\r\nhello\r\n"));
    assert!(body.contains(
        "Content-Disposition: form-data; name=\"upload\"; filename=\"notes.txt\"\r\nContent-Type: text/plain\r\n\r\nfile contents\r\n"
    ));
}

#[tokio::test]
async fn streams_a_file_body() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("payload.bin");
    let data: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
    std::fs::write(&file, &data).unwrap();
    let server = ok_server().await;
    let mut prepared = request("PUT", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::File(file);
    send(prepared).await.unwrap();
    assert_eq!(only_request(&server).await.body, data);
}

// S1: response.

#[tokio::test]
async fn returns_status_reason_version_headers_and_body() {
    let server = MockServer::start().await;
    Mock::given(path("/teapot"))
        .respond_with(
            ResponseTemplate::new(418)
                .append_header("X-Multi", "one")
                .append_header("X-Other", "z")
                .append_header("X-Multi", "two")
                .set_body_string("short and stout"),
        )
        .mount(&server)
        .await;
    let url = format!("{}/teapot", server.uri());
    let response = send(request("GET", url.clone())).await.unwrap();
    assert_eq!(response.status, 418);
    assert_eq!(response.reason.as_deref(), Some("I'm a teapot"));
    assert_eq!(response.version, "1.1");
    assert_eq!(response.body, b"short and stout");
    assert_eq!(response.size(), 15);
    assert_eq!(response.final_url, url);
    assert!(response.elapsed > Duration::ZERO);
    let custom: Vec<(&str, &str)> = response
        .headers
        .iter()
        .filter(|(name, _)| name.starts_with("x-"))
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    assert_eq!(
        custom,
        vec![("x-multi", "one"), ("x-multi", "two"), ("x-other", "z")]
    );
}

/// A gzip stream holding `data` in one stored (uncompressed) deflate block.
fn gzip_stored(data: &[u8]) -> Vec<u8> {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    let crc = !crc;
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff, 0x01];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&u32::from(len).to_le_bytes());
    out
}

#[tokio::test]
async fn decompresses_gzip_bodies() {
    let server = MockServer::start().await;
    Mock::given(path("/gz"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Encoding", "gzip")
                .set_body_bytes(gzip_stored(b"decompressed text")),
        )
        .mount(&server)
        .await;
    let response = send(request("GET", format!("{}/gz", server.uri())))
        .await
        .unwrap();
    assert_eq!(response.body, b"decompressed text");
    assert_eq!(response.size(), 17);
    // The request that carries the header is what this test is about; a retry after a closed
    // pooled connection can leave more than one, so the last one answers.
    let mut requests = server.received_requests().await.unwrap();
    let last = requests.pop().expect("the mock received the request");
    let accept = last
        .headers
        .get("accept-encoding")
        .unwrap()
        .to_str()
        .unwrap();
    for coding in ["gzip", "br", "deflate", "zstd"] {
        assert!(accept.contains(coding), "{accept}");
    }
}

// S1, S3: redirects.

async fn redirect_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/from"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/to"))
        .mount(&server)
        .await;
    Mock::given(path("/to"))
        .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
        .mount(&server)
        .await;
    Mock::given(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/loop"))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn follows_redirects_when_enabled() {
    let server = redirect_server().await;
    let response = send(request("GET", format!("{}/from", server.uri())))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"arrived");
    assert_eq!(response.final_url, format!("{}/to", server.uri()));
}

#[tokio::test]
async fn does_not_follow_redirects_when_disabled() {
    let server = redirect_server().await;
    let url = format!("{}/from", server.uri());
    let mut prepared = request("GET", url.clone());
    prepared.options.follow_redirects = false;
    let response = send(prepared).await.unwrap();
    assert_eq!(response.status, 302);
    assert_eq!(response.final_url, url);
}

#[tokio::test]
async fn stops_after_ten_redirects() {
    let server = redirect_server().await;
    let response = send(request("GET", format!("{}/loop", server.uri()))).await;
    assert!(response.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 11);
}

#[tokio::test]
async fn one_client_serves_both_redirect_settings() {
    let server = redirect_server().await;
    let client = HttpClient::new();
    let url = format!("{}/from", server.uri());
    let mut no_follow = request("GET", url.clone());
    no_follow.options.follow_redirects = false;
    for _ in 0..2 {
        let followed = client
            .send(request("GET", url.clone()), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(followed.status, 200);
        let stopped = client
            .clone()
            .send(no_follow.clone(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(stopped.status, 302);
    }
}

/// A redirect check refusing every target whose path is `refused`.
fn refusing(refused: &'static str) -> reqflux_http::RedirectCheck {
    std::sync::Arc::new(move |url: &url::Url| {
        if url.path() == refused {
            Err("not that one".to_owned())
        } else {
            Ok(())
        }
    })
}

#[tokio::test]
async fn a_refused_redirect_stops_the_chain_and_sends_nothing_to_its_target() {
    let server = redirect_server().await;
    let error = HttpClient::new()
        .send_checked(
            request("GET", format!("{}/from", server.uri())),
            CancellationToken::new(),
            Some(refusing("/to")),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!(
            "a redirect to {}/to was refused: not that one",
            server.uri()
        )
    );
    let paths: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect();
    assert_eq!(paths, vec!["/from".to_owned()]);
}

#[tokio::test]
async fn a_redirect_check_keeps_the_ten_redirect_limit() {
    let server = redirect_server().await;
    let result = HttpClient::new()
        .send_checked(
            request("GET", format!("{}/loop", server.uri())),
            CancellationToken::new(),
            Some(refusing("/never")),
        )
        .await;
    assert!(matches!(result, Err(HttpError::Request(_))), "{result:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 11);
}

#[tokio::test]
async fn a_redirect_check_is_not_consulted_when_redirects_are_off() {
    let server = redirect_server().await;
    let mut no_follow = request("GET", format!("{}/from", server.uri()));
    no_follow.options.follow_redirects = false;
    let response = HttpClient::new()
        .send_checked(no_follow, CancellationToken::new(), Some(refusing("/to")))
        .await
        .unwrap();
    assert_eq!(response.status, 302);
}

// reqwest drops Authorization and Cookie when a redirect changes scheme, host or port; a
// custom header such as X-Api-Key is carried along.
#[tokio::test]
async fn a_cross_origin_redirect_drops_authorization_but_not_custom_headers() {
    let target = ok_server().await;
    let start = MockServer::start().await;
    Mock::given(path("/start"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/echo", target.uri())),
        )
        .mount(&start)
        .await;
    let mut sent = request("GET", format!("{}/start", start.uri()));
    sent.headers = vec![
        ("Authorization".to_owned(), "Bearer t".to_owned()),
        ("X-Api-Key".to_owned(), "k".to_owned()),
    ];
    assert_eq!(send(sent).await.unwrap().status, 200);
    let arrived = only_request(&target).await;
    assert!(!arrived.headers.contains_key("authorization"));
    assert!(arrived.headers.contains_key("x-api-key"));
}

// S2: errors.

#[tokio::test]
async fn invalid_url_error() {
    let error = send(request("GET", "not a url".to_owned()))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid URL not a url: relative URL without a base"
    );
}

#[tokio::test]
async fn timeout_error() {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let mut prepared = request("GET", format!("{}/slow", server.uri()));
    prepared.options.timeout = Duration::from_millis(150);
    let started = Instant::now();
    let error = send(prepared).await.unwrap_err();
    assert_eq!(error.to_string(), "request timed out after 150 ms");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn connect_error() {
    // Port 1 needs privileges to bind, so nothing else in the test process or the machine can
    // take it: binding a free port and dropping it races with every other test's mock server,
    // which is how this test once got a 404 from someone else's listener.
    let error = send(request("GET", "http://127.0.0.1:1/".to_owned()))
        .await
        .unwrap_err();
    let message = error.to_string();
    let reason = message
        .strip_prefix("could not connect to 127.0.0.1: ")
        .unwrap_or_else(|| panic!("unexpected message: {message}"));
    assert!(!reason.is_empty());
}

#[tokio::test]
async fn missing_body_file_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing: PathBuf = dir.path().join("missing.bin");
    let io_error = std::fs::metadata(&missing).unwrap_err();
    let expected = format!("{}: {io_error}", missing.display());
    let server = ok_server().await;

    let mut prepared = request("PUT", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::File(missing.clone());
    assert_eq!(send(prepared).await.unwrap_err().to_string(), expected);

    let mut prepared = request("POST", format!("{}/echo", server.uri()));
    prepared.body = PreparedBody::Multipart(vec![MultipartPart {
        name: "f".to_owned(),
        value: MultipartValue::File(missing),
    }]);
    assert_eq!(send(prepared).await.unwrap_err().to_string(), expected);
    assert!(server.received_requests().await.unwrap().is_empty());
}

// S2, S3: cancellation.

async fn slow_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    Mock::given(path("/medium"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(400))
                .set_body_string("survived"),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn cancelling_an_in_flight_request() {
    let server = slow_server().await;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let error = HttpClient::new()
        .send(request("GET", format!("{}/slow", server.uri())), cancel)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "request cancelled");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn cancelling_one_request_leaves_others_running() {
    let server = slow_server().await;
    let client = HttpClient::new();
    let cancel = CancellationToken::new();
    let cancelled = client.send(
        request("GET", format!("{}/slow", server.uri())),
        cancel.clone(),
    );
    let survivor = client.send(
        request("GET", format!("{}/medium", server.uri())),
        CancellationToken::new(),
    );
    let trigger = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
    };
    let (cancelled, survivor, ()) = tokio::join!(cancelled, survivor, trigger);
    assert_eq!(cancelled.unwrap_err().to_string(), "request cancelled");
    assert_eq!(survivor.unwrap().body, b"survived");
}
