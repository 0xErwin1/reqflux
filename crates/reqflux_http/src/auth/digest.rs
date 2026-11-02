//! HTTP digest access authentication (RFC 7616): reading a `WWW-Authenticate: Digest`
//! challenge and answering it with an `Authorization` header.

use md5::Md5;
use sha2::{Digest as _, Sha256};

use super::hex;

/// The hash a digest challenge uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestHash {
    /// MD5.
    Md5,
    /// SHA-256.
    Sha256,
}

impl DigestHash {
    fn hash(self, text: &str) -> String {
        match self {
            Self::Md5 => hex(&Md5::digest(text.as_bytes())),
            Self::Sha256 => hex(&Sha256::digest(text.as_bytes())),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha256 => "SHA-256",
        }
    }
}

/// A parsed `Digest` challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    /// The protection space.
    pub realm: String,
    /// The server nonce.
    pub nonce: String,
    /// Returned unchanged when present.
    pub opaque: Option<String>,
    /// The `algorithm` parameter as sent, `None` when absent.
    pub algorithm: Option<String>,
    /// Whether the server offers `qop=auth`; `false` means the RFC 2069 form.
    pub qop_auth: bool,
    /// Whether the server offers only `qop=auth-int`, which is not supported.
    pub auth_int_only: bool,
}

/// Why a challenge cannot be answered. The text never holds a credential.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// The algorithm is not MD5, MD5-sess, SHA-256 or SHA-256-sess.
    #[error("digest algorithm {0} is not supported")]
    Algorithm(String),
    /// Only `qop=auth-int` is offered.
    #[error("digest qop auth-int is not supported")]
    AuthInt,
}

/// What answers a challenge.
#[derive(Debug, Clone, Copy)]
pub struct DigestInput<'a> {
    /// The request method.
    pub method: &'a str,
    /// The request target: path and query.
    pub uri: &'a str,
    /// The user name.
    pub username: &'a str,
    /// The password.
    pub password: &'a str,
    /// The hash used when the challenge names no algorithm.
    pub fallback: DigestHash,
    /// The client nonce.
    pub cnonce: &'a str,
    /// The nonce count, sent as eight hex digits.
    pub nc: u32,
}

/// The `Digest` challenge among `headers` (`WWW-Authenticate`, any case), the SHA-256 one
/// first when the server offers several.
#[must_use]
pub fn challenge(headers: &[(String, String)]) -> Option<DigestChallenge> {
    let mut found: Vec<DigestChallenge> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("www-authenticate"))
        .filter_map(|(_, value)| parse(value))
        .collect();
    found.sort_by_key(|challenge| {
        !challenge
            .algorithm
            .as_deref()
            .is_some_and(|algorithm| algorithm.to_ascii_uppercase().starts_with("SHA-256"))
    });
    found.into_iter().next()
}

fn parse(value: &str) -> Option<DigestChallenge> {
    let trimmed = value.trim_start();
    let scheme = trimmed.get(..6)?;
    if !scheme.eq_ignore_ascii_case("digest") {
        return None;
    }
    let params = params(trimmed.get(6..)?);
    let get = |key: &str| {
        params
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.clone())
    };
    let qop = get("qop");
    let offered: Vec<String> = qop
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect();
    let qop_auth = offered.iter().any(|item| item == "auth");
    Some(DigestChallenge {
        realm: get("realm").unwrap_or_default(),
        nonce: get("nonce")?,
        opaque: get("opaque"),
        algorithm: get("algorithm"),
        qop_auth,
        auth_int_only: !offered.is_empty() && !qop_auth,
    })
}

/// `key=value` and `key="quoted, value"` pairs separated by commas.
fn params(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ',' || c.is_whitespace()).is_some() {}
        let key: String =
            std::iter::from_fn(|| chars.next_if(|c| *c != '=' && *c != ',')).collect();
        if key.trim().is_empty() {
            return pairs;
        }
        if chars.next_if_eq(&'=').is_none() {
            continue;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let value = if chars.next_if_eq(&'"').is_some() {
            let mut value = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => value.extend(chars.next()),
                    '"' => break,
                    other => value.push(other),
                }
            }
            value
        } else {
            std::iter::from_fn(|| chars.next_if(|c| *c != ','))
                .collect::<String>()
                .trim()
                .to_owned()
        };
        pairs.push((key.trim().to_owned(), value));
    }
}

/// The `Authorization` header value answering `challenge`.
///
/// # Errors
///
/// The challenge's algorithm is unknown or it offers only `auth-int`.
pub fn authorization(
    challenge: &DigestChallenge,
    input: &DigestInput<'_>,
) -> Result<String, DigestError> {
    if challenge.auth_int_only {
        return Err(DigestError::AuthInt);
    }
    let (hash, session) = match challenge.algorithm.as_deref() {
        None => (input.fallback, false),
        Some(name) => match name.to_ascii_uppercase().as_str() {
            "MD5" => (DigestHash::Md5, false),
            "MD5-SESS" => (DigestHash::Md5, true),
            "SHA-256" => (DigestHash::Sha256, false),
            "SHA-256-SESS" => (DigestHash::Sha256, true),
            _ => return Err(DigestError::Algorithm(name.to_owned())),
        },
    };
    let nc = format!("{:08x}", input.nc);
    let response = response(challenge, input, hash, session, &nc);
    let algorithm = format!("{}{}", hash.name(), if session { "-sess" } else { "" });
    let mut header = format!(
        "Digest username=\"{}\", realm=\"{}\", uri=\"{}\", algorithm={algorithm}, nonce=\"{}\"",
        quote(input.username),
        quote(&challenge.realm),
        quote(input.uri),
        quote(&challenge.nonce),
    );
    if challenge.qop_auth {
        header.push_str(&format!(
            ", nc={nc}, cnonce=\"{}\", qop=auth",
            quote(input.cnonce)
        ));
    }
    header.push_str(&format!(", response=\"{response}\""));
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(", opaque=\"{}\"", quote(opaque)));
    }
    Ok(header)
}

fn response(
    challenge: &DigestChallenge,
    input: &DigestInput<'_>,
    hash: DigestHash,
    session: bool,
    nc: &str,
) -> String {
    let mut ha1 = hash.hash(&format!(
        "{}:{}:{}",
        input.username, challenge.realm, input.password
    ));
    if session {
        ha1 = hash.hash(&format!("{ha1}:{}:{}", challenge.nonce, input.cnonce));
    }
    let ha2 = hash.hash(&format!("{}:{}", input.method, input.uri));
    if challenge.qop_auth {
        hash.hash(&format!(
            "{ha1}:{}:{nc}:{}:auth:{ha2}",
            challenge.nonce, input.cnonce
        ))
    } else {
        hash.hash(&format!("{ha1}:{}:{ha2}", challenge.nonce))
    }
}

fn quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const CHALLENGE: &str = "Digest realm=\"http-auth@example.org\", qop=\"auth, auth-int\", algorithm=SHA-256, nonce=\"7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v\", opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\"";
    const CNONCE: &str = "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ";

    fn input() -> DigestInput<'static> {
        DigestInput {
            method: "GET",
            uri: "/dir/index.html",
            username: "Mufasa",
            password: "Circle of Life",
            fallback: DigestHash::Md5,
            cnonce: CNONCE,
            nc: 1,
        }
    }

    fn rfc_challenge(algorithm: &str) -> DigestChallenge {
        let text = CHALLENGE.replace("SHA-256", algorithm);
        let headers = vec![("WWW-Authenticate".to_owned(), text)];
        match challenge(&headers) {
            Some(challenge) => challenge,
            None => panic!("the RFC 7616 challenge did not parse"),
        }
    }

    fn answer(challenge: &DigestChallenge) -> String {
        match authorization(challenge, &input()) {
            Ok(header) => header,
            Err(error) => panic!("{error}"),
        }
    }

    /// RFC 7616 section 3.9.1, the SHA-256 and MD5 responses.
    #[test]
    fn answers_the_rfc_7616_example() {
        let sha = rfc_challenge("SHA-256");
        assert_eq!(sha.realm, "http-auth@example.org");
        assert!(sha.qop_auth);
        assert_eq!(
            answer(&sha),
            "Digest username=\"Mufasa\", realm=\"http-auth@example.org\", uri=\"/dir/index.html\", \
             algorithm=SHA-256, nonce=\"7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v\", \
             nc=00000001, cnonce=\"f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ\", qop=auth, \
             response=\"753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1\", \
             opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\""
        );
        let md5 = answer(&rfc_challenge("MD5"));
        assert!(
            md5.contains("response=\"8ca523f5e9506fed4657c9700eebdbec\""),
            "{md5}"
        );
    }

    /// RFC 2617 section 3.5: the response without qop uses the nonce only.
    #[test]
    fn answers_a_challenge_without_qop_and_prefers_sha_256() {
        let headers = vec![
            (
                "www-authenticate".to_owned(),
                "Digest realm=\"r\", nonce=\"n\", algorithm=MD5".to_owned(),
            ),
            (
                "www-authenticate".to_owned(),
                "Digest realm=\"r\", nonce=\"n\", algorithm=SHA-256".to_owned(),
            ),
        ];
        let Some(found) = challenge(&headers) else {
            panic!("no challenge");
        };
        assert_eq!(found.algorithm.as_deref(), Some("SHA-256"));
        let header = answer(&DigestChallenge {
            algorithm: None,
            ..found
        });
        assert!(!header.contains("qop="), "{header}");
        let ha1 = DigestHash::Md5.hash("Mufasa:r:Circle of Life");
        let ha2 = DigestHash::Md5.hash("GET:/dir/index.html");
        let expected = DigestHash::Md5.hash(&format!("{ha1}:n:{ha2}"));
        assert!(
            header.contains(&format!("response=\"{expected}\"")),
            "{header}"
        );
    }

    #[test]
    fn refuses_auth_int_only_and_unknown_algorithms() {
        let headers = vec![(
            "WWW-Authenticate".to_owned(),
            "Digest realm=\"r\", nonce=\"n\", qop=\"auth-int\"".to_owned(),
        )];
        let Some(found) = challenge(&headers) else {
            panic!("no challenge");
        };
        assert_eq!(authorization(&found, &input()), Err(DigestError::AuthInt));
        let unknown = rfc_challenge("SHA-512-256");
        assert_eq!(
            authorization(&unknown, &input()),
            Err(DigestError::Algorithm("SHA-512-256".to_owned()))
        );
        let basic = vec![(
            "WWW-Authenticate".to_owned(),
            "Basic realm=\"r\"".to_owned(),
        )];
        assert_eq!(challenge(&basic), None);
    }
}
