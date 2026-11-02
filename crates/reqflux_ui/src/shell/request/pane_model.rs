//! The Body, Auth, Scripts and Settings panes without GPUI: the body mode, the auth type and
//! its fields with their masking, the settings text and the lines of a code block.

use reqflux_core::model::{
    ApiKeyLocation, Auth, AwsV4Auth, Body, DigestAlgorithm, DigestAuth, OAuth2Auth, OAuth2Grant,
    RawLanguage, Settings, TypedAuth, secret_fields,
};
use reqflux_engine::DEFAULT_TIMEOUT_MS;

pub(crate) use crate::shell::auth_edit::AuthMenu;

/// The body mode the pane header shows.
pub(crate) fn body_mode(body: &Body) -> String {
    match body {
        Body::None => "none".to_owned(),
        Body::Raw { language, .. } => format!("raw · {}", language_label(*language)),
        Body::Urlencoded { .. } => "urlencoded".to_owned(),
        Body::FormData { .. } => "form-data".to_owned(),
        Body::Binary { .. } => "binary".to_owned(),
        Body::Graphql { .. } => "GraphQL".to_owned(),
    }
}

fn language_label(language: RawLanguage) -> &'static str {
    match language {
        RawLanguage::Json => "JSON",
        RawLanguage::Xml => "XML",
        RawLanguage::Text => "Text",
        RawLanguage::Html => "HTML",
        RawLanguage::Javascript => "JavaScript",
    }
}

/// The auth type the pane header shows.
pub(crate) fn auth_type(auth: &Auth) -> &str {
    match auth {
        Auth::Inherit => "Inherit from parent",
        Auth::None => "No auth",
        Auth::Basic { .. } => "Basic",
        Auth::Bearer { .. } => "Bearer token",
        Auth::ApiKey { .. } => "API key",
        Auth::Other { kind, .. } => match kind.as_str() {
            "digest" => "Digest",
            "awsv4" => "AWS Signature v4",
            "oauth2" => "OAuth 2.0",
            _ => kind,
        },
    }
}

/// What a click on an auth field row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldEdit {
    /// Opens a field seeded with the text: Username and Key of basic and API key, or the
    /// typed field with this on-disk key.
    Text(&'static str),
    /// Opens an empty secret field: Password, Token and Value, or the typed field with this
    /// on-disk key.
    Secret(&'static str),
    /// Opens the menu.
    Select(AuthMenu),
    /// Add to, which its segments edit.
    Location,
    /// Nothing: a field of an auth type ReqFlux does not edit.
    None,
}

/// An auth field row. A `masked` value is never shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthField {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) masked: bool,
    pub(crate) edit: FieldEdit,
    /// What an empty value shows.
    pub(crate) placeholder: Option<&'static str>,
    /// The muted line under the control.
    pub(crate) hint: Option<&'static str>,
}

impl AuthField {
    /// The automation id, `request-auth-field-<lowercase label>` with dashes for spaces.
    pub(crate) fn element_id(&self) -> String {
        format!(
            "request-auth-field-{}",
            self.label.to_lowercase().replace(' ', "-")
        )
    }
}

/// The fields of `auth`. Passwords, tokens, API key values, the `secret_fields` of digest,
/// AWS SigV4 and OAuth 2.0, and every field of other types are always masked; other values
/// when they reference a secret, as `references_secret` (the engine's) decides.
pub(crate) fn auth_fields(auth: &Auth, references_secret: impl Fn(&str) -> bool) -> Vec<AuthField> {
    let field = |label: &str, value: &str, edit: FieldEdit| AuthField {
        label: label.to_owned(),
        value: value.to_owned(),
        masked: matches!(edit, FieldEdit::Secret(_)) || references_secret(value),
        edit,
        placeholder: None,
        hint: None,
    };
    match auth {
        Auth::Inherit | Auth::None => Vec::new(),
        Auth::Basic { username, password } => vec![
            field("Username", username, FieldEdit::Text("username")),
            field("Password", password, FieldEdit::Secret("password")),
        ],
        Auth::Bearer { token } => vec![field("Token", token, FieldEdit::Secret("token"))],
        Auth::ApiKey {
            key,
            value,
            location,
        } => vec![
            field("Key", key, FieldEdit::Text("key")),
            field("Value", value, FieldEdit::Secret("value")),
            field(
                "Add to",
                match location {
                    ApiKeyLocation::Header => "header",
                    ApiKeyLocation::Query => "query",
                },
                FieldEdit::Location,
            ),
        ],
        Auth::Other { kind, fields } => match auth.typed() {
            Some(Ok(typed)) => typed_fields(&typed)
                .into_iter()
                .map(|row| {
                    let mut shown = field(row.label, &row.value, row.edit);
                    shown.placeholder = row.placeholder;
                    shown.hint = row.hint;
                    shown
                })
                .collect(),
            // Fields that do not read stay read-only. A type that names its credential fields
            // masks those; for any other type every value is masked, since which ones hold
            // a credential is not known.
            _ => {
                let secrets = secret_fields(kind);
                fields
                    .iter()
                    .map(|(key, value)| {
                        let text = value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned);
                        let mut shown = field(key, &text, FieldEdit::None);
                        shown.masked |= secrets.is_empty() || secrets.contains(&key.as_str());
                        shown
                    })
                    .collect()
            }
        },
    }
}

/// A typed auth row before masking.
struct TypedRow {
    label: &'static str,
    value: String,
    edit: FieldEdit,
    placeholder: Option<&'static str>,
    hint: Option<&'static str>,
}

fn text_row(label: &'static str, key: &'static str, value: &str) -> TypedRow {
    TypedRow {
        label,
        value: value.to_owned(),
        edit: FieldEdit::Text(key),
        placeholder: None,
        hint: None,
    }
}

fn secret_row(label: &'static str, key: &'static str, value: &str) -> TypedRow {
    TypedRow {
        edit: FieldEdit::Secret(key),
        ..text_row(label, key, value)
    }
}

fn select_row(label: &'static str, menu: AuthMenu, value: &str) -> TypedRow {
    TypedRow {
        edit: FieldEdit::Select(menu),
        ..text_row(label, "", value)
    }
}

impl TypedRow {
    fn optional(self) -> Self {
        Self {
            placeholder: Some("Optional"),
            ..self
        }
    }

    fn hint(self, hint: &'static str) -> Self {
        Self {
            hint: Some(hint),
            ..self
        }
    }
}

/// The name the Algorithm select shows for `algorithm`, as written on disk.
pub(crate) fn algorithm_name(algorithm: DigestAlgorithm) -> &'static str {
    match algorithm {
        DigestAlgorithm::Md5 => "MD5",
        DigestAlgorithm::Sha256 => "SHA-256",
    }
}

/// The name the Grant type select shows for `grant`.
pub(crate) fn grant_name(grant: OAuth2Grant) -> &'static str {
    match grant {
        OAuth2Grant::ClientCredentials => "Client credentials",
        OAuth2Grant::AuthorizationCode => "Authorization code with PKCE",
        OAuth2Grant::Password => "Password",
        OAuth2Grant::RefreshToken => "Refresh token",
    }
}

/// The digest algorithms the Algorithm menu offers, in menu order, with their ids.
pub(crate) const ALGORITHMS: [(DigestAlgorithm, &str); 2] = [
    (DigestAlgorithm::Md5, "md5"),
    (DigestAlgorithm::Sha256, "sha-256"),
];

/// The OAuth 2.0 grants the Grant type menu offers, in menu order, with their ids.
pub(crate) const GRANTS: [(OAuth2Grant, &str); 4] = [
    (OAuth2Grant::ClientCredentials, "client-credentials"),
    (OAuth2Grant::AuthorizationCode, "authorization-code"),
    (OAuth2Grant::Password, "password"),
    (OAuth2Grant::RefreshToken, "refresh-token"),
];

fn typed_fields(typed: &TypedAuth) -> Vec<TypedRow> {
    match typed {
        TypedAuth::Digest(DigestAuth {
            username,
            password,
            algorithm,
        }) => vec![
            text_row("Username", "username", username),
            secret_row("Password", "password", password),
            select_row("Algorithm", AuthMenu::Algorithm, algorithm_name(*algorithm)).hint(
                "Used when the challenge names none. Realm, nonce and qop come from the \
                 server's challenge on the first send.",
            ),
        ],
        TypedAuth::AwsV4(AwsV4Auth {
            access_key,
            secret_key,
            session_token,
            region,
            service,
        }) => vec![
            text_row("Access key", "access_key", access_key),
            secret_row("Secret key", "secret_key", secret_key),
            secret_row("Session token", "session_token", session_token)
                .optional()
                .hint("Only for temporary credentials (STS, SSO)."),
            text_row("Region", "region", region),
            text_row("Service", "service", service).hint(
                "Send adds Authorization, X-Amz-Date and X-Amz-Content-Sha256, plus \
                 X-Amz-Security-Token with a session token.",
            ),
        ],
        TypedAuth::OAuth2(oauth) => oauth_fields(oauth),
    }
}

fn oauth_fields(oauth: &OAuth2Auth) -> Vec<TypedRow> {
    let mut rows = vec![select_row(
        "Grant type",
        AuthMenu::Grant,
        grant_name(oauth.grant_type),
    )];
    if oauth.grant_type == OAuth2Grant::AuthorizationCode {
        rows.push(text_row("Auth URL", "auth_url", &oauth.auth_url));
    }
    rows.push(text_row("Token URL", "token_url", &oauth.token_url));
    rows.push(text_row("Client ID", "client_id", &oauth.client_id));
    let client_secret = secret_row("Client secret", "client_secret", &oauth.client_secret);
    rows.push(if oauth.grant_type == OAuth2Grant::AuthorizationCode {
        client_secret
            .optional()
            .hint("Leave empty for a public client; PKCE (S256) is always used.")
    } else {
        client_secret
    });
    rows.push(text_row("Scopes", "scope", &oauth.scope).hint("Separated by spaces."));
    match oauth.grant_type {
        OAuth2Grant::Password => {
            rows.push(text_row("Username", "username", &oauth.username));
            rows.push(secret_row("Password", "password", &oauth.password));
        }
        OAuth2Grant::RefreshToken => {
            rows.push(secret_row(
                "Refresh token",
                "refresh_token",
                &oauth.refresh_token,
            ));
        }
        OAuth2Grant::ClientCredentials | OAuth2Grant::AuthorizationCode => {}
    }
    rows
}

/// The automation id and the text of each settings row, top to bottom.
pub(crate) fn settings_rows(settings: &Settings) -> [(&'static str, &'static str, String); 3] {
    let flag = |value: Option<bool>| match value {
        Some(true) => "on".to_owned(),
        Some(false) => "off".to_owned(),
        None => "Default (on)".to_owned(),
    };
    let timeout = match settings.timeout_ms {
        Some(ms) => format!("{ms} ms"),
        None => format!("Default ({DEFAULT_TIMEOUT_MS} ms)"),
    };
    [
        ("request-setting-timeout", "Timeout", timeout),
        (
            "request-setting-redirects",
            "Follow redirects",
            flag(settings.follow_redirects),
        ),
        (
            "request-setting-tls",
            "Verify TLS",
            flag(settings.verify_tls),
        ),
    ]
}

/// The lines of a code block, numbered from 1. An empty text is one empty line.
pub(crate) fn numbered_lines(text: &str) -> Vec<(usize, &str)> {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        lines.push("");
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
        .collect()
}

#[cfg(test)]
mod tests {
    use reqflux_core::model::{AwsV4Auth, DigestAlgorithm, DigestAuth, OAuth2Auth, OAuth2Grant};

    use super::*;

    fn secret(text: &str) -> bool {
        text.contains("{{token}}")
    }

    fn masks(fields: &[AuthField]) -> Vec<(&str, bool)> {
        fields
            .iter()
            .map(|field| (field.label.as_str(), field.masked))
            .collect()
    }

    #[test]
    fn body_modes_name_the_raw_language() {
        let raw = Body::Raw {
            language: RawLanguage::Json,
            text: String::new(),
        };
        assert_eq!(body_mode(&raw), "raw · JSON");
        assert_eq!(body_mode(&Body::None), "none");
        let graphql = Body::Graphql {
            query: String::new(),
            variables: String::new(),
        };
        assert_eq!(body_mode(&graphql), "GraphQL");
    }

    #[test]
    fn plain_tokens_and_passwords_are_masked() {
        let bearer = Auth::Bearer {
            token: "plain".to_owned(),
        };
        assert_eq!(masks(&auth_fields(&bearer, secret)), [("Token", true)]);
        let basic = Auth::Basic {
            username: "me".to_owned(),
            password: "plain".to_owned(),
        };
        assert_eq!(
            masks(&auth_fields(&basic, secret)),
            [("Username", false), ("Password", true)]
        );
        assert_eq!(auth_type(&basic), "Basic");
    }

    #[test]
    fn an_api_key_value_referencing_a_secret_is_masked() {
        let auth = Auth::ApiKey {
            key: "X-Key".to_owned(),
            value: "{{token}}".to_owned(),
            location: ApiKeyLocation::Query,
        };
        let fields = auth_fields(&auth, secret);
        assert_eq!(
            masks(&fields),
            [("Key", false), ("Value", true), ("Add to", false)]
        );
        let ids: Vec<_> = fields.iter().map(AuthField::element_id).collect();
        assert_eq!(
            ids.last().map(String::as_str),
            Some("request-auth-field-add-to")
        );
    }

    fn rows(auth: &Auth) -> Vec<(String, String, bool, FieldEdit)> {
        auth_fields(auth, secret)
            .into_iter()
            .map(|field| (field.label, field.value, field.masked, field.edit))
            .collect()
    }

    fn row(
        label: &str,
        value: &str,
        masked: bool,
        edit: FieldEdit,
    ) -> (String, String, bool, FieldEdit) {
        (label.to_owned(), value.to_owned(), masked, edit)
    }

    #[test]
    fn digest_masks_the_password_and_selects_the_algorithm() {
        let auth: Auth = DigestAuth {
            username: "bot".to_owned(),
            password: "literal".to_owned(),
            algorithm: DigestAlgorithm::Sha256,
        }
        .into();
        assert_eq!(auth_type(&auth), "Digest");
        assert_eq!(
            rows(&auth),
            [
                row("Username", "bot", false, FieldEdit::Text("username")),
                row("Password", "literal", true, FieldEdit::Secret("password")),
                row(
                    "Algorithm",
                    "SHA-256",
                    false,
                    FieldEdit::Select(AuthMenu::Algorithm)
                ),
            ]
        );
    }

    #[test]
    fn aws_masks_both_secrets_and_shows_region_and_service() {
        let auth: Auth = AwsV4Auth {
            access_key: "AKID".to_owned(),
            secret_key: "literal".to_owned(),
            session_token: String::new(),
            region: "eu-west-1".to_owned(),
            service: "execute-api".to_owned(),
        }
        .into();
        assert_eq!(auth_type(&auth), "AWS Signature v4");
        assert_eq!(
            rows(&auth),
            [
                row("Access key", "AKID", false, FieldEdit::Text("access_key")),
                row(
                    "Secret key",
                    "literal",
                    true,
                    FieldEdit::Secret("secret_key")
                ),
                row(
                    "Session token",
                    "",
                    true,
                    FieldEdit::Secret("session_token")
                ),
                row("Region", "eu-west-1", false, FieldEdit::Text("region")),
                row("Service", "execute-api", false, FieldEdit::Text("service")),
            ]
        );
        let fields = auth_fields(&auth, secret);
        let optional: Vec<_> = fields.iter().map(|field| field.placeholder).collect();
        assert_eq!(optional, [None, None, Some("Optional"), None, None]);
    }

    #[test]
    fn oauth2_rows_follow_the_grant() {
        let mut oauth = OAuth2Auth {
            token_url: "https://id/token".to_owned(),
            client_id: "billing".to_owned(),
            client_secret: "literal".to_owned(),
            scope: "read".to_owned(),
            ..OAuth2Auth::default()
        };
        let labels = |oauth: &OAuth2Auth| -> Vec<String> {
            auth_fields(&oauth.clone().into(), secret)
                .into_iter()
                .map(|field| field.label)
                .collect()
        };
        assert_eq!(auth_type(&oauth.clone().into()), "OAuth 2.0");
        assert_eq!(
            rows(&oauth.clone().into()),
            [
                row(
                    "Grant type",
                    "Client credentials",
                    false,
                    FieldEdit::Select(AuthMenu::Grant)
                ),
                row(
                    "Token URL",
                    "https://id/token",
                    false,
                    FieldEdit::Text("token_url")
                ),
                row("Client ID", "billing", false, FieldEdit::Text("client_id")),
                row(
                    "Client secret",
                    "literal",
                    true,
                    FieldEdit::Secret("client_secret")
                ),
                row("Scopes", "read", false, FieldEdit::Text("scope")),
            ]
        );
        oauth.grant_type = OAuth2Grant::AuthorizationCode;
        assert_eq!(
            labels(&oauth),
            [
                "Grant type",
                "Auth URL",
                "Token URL",
                "Client ID",
                "Client secret",
                "Scopes"
            ]
        );
        oauth.grant_type = OAuth2Grant::Password;
        assert_eq!(
            labels(&oauth),
            [
                "Grant type",
                "Token URL",
                "Client ID",
                "Client secret",
                "Scopes",
                "Username",
                "Password"
            ]
        );
        oauth.grant_type = OAuth2Grant::RefreshToken;
        assert_eq!(
            labels(&oauth),
            [
                "Grant type",
                "Token URL",
                "Client ID",
                "Client secret",
                "Scopes",
                "Refresh token"
            ]
        );
        let ids: Vec<_> = auth_fields(&oauth.into(), secret)
            .iter()
            .map(AuthField::element_id)
            .collect();
        assert_eq!(
            ids.last().map(String::as_str),
            Some("request-auth-field-refresh-token")
        );
    }

    #[test]
    fn a_typed_field_referencing_a_secret_is_masked() {
        let auth: Auth = AwsV4Auth {
            access_key: "{{token}}".to_owned(),
            ..AwsV4Auth::default()
        }
        .into();
        assert_eq!(
            auth_fields(&auth, secret).first().map(|field| field.masked),
            Some(true)
        );
    }

    #[test]
    fn invalid_typed_fields_mask_only_their_secret_fields_read_only() {
        let mut fields = toml::Table::new();
        fields.insert("region".to_owned(), "eu-west-1".into());
        fields.insert("secret_key".to_owned(), "literal".into());
        let auth = Auth::Other {
            kind: "awsv4".to_owned(),
            fields,
        };
        assert_eq!(
            rows(&auth),
            [
                row("region", "eu-west-1", false, FieldEdit::None),
                row("secret_key", "literal", true, FieldEdit::None),
            ]
        );
    }

    #[test]
    fn every_field_of_another_auth_type_is_masked() {
        let mut fields = toml::Table::new();
        fields.insert("client_secret".to_owned(), "plain".into());
        fields.insert("expires_in".to_owned(), 3600.into());
        let other = Auth::Other {
            kind: "hawk".to_owned(),
            fields,
        };
        let masked: Vec<_> = auth_fields(&other, |_| false)
            .into_iter()
            .map(|field| (field.label, field.masked))
            .collect();
        assert_eq!(
            masked,
            [
                ("client_secret".to_owned(), true),
                ("expires_in".to_owned(), true)
            ]
        );
    }

    #[test]
    fn a_plain_api_key_value_is_masked() {
        let auth = Auth::ApiKey {
            key: "X-Key".to_owned(),
            value: "plain-key".to_owned(),
            location: ApiKeyLocation::Header,
        };
        assert_eq!(
            masks(&auth_fields(&auth, |_| false)),
            [("Key", false), ("Value", true), ("Add to", false)]
        );
    }

    #[test]
    fn settings_show_defaults() {
        let texts: Vec<_> = settings_rows(&Settings::default())
            .into_iter()
            .map(|(_, _, text)| text)
            .collect();
        assert_eq!(
            texts,
            ["Default (30000 ms)", "Default (on)", "Default (on)"]
        );
        let set = Settings {
            timeout_ms: Some(5000),
            follow_redirects: Some(false),
            verify_tls: None,
        };
        assert_eq!(settings_rows(&set)[0].2, "5000 ms");
    }

    #[test]
    fn lines_are_numbered_from_one() {
        assert_eq!(numbered_lines("a\nb"), [(1, "a"), (2, "b")]);
        assert_eq!(numbered_lines(""), [(1, "")]);
    }
}
