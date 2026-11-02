use pretty_assertions::assert_eq;

use super::super::{AwsV4Auth, DIGEST, DigestAuth, OAuth2Grant};
use super::*;

fn table(text: &str) -> toml::Table {
    match toml::from_str(text) {
        Ok(table) => table,
        Err(error) => panic!("deserialize failed: {error}"),
    }
}

fn other(kind: &str, text: &str) -> Auth {
    Auth::Other {
        kind: kind.to_owned(),
        fields: table(text),
    }
}

#[test]
fn an_edit_writes_only_the_changed_key_under_its_spelling() {
    let auth = other(
        OAUTH2,
        "grant_type = \"password_credentials\"\naccessTokenUrl = \"https://t\"\nclientId = \"app\"\nclientSecret = \"cs\"\nx_extra = 1\n",
    );
    let Some(Ok(TypedAuth::OAuth2(mut oauth))) = auth.typed() else {
        panic!("an OAuth 2.0 view");
    };
    oauth.client_id = "other".to_owned();

    assert_eq!(
        auth.with_typed(TypedAuth::OAuth2(oauth)),
        other(
            OAUTH2,
            "grant_type = \"password_credentials\"\naccessTokenUrl = \"https://t\"\nclientId = \"other\"\nclientSecret = \"cs\"\nx_extra = 1\n",
        )
    );
}

#[test]
fn a_changed_enum_and_an_emptied_field_keep_their_keys() {
    let auth = other(
        OAUTH2,
        "grantType = \"client_credentials\"\ntoken_url = \"t\"\nclientSecret = \"cs\"\n",
    );
    let Some(Ok(TypedAuth::OAuth2(mut oauth))) = auth.typed() else {
        panic!("an OAuth 2.0 view");
    };
    oauth.grant_type = OAuth2Grant::Password;
    oauth.client_secret = String::new();
    oauth.scope = "read".to_owned();

    assert_eq!(
        auth.with_typed(TypedAuth::OAuth2(oauth)),
        other(
            OAUTH2,
            "grantType = \"password\"\ntoken_url = \"t\"\nclientSecret = \"\"\nscope = \"read\"\n",
        )
    );
}

#[test]
fn a_new_type_starts_canonical() {
    let aws = AwsV4Auth {
        access_key: "AK".to_owned(),
        ..AwsV4Auth::default()
    };
    let from_digest = other(DIGEST, "username = \"u\"\npassword = \"p\"\nextra = 1\n");
    assert_eq!(
        from_digest.with_typed(TypedAuth::AwsV4(aws.clone())),
        Auth::from(aws.clone())
    );
    assert_eq!(
        Auth::None.with_typed(TypedAuth::Digest(DigestAuth::default())),
        Auth::from(DigestAuth::default())
    );
}
