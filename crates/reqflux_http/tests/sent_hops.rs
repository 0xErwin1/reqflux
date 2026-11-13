//! The headers a hop records are the ones on the wire (sent-headers S5), checked against a raw
//! TCP capture of the request head.

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
    CookieJar, HttpClient, MultipartPart, MultipartValue, PreparedBody, PreparedRequest,
    RequestOptions, Response,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

/// One request head as the server read it: the request line and the header lines, in order.
#[derive(Debug, Clone)]
struct Captured {
    line: String,
    headers: Vec<(String, String)>,
}

/// A raw HTTP/1.1 server: `/start` answers 302 to `/next`, anything else 200.
struct Raw {
    base: String,
    captured: Arc<Mutex<Vec<Captured>>>,
}

async fn raw() -> Raw {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&captured);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                let mut buffer: Vec<u8> = Vec::new();
                loop {
                    let head_end = loop {
                        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                            break at;
                        }
                        let mut chunk = [0_u8; 4096];
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                        }
                    };
                    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
                    let mut lines = head.split("\r\n");
                    let line = lines.next().unwrap_or_default().to_owned();
                    let headers: Vec<(String, String)> = lines
                        .map(|text| {
                            let (name, value) = text.split_once(':').unwrap();
                            (name.to_ascii_lowercase(), value.trim().to_owned())
                        })
                        .collect();
                    let length = headers
                        .iter()
                        .find(|(name, _)| name == "content-length")
                        .map_or(0, |(_, value)| value.parse::<usize>().unwrap());
                    let chunked = headers
                        .iter()
                        .any(|(name, value)| name == "transfer-encoding" && value == "chunked");
                    let total = loop {
                        let body = &buffer[head_end + 4..];
                        if chunked {
                            if let Some(at) = body.windows(5).position(|w| w == b"0\r\n\r\n") {
                                break head_end + 4 + at + 5;
                            }
                        } else if body.len() >= length {
                            break head_end + 4 + length;
                        }
                        let mut chunk = [0_u8; 4096];
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                        }
                    };
                    buffer.drain(..total);
                    let target = line.split(' ').nth(1).unwrap_or_default().to_owned();
                    sink.lock().unwrap().push(Captured { line, headers });
                    let reply = if target == "/start" {
                        "HTTP/1.1 302 Found\r\nlocation: /next\r\ncontent-length: 0\r\n\r\n"
                    } else {
                        "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n"
                    };
                    if stream.write_all(reply.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    Raw { base, captured }
}

#[derive(Debug)]
struct FixedJar;

impl CookieJar for FixedJar {
    fn cookie_header(&self, _url: &Url) -> Option<String> {
        Some("jar=1; other=2".to_owned())
    }

    fn store(&self, _url: &Url, _set_cookies: &[String]) {}
}

fn request(
    method: &str,
    url: String,
    headers: &[(&str, &str)],
    body: PreparedBody,
) -> PreparedRequest {
    PreparedRequest {
        method: method.to_owned(),
        url,
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body,
        options: RequestOptions {
            timeout: Duration::from_secs(10),
            follow_redirects: true,
            verify_tls: true,
        },
    }
}

async fn send(client: &HttpClient, request: PreparedRequest) -> Response {
    client
        .send(request, CancellationToken::new())
        .await
        .unwrap()
}

/// Asserts every hop recorded the method, URL and headers the server captured.
fn assert_wire(response: &Response, captured: &Raw, expected: &[(&str, &str)]) {
    let captured = captured.captured.lock().unwrap();
    assert_eq!(response.timings.hops.len(), expected.len());
    assert_eq!(captured.len(), expected.len());
    for ((hop, wire), (method, url)) in response
        .timings
        .hops
        .iter()
        .zip(captured.iter())
        .zip(expected)
    {
        assert_eq!(hop.wire.method, *method);
        assert_eq!(hop.wire.url, *url);
        let target = wire.line.split(' ').nth(1).unwrap();
        assert_eq!(wire.line.split(' ').next().unwrap(), *method);
        assert!(url.ends_with(target), "{url} vs {target}");
        assert_eq!(hop.wire.headers, wire.headers, "{}", wire.line);
    }
}

fn names(response: &Response) -> Vec<&str> {
    response.timings.hops[0]
        .wire
        .headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect()
}

#[tokio::test]
async fn get_without_body_records_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/get?a=1", server.base);
    let response = send(
        &HttpClient::new(),
        request(
            "GET",
            url.clone(),
            &[("X-Custom", "one")],
            PreparedBody::None,
        ),
    )
    .await;
    assert_wire(&response, &server, &[("GET", &url)]);
    let names = names(&response);
    for expected in [
        "x-custom",
        "user-agent",
        "accept",
        "accept-encoding",
        "host",
    ] {
        assert!(names.contains(&expected), "{names:?} lacks {expected}");
    }
}

#[tokio::test]
async fn post_without_body_records_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/post", server.base);
    let response = send(
        &HttpClient::new(),
        request("POST", url.clone(), &[], PreparedBody::None),
    )
    .await;
    assert_wire(&response, &server, &[("POST", &url)]);
}

#[tokio::test]
async fn raw_json_bytes_record_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/json", server.base);
    let body = PreparedBody::Bytes {
        content_type: Some("application/json".to_owned()),
        data: br#"{"a":1}"#.to_vec(),
    };
    let response = send(&HttpClient::new(), request("POST", url.clone(), &[], body)).await;
    assert_wire(&response, &server, &[("POST", &url)]);
    let headers = &response.timings.hops[0].wire.headers;
    assert!(headers.contains(&("content-type".to_owned(), "application/json".to_owned())));
    assert!(headers.contains(&("content-length".to_owned(), "7".to_owned())));
}

#[tokio::test]
async fn bytes_with_own_content_type_record_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/own", server.base);
    let body = PreparedBody::Bytes {
        content_type: Some("text/plain".to_owned()),
        data: b"hello".to_vec(),
    };
    let headers = [("Content-Type", "application/x-custom")];
    let response = send(
        &HttpClient::new(),
        request("PUT", url.clone(), &headers, body),
    )
    .await;
    assert_wire(&response, &server, &[("PUT", &url)]);
}

#[tokio::test]
async fn urlencoded_form_records_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/form", server.base);
    let body = PreparedBody::Form(vec![
        ("a".to_owned(), "1".to_owned()),
        ("b".to_owned(), "x y".to_owned()),
    ]);
    let response = send(&HttpClient::new(), request("POST", url.clone(), &[], body)).await;
    assert_wire(&response, &server, &[("POST", &url)]);
}

#[tokio::test]
async fn multipart_text_and_file_record_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/multipart", server.base);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("upload.txt");
    std::fs::write(&path, b"file contents").unwrap();
    let body = PreparedBody::Multipart(vec![
        MultipartPart {
            name: "text".to_owned(),
            value: MultipartValue::Text("value".to_owned()),
        },
        MultipartPart {
            name: "file".to_owned(),
            value: MultipartValue::File(path),
        },
    ]);
    let response = send(&HttpClient::new(), request("POST", url.clone(), &[], body)).await;
    assert_wire(&response, &server, &[("POST", &url)]);
    let content_type = response.timings.hops[0]
        .wire
        .headers
        .iter()
        .find(|(name, _)| name == "content-type")
        .unwrap();
    assert!(content_type.1.starts_with("multipart/form-data; boundary="));
}

#[tokio::test]
async fn file_body_records_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/file", server.base);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"0123456789").unwrap();
    let response = send(
        &HttpClient::new(),
        request("POST", url.clone(), &[], PreparedBody::File(path)),
    )
    .await;
    assert_wire(&response, &server, &[("POST", &url)]);
    let headers = &response.timings.hops[0].wire.headers;
    assert!(headers.contains(&("content-length".to_owned(), "10".to_owned())));
}

#[tokio::test]
async fn jar_cookie_is_recorded() {
    let server = raw().await;
    let url = format!("{}/cookie", server.base);
    let client = HttpClient::new();
    client.set_cookie_jar(Some(Arc::new(FixedJar)));
    let response = send(
        &client,
        request(
            "GET",
            url.clone(),
            &[("Cookie", "own=0")],
            PreparedBody::None,
        ),
    )
    .await;
    assert_wire(&response, &server, &[("GET", &url)]);
    let headers = &response.timings.hops[0].wire.headers;
    assert!(headers.contains(&("cookie".to_owned(), "own=0; jar=1; other=2".to_owned())));
}

#[tokio::test]
async fn redirect_records_both_hops() {
    let server = raw().await;
    let start = format!("{}/start", server.base);
    let next = format!("{}/next", server.base);
    let body = PreparedBody::Bytes {
        content_type: Some("application/json".to_owned()),
        data: b"{}".to_vec(),
    };
    let response = send(
        &HttpClient::new(),
        request("POST", start.clone(), &[], body),
    )
    .await;
    assert_wire(&response, &server, &[("POST", &start), ("GET", &next)]);
    let second = &response.timings.hops[1].wire.headers;
    assert!(second.contains(&("referer".to_owned(), start)));
    assert!(!second.iter().any(|(name, _)| name == "content-length"));
}

#[tokio::test]
async fn empty_bytes_body_records_the_wire_headers() {
    let server = raw().await;
    let url = format!("{}/empty", server.base);
    let body = PreparedBody::Bytes {
        content_type: None,
        data: Vec::new(),
    };
    let response = send(&HttpClient::new(), request("POST", url.clone(), &[], body)).await;
    assert_wire(&response, &server, &[("POST", &url)]);
}

#[tokio::test]
async fn a_body_of_unknown_size_records_chunked() {
    let server = raw().await;
    let url = format!("{}/chunked", server.base);
    let path = std::path::PathBuf::from("/dev/null");
    let response = send(
        &HttpClient::new(),
        request("POST", url.clone(), &[], PreparedBody::File(path)),
    )
    .await;
    assert_wire(&response, &server, &[("POST", &url)]);
    let headers = &response.timings.hops[0].wire.headers;
    assert!(headers.contains(&("transfer-encoding".to_owned(), "chunked".to_owned())));
}
