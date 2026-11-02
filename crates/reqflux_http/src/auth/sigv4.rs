//! AWS Signature Version 4 in the `Authorization` header.
//!
//! Every header of the request is signed, with `host` and `x-amz-date` added, and the
//! payload hash covers the body as sent. A file or multipart body is signed as
//! `UNSIGNED-PAYLOAD`, since its bytes are only known while streaming.

use std::time::SystemTime;

use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::{Digest as _, Sha256};
use time::OffsetDateTime;
use url::Url;

use super::hex;
use crate::error::HttpError;
use crate::request::{PreparedBody, PreparedRequest};

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// The credentials and scope a request is signed with.
#[derive(Clone, Copy)]
pub struct AwsCredentials<'a> {
    /// The access key id.
    pub access_key: &'a str,
    /// The secret access key.
    pub secret_key: &'a str,
    /// The session token of temporary credentials, sent as `X-Amz-Security-Token`.
    pub session_token: Option<&'a str>,
    /// The region.
    pub region: &'a str,
    /// The service.
    pub service: &'a str,
}

impl std::fmt::Debug for AwsCredentials<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AwsCredentials")
            .field("region", &self.region)
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

/// Signs `request` at `now`: adds `X-Amz-Date`, `X-Amz-Security-Token` with a session
/// token, `X-Amz-Content-Sha256` for S3, and `Authorization`. A request that already has an
/// `Authorization` header is left as it is.
///
/// # Errors
///
/// The URL does not parse or has no host.
pub fn sign(
    request: &mut PreparedRequest,
    credentials: &AwsCredentials<'_>,
    now: SystemTime,
) -> Result<(), HttpError> {
    if has(&request.headers, "authorization") {
        return Ok(());
    }
    let url = Url::parse(&request.url).map_err(|error| HttpError::InvalidUrl {
        url: request.url.clone(),
        reason: error.to_string(),
    })?;
    let host = match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        (None, _) => {
            return Err(HttpError::InvalidUrl {
                url: request.url.clone(),
                reason: "the URL has no host to sign".to_owned(),
            });
        }
    };
    let time = OffsetDateTime::from(now);
    let date = format!(
        "{:04}{:02}{:02}",
        time.year(),
        u8::from(time.month()),
        time.day()
    );
    let stamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        time.hour(),
        time.minute(),
        time.second()
    );
    let payload = payload_hash(&request.body);
    let s3 = credentials.service == "s3";

    let mut headers = request.headers.clone();
    if !has(&headers, "host") {
        headers.push(("Host".to_owned(), host));
    }
    headers.push(("X-Amz-Date".to_owned(), stamp.clone()));
    if let Some(token) = credentials.session_token.filter(|token| !token.is_empty()) {
        headers.push(("X-Amz-Security-Token".to_owned(), token.to_owned()));
    }
    if s3 {
        headers.push(("X-Amz-Content-Sha256".to_owned(), payload.clone()));
    }

    let (canonical_headers, signed_headers) = canonical_headers(&headers);
    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{payload}",
        request.method,
        canonical_uri(&url, s3),
        canonical_query(&url),
    );
    let scope = format!(
        "{date}/{}/{}/aws4_request",
        credentials.region, credentials.service
    );
    let string_to_sign = format!(
        "{ALGORITHM}\n{stamp}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let key = [credentials.region, credentials.service, "aws4_request"]
        .iter()
        .fold(
            hmac(format!("AWS4{}", credentials.secret_key).as_bytes(), &date),
            |key, part| hmac(&key, part),
        );
    let signature = hex(&hmac(&key, &string_to_sign));

    // The Host header stays implicit: the transport sends the URL's.
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case("host") || has(&request.headers, "host"));
    headers.push((
        "Authorization".to_owned(),
        format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            credentials.access_key
        ),
    ));
    request.headers = headers;
    Ok(())
}

fn has(headers: &[(String, String)], name: &str) -> bool {
    headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case(name))
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    match Hmac::<Sha256>::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(data.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }
        // HMAC takes a key of any length.
        Err(_) => Vec::new(),
    }
}

fn payload_hash(body: &PreparedBody) -> String {
    let bytes = match body {
        PreparedBody::None => Vec::new(),
        PreparedBody::Bytes { data, .. } => data.clone(),
        PreparedBody::Form(fields) => url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish()
            .into_bytes(),
        PreparedBody::Multipart(_) | PreparedBody::File(_) => {
            return UNSIGNED_PAYLOAD.to_owned();
        }
    };
    hex(&Sha256::digest(&bytes))
}

/// Lowercase names sorted, values trimmed with inner runs of spaces collapsed, repeated
/// names joined with commas; and the signed header list.
fn canonical_headers(headers: &[(String, String)]) -> (String, String) {
    let mut merged: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (name, value) in headers {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        merged
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(value);
    }
    let canonical = merged
        .iter()
        .map(|(name, values)| format!("{name}:{}\n", values.join(",")))
        .collect();
    let signed = merged.keys().cloned().collect::<Vec<_>>().join(";");
    (canonical, signed)
}

/// The path as sent, encoded once more for every service but S3.
fn canonical_uri(url: &Url, s3: bool) -> String {
    let path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };
    if s3 {
        path.to_owned()
    } else {
        encode(path, false)
    }
}

/// The query pairs, each encoded and sorted by name then value.
fn canonical_query(url: &Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (encode(&key, true), encode(&value, true)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// AWS URI encoding: everything but the unreserved characters, and `/` too when `slash`.
fn encode(text: &str, slash: bool) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            b'/' if !slash => encoded.push('/'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::request::RequestOptions;

    /// 2015-08-30T12:36:00Z, the time of the AWS SigV4 test suite.
    fn suite_time() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_440_938_160)
    }

    const CREDENTIALS: AwsCredentials<'static> = AwsCredentials {
        access_key: "AKIDEXAMPLE",
        secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        session_token: None,
        region: "us-east-1",
        service: "service",
    };

    fn request(
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: PreparedBody,
    ) -> PreparedRequest {
        PreparedRequest {
            method: method.to_owned(),
            url: url.to_owned(),
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body,
            options: RequestOptions {
                timeout: Duration::from_secs(1),
                follow_redirects: true,
                verify_tls: true,
            },
        }
    }

    fn authorization(
        mut request: PreparedRequest,
        credentials: &AwsCredentials<'_>,
    ) -> PreparedRequest {
        if let Err(error) = sign(&mut request, credentials, suite_time()) {
            panic!("{error}");
        }
        request
    }

    fn header<'a>(request: &'a PreparedRequest, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn signed(signed_headers: &str, signature: &str) -> String {
        format!(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders={signed_headers}, Signature={signature}"
        )
    }

    /// `get-vanilla`, `post-vanilla`, `get-vanilla-query-order-key-case` and
    /// `post-x-www-form-urlencoded` from the AWS SigV4 test suite.
    #[test]
    fn signs_the_aws_test_suite_vectors() {
        let cases = [
            (
                request(
                    "GET",
                    "https://example.amazonaws.com/",
                    &[],
                    PreparedBody::None,
                ),
                signed(
                    "host;x-amz-date",
                    "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
                ),
            ),
            (
                request(
                    "POST",
                    "https://example.amazonaws.com/",
                    &[],
                    PreparedBody::None,
                ),
                signed(
                    "host;x-amz-date",
                    "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b",
                ),
            ),
            (
                request(
                    "GET",
                    "https://example.amazonaws.com/?Param2=value2&Param1=value1",
                    &[],
                    PreparedBody::None,
                ),
                signed(
                    "host;x-amz-date",
                    "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500",
                ),
            ),
            (
                request(
                    "POST",
                    "https://example.amazonaws.com/",
                    &[("Content-Type", "application/x-www-form-urlencoded")],
                    PreparedBody::Bytes {
                        content_type: None,
                        data: b"Param1=value1".to_vec(),
                    },
                ),
                signed(
                    "content-type;host;x-amz-date",
                    "ff11897932ad3f4e8b18135d722051e5ac45fc38421b1da7b9d196a0fe09473a",
                ),
            ),
        ];
        for (request, expected) in cases {
            let url = request.url.clone();
            let signed = authorization(request, &CREDENTIALS);
            assert_eq!(
                header(&signed, "Authorization"),
                Some(expected.as_str()),
                "{url}"
            );
            assert_eq!(header(&signed, "X-Amz-Date"), Some("20150830T123600Z"));
            assert_eq!(
                header(&signed, "Host"),
                None,
                "the transport sends the host"
            );
        }
    }

    #[test]
    fn signs_the_session_token_and_the_s3_payload_hash() {
        let credentials = AwsCredentials {
            session_token: Some("session-token"),
            service: "s3",
            ..CREDENTIALS
        };
        let signed = authorization(
            request(
                "PUT",
                "https://bucket.s3.amazonaws.com/key",
                &[],
                PreparedBody::Bytes {
                    content_type: None,
                    data: b"hello".to_vec(),
                },
            ),
            &credentials,
        );
        assert_eq!(
            header(&signed, "X-Amz-Security-Token"),
            Some("session-token")
        );
        assert_eq!(
            header(&signed, "X-Amz-Content-Sha256"),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
        let authorization = header(&signed, "Authorization").unwrap_or_default();
        assert!(
            authorization.contains(
                "SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token,"
            ),
            "{authorization}"
        );
        assert!(!authorization.contains("wJalr"), "{authorization}");
    }

    #[test]
    fn a_request_with_an_authorization_header_is_not_signed() {
        let signed = authorization(
            request(
                "GET",
                "https://example.amazonaws.com/",
                &[("Authorization", "Bearer mine")],
                PreparedBody::None,
            ),
            &CREDENTIALS,
        );
        assert_eq!(signed.headers.len(), 1);
        assert_eq!(
            payload_hash(&PreparedBody::File("f".into())),
            UNSIGNED_PAYLOAD
        );
    }
}
