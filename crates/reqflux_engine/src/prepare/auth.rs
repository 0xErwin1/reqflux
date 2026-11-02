//! The effective auth and how it is sent (S7, v1-features S3).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqflux_core::model::{
    ApiKeyLocation, Auth, DigestAlgorithm, Id, Request, TokenLocation, TypedAuth,
};
use reqflux_core::template::DynamicSource;
use reqflux_http::auth::digest::DigestHash;

use super::has_header;
use super::resolver::Resolver;
use crate::auth::PendingAuth;
use crate::error::ExecuteError;
use crate::secrets::{SecretKey, SecretScope};
use crate::select::Target;
use crate::sent::Secrets;

const AUTHORIZATION: &str = "Authorization";

/// The document whose auth is in effect: an OAuth 2.0 token is kept per owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Request(Id),
    Folder(Id),
    Collection(Id),
}

impl Owner {
    /// The secret store key of the OAuth 2.0 token of this owner in `collection` of
    /// `workspace`: `<workspace>/collection/<collection>/oauth2/<request|folder|collection>/<id>`.
    pub(crate) fn token_key(self, workspace: Id, collection: Id) -> SecretKey {
        let owner = match self {
            Self::Request(id) => format!("request/{id}"),
            Self::Folder(id) => format!("folder/{id}"),
            Self::Collection(id) => format!("collection/{id}"),
        };
        SecretKey {
            workspace_id: workspace,
            scope: SecretScope::Collection(collection),
            key: format!("oauth2/{owner}"),
        }
    }
}

/// The auth of the request, or of the nearest folder, then the collection, that does not
/// inherit, with the document it comes from. A collection that inherits means no auth.
/// `request` is the document being prepared, so a draft carries its own auth in front of
/// the same walk.
pub(crate) fn effective<'a>(request: &'a Request, target: &Target<'a>) -> (Owner, &'a Auth) {
    std::iter::once((Owner::Request(request.id), &request.auth))
        .chain(
            target
                .folders
                .iter()
                .rev()
                .map(|folder| (Owner::Folder(folder.folder.id), &folder.folder.auth)),
        )
        .chain(std::iter::once((
            Owner::Collection(target.collection.collection.id),
            &target.collection.collection.auth,
        )))
        .find(|(_, auth)| !auth.is_inherit())
        .unwrap_or((
            Owner::Collection(target.collection.collection.id),
            &Auth::None,
        ))
}

/// Adds `auth` to `headers` or `query`. An enabled request header with the same name wins,
/// and then the auth fields are not resolved at all. The resolved password, token or key
/// value, and the encoded basic credentials, go to `secrets` so history masks them.
///
/// Digest, AWS SigV4 and OAuth 2.0 come back as the [`PendingAuth`] the send completes;
/// their credentials go to `secrets` too. `token` is where an OAuth 2.0 token is kept.
pub(super) fn apply<D: DynamicSource>(
    auth: &Auth,
    token: SecretKey,
    resolver: &mut Resolver<'_, D>,
    headers: &mut Vec<(String, String)>,
    query: &mut Vec<(String, String)>,
    secrets: &mut Secrets,
) -> Result<Option<PendingAuth>, ExecuteError> {
    match auth {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => {
            if !has_header(headers, AUTHORIZATION) {
                let password = resolver.text(password);
                let credentials = format!("{}:{password}", resolver.text(username));
                let encoded = STANDARD.encode(credentials);
                secrets.add(password);
                secrets.add(encoded.clone());
                headers.push((AUTHORIZATION.to_owned(), format!("Basic {encoded}")));
            }
        }
        Auth::Bearer { token } => {
            if !has_header(headers, AUTHORIZATION) {
                let token = resolver.text(token);
                headers.push((AUTHORIZATION.to_owned(), format!("Bearer {token}")));
                secrets.add(token);
            }
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let key = resolver.text(key);
            match location {
                ApiKeyLocation::Header if has_header(headers, &key) => {}
                ApiKeyLocation::Header => {
                    let value = resolver.text(value);
                    secrets.add(value.clone());
                    headers.push((key, value));
                }
                ApiKeyLocation::Query => {
                    let value = resolver.text(value);
                    secrets.add(value.clone());
                    query.push((key, value));
                }
            }
        }
        Auth::Other { kind, .. } => {
            let typed = auth
                .typed()
                .ok_or_else(|| ExecuteError::UnsupportedAuth(kind.clone()))?
                .map_err(|error| ExecuteError::InvalidAuth(error.kind))?;
            let in_query = matches!(
                &typed,
                TypedAuth::OAuth2(oauth) if oauth.token_location == TokenLocation::Query
            );
            if !in_query && has_header(headers, AUTHORIZATION) {
                return Ok(None);
            }
            return Ok(Some(pending(typed, token, resolver, secrets)));
        }
    }
    Ok(None)
}

/// Resolves the fields of a typed auth, adding every credential to `secrets`.
fn pending<D: DynamicSource>(
    typed: TypedAuth,
    token: SecretKey,
    resolver: &mut Resolver<'_, D>,
    secrets: &mut Secrets,
) -> PendingAuth {
    let mut secret = |resolver: &mut Resolver<'_, D>, text: &str| {
        let value = resolver.text(text);
        secrets.add(value.clone());
        value
    };
    match typed {
        TypedAuth::Digest(digest) => PendingAuth::Digest {
            username: resolver.text(&digest.username),
            password: secret(resolver, &digest.password),
            fallback: match digest.algorithm {
                DigestAlgorithm::Md5 => DigestHash::Md5,
                DigestAlgorithm::Sha256 => DigestHash::Sha256,
            },
        },
        TypedAuth::AwsV4(aws) => PendingAuth::AwsV4 {
            access_key: resolver.text(&aws.access_key),
            secret_key: secret(resolver, &aws.secret_key),
            session_token: secret(resolver, &aws.session_token),
            region: resolver.text(&aws.region),
            service: resolver.text(&aws.service),
        },
        TypedAuth::OAuth2(mut oauth) => {
            oauth.token_url = resolver.text(&oauth.token_url);
            oauth.auth_url = resolver.text(&oauth.auth_url);
            oauth.client_id = resolver.text(&oauth.client_id);
            oauth.client_secret = secret(resolver, &oauth.client_secret);
            oauth.scope = resolver.text(&oauth.scope);
            oauth.username = resolver.text(&oauth.username);
            oauth.password = secret(resolver, &oauth.password);
            oauth.refresh_token = secret(resolver, &oauth.refresh_token);
            PendingAuth::OAuth2 {
                config: oauth,
                token,
            }
        }
    }
}
