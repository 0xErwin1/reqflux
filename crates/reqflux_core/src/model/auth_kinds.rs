//! The auth types kept as [`Auth::Other`] on disk and read through typed views: digest,
//! AWS Signature Version 4 and OAuth 2.0 (v1-features S3).
//!
//! They stay `Other` in the model so a front end that does not edit them yet shows and masks
//! every field. [`Auth::typed`] reads the fields, accepting the ReqFlux keys and the keys a
//! Postman import keeps (`accessKey`, `grant_type = "password_credentials"`, ...), and
//! ignoring keys it does not use. The `From` conversions build the `Other` value in the
//! canonical key order, so a new auth the app writes is stable; [`Auth::with_typed`] writes an
//! edit back into an existing table, keeping its other keys and their spelling.

use serde::{Deserialize, Serialize};

use super::auth::Auth;

/// The `type` of a digest auth.
pub const DIGEST: &str = "digest";
/// The `type` of an AWS Signature Version 4 auth.
pub const AWS_V4: &str = "awsv4";
/// The `type` of an OAuth 2.0 auth.
pub const OAUTH2: &str = "oauth2";

/// One of the typed auth views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedAuth {
    /// HTTP digest (RFC 7616).
    Digest(DigestAuth),
    /// AWS Signature Version 4.
    AwsV4(AwsV4Auth),
    /// OAuth 2.0 with a token the engine acquires.
    OAuth2(Box<OAuth2Auth>),
}

/// The fields of a typed auth do not read: a missing key or a value of the wrong type. The
/// message names the type, never a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the {kind} auth fields are not valid")]
pub struct AuthFieldsError {
    /// The auth `type`.
    pub kind: String,
}

/// The digest algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DigestAlgorithm {
    /// MD5, the RFC 2617 default.
    #[default]
    #[serde(rename = "MD5")]
    Md5,
    /// SHA-256 (RFC 7616).
    #[serde(rename = "SHA-256")]
    Sha256,
}

/// HTTP digest auth: the server's challenge on a 401 is answered once.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DigestAuth {
    /// The user name.
    pub username: String,
    /// The password.
    pub password: String,
    /// The algorithm used when the challenge names none.
    #[serde(default)]
    pub algorithm: DigestAlgorithm,
}

/// AWS Signature Version 4 header signing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AwsV4Auth {
    /// The access key id.
    #[serde(alias = "accessKey")]
    pub access_key: String,
    /// The secret access key; normally a `{{secret}}` variable.
    #[serde(alias = "secretKey")]
    pub secret_key: String,
    /// The session token of temporary credentials; empty for none.
    #[serde(
        default,
        alias = "sessionToken",
        skip_serializing_if = "String::is_empty"
    )]
    pub session_token: String,
    /// The region, such as `us-east-1`.
    pub region: String,
    /// The service, such as `s3` or `execute-api`.
    pub service: String,
}

/// How an OAuth 2.0 token is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    /// The client's own credentials.
    #[default]
    ClientCredentials,
    /// A user name and password (resource owner password credentials).
    #[serde(alias = "password_credentials")]
    Password,
    /// Authorization code with PKCE (S256) through the system browser and a loopback
    /// redirect.
    #[serde(alias = "authorization_code_with_pkce")]
    AuthorizationCode,
    /// A refresh token given in the fields.
    RefreshToken,
}

/// How the client authenticates to the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthentication {
    /// `client_id` and `client_secret` in the form body.
    #[default]
    Body,
    /// HTTP basic auth with the client id and secret.
    Header,
}

/// Where the access token is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenLocation {
    /// `Authorization: <prefix> <token>`.
    #[default]
    Header,
    /// A query parameter.
    #[serde(alias = "queryParams")]
    Query,
}

/// OAuth 2.0: the engine acquires the token, keeps it in the OS keyring and refreshes it
/// when it has expired.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OAuth2Auth {
    /// How the token is obtained.
    #[serde(default, alias = "grantType")]
    pub grant_type: OAuth2Grant,
    /// The token endpoint.
    #[serde(alias = "accessTokenUrl")]
    pub token_url: String,
    /// The authorization endpoint, for the authorization code grant.
    #[serde(default, alias = "authUrl", skip_serializing_if = "String::is_empty")]
    pub auth_url: String,
    /// The client id.
    #[serde(default, alias = "clientId")]
    pub client_id: String,
    /// The client secret; empty for a public client.
    #[serde(
        default,
        alias = "clientSecret",
        skip_serializing_if = "String::is_empty"
    )]
    pub client_secret: String,
    /// The requested scope, space-separated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// The user name, for the password grant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    /// The password, for the password grant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// The refresh token, for the refresh token grant.
    #[serde(
        default,
        alias = "refreshToken",
        skip_serializing_if = "String::is_empty"
    )]
    pub refresh_token: String,
    /// How the client authenticates to the token endpoint.
    #[serde(default, alias = "client_authentication")]
    pub client_auth: ClientAuthentication,
    /// Where the token is sent.
    #[serde(default, alias = "addTokenTo")]
    pub token_location: TokenLocation,
    /// The `Authorization` scheme; empty means `Bearer`.
    #[serde(
        default,
        alias = "headerPrefix",
        skip_serializing_if = "String::is_empty"
    )]
    pub header_prefix: String,
    /// The query parameter name; empty means `access_token`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub query_param: String,
}

impl OAuth2Auth {
    /// The `Authorization` scheme the token is sent with.
    #[must_use]
    pub fn scheme(&self) -> &str {
        if self.header_prefix.is_empty() {
            "Bearer"
        } else {
            &self.header_prefix
        }
    }

    /// The query parameter the token is sent in.
    #[must_use]
    pub fn query_name(&self) -> &str {
        if self.query_param.is_empty() {
            "access_token"
        } else {
            &self.query_param
        }
    }
}

impl TypedAuth {
    /// The field values that are credentials, as written: what history and every masked
    /// view must hide.
    #[must_use]
    pub fn secret_values(&self) -> Vec<&str> {
        match self {
            Self::Digest(digest) => vec![digest.password.as_str()],
            Self::AwsV4(aws) => vec![aws.secret_key.as_str(), aws.session_token.as_str()],
            Self::OAuth2(oauth) => vec![
                oauth.client_secret.as_str(),
                oauth.password.as_str(),
                oauth.refresh_token.as_str(),
            ],
        }
    }
}

/// The keys of each typed auth that hold a credential, in the ReqFlux and the Postman
/// spelling. A front end masks these.
#[must_use]
pub fn secret_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        DIGEST => &["password"],
        AWS_V4 => &["secret_key", "secretKey", "session_token", "sessionToken"],
        OAUTH2 => &[
            "client_secret",
            "clientSecret",
            "password",
            "refresh_token",
            "refreshToken",
            "accessToken",
        ],
        _ => &[],
    }
}

impl Auth {
    /// The typed view of a digest, AWS SigV4 or OAuth 2.0 auth; `None` for any other auth.
    ///
    /// # Errors
    ///
    /// The fields of a typed kind do not read.
    pub fn typed(&self) -> Option<Result<TypedAuth, AuthFieldsError>> {
        let Self::Other { kind, fields } = self else {
            return None;
        };
        let value = || toml::Value::Table(fields.clone());
        let invalid = || AuthFieldsError { kind: kind.clone() };
        let typed = match kind.as_str() {
            DIGEST => value().try_into().map(TypedAuth::Digest),
            AWS_V4 => value().try_into().map(TypedAuth::AwsV4),
            OAUTH2 => value()
                .try_into()
                .map(|oauth| TypedAuth::OAuth2(Box::new(oauth))),
            _ => return None,
        };
        Some(typed.map_err(|_| invalid()))
    }
}

fn other(kind: &str, value: &impl Serialize) -> Auth {
    let fields = match toml::Value::try_from(value) {
        Ok(toml::Value::Table(fields)) => fields,
        // The views are plain string and enum fields, which always serialize to a table.
        _ => toml::Table::new(),
    };
    Auth::Other {
        kind: kind.to_owned(),
        fields,
    }
}

impl From<DigestAuth> for Auth {
    fn from(value: DigestAuth) -> Self {
        other(DIGEST, &value)
    }
}

impl From<AwsV4Auth> for Auth {
    fn from(value: AwsV4Auth) -> Self {
        other(AWS_V4, &value)
    }
}

impl From<OAuth2Auth> for Auth {
    fn from(value: OAuth2Auth) -> Self {
        other(OAUTH2, &value)
    }
}

impl From<TypedAuth> for Auth {
    fn from(value: TypedAuth) -> Self {
        match value {
            TypedAuth::Digest(digest) => digest.into(),
            TypedAuth::AwsV4(aws) => aws.into(),
            TypedAuth::OAuth2(oauth) => (*oauth).into(),
        }
    }
}

mod edit;

#[cfg(test)]
mod tests;
