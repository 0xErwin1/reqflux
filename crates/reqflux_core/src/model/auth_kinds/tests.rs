use pretty_assertions::assert_eq;

use super::*;
use crate::model::Folder;

const HEAD: &str = "id = \"01JBQ3Z7W4X9V8N2K5M6P7R8S9\"\nname = \"F\"\n\n";

fn folder(auth: &str) -> Folder {
    match toml::from_str(&format!("{HEAD}{auth}")) {
        Ok(folder) => folder,
        Err(error) => panic!("deserialize failed: {error}"),
    }
}

fn written(folder: &Folder) -> String {
    match toml::to_string(folder) {
        Ok(text) => text,
        Err(error) => panic!("serialize failed: {error}"),
    }
}

fn typed(auth: &Auth) -> TypedAuth {
    match auth.typed() {
        Some(Ok(typed)) => typed,
        other => panic!("expected a typed auth, got {other:?}"),
    }
}

#[test]
fn digest_round_trips_and_reads_typed() {
    let text = "[auth]\ntype = \"digest\"\nusername = \"u\"\npassword = \"{{pw}}\"\nalgorithm = \"SHA-256\"\n";
    let folder = folder(text);
    assert_eq!(written(&folder), format!("{HEAD}{text}"));
    assert_eq!(
        typed(&folder.auth),
        TypedAuth::Digest(DigestAuth {
            username: "u".to_owned(),
            password: "{{pw}}".to_owned(),
            algorithm: DigestAlgorithm::Sha256,
        })
    );
}

#[test]
fn constructors_write_the_canonical_key_order_and_skip_empty_fields() {
    let aws: Auth = AwsV4Auth {
        access_key: "AKID".to_owned(),
        secret_key: "{{aws_secret}}".to_owned(),
        session_token: String::new(),
        region: "us-east-1".to_owned(),
        service: "s3".to_owned(),
    }
    .into();
    let oauth: Auth = OAuth2Auth {
        grant_type: OAuth2Grant::AuthorizationCode,
        token_url: "https://idp.test/token".to_owned(),
        auth_url: "https://idp.test/authorize".to_owned(),
        client_id: "app".to_owned(),
        scope: "read".to_owned(),
        ..OAuth2Auth::default()
    }
    .into();
    let mut with = folder("");
    with.auth = aws;
    assert_eq!(
        written(&with),
        format!(
            "{HEAD}[auth]\ntype = \"awsv4\"\naccess_key = \"AKID\"\nsecret_key = \"{{{{aws_secret}}}}\"\nregion = \"us-east-1\"\nservice = \"s3\"\n"
        )
    );
    with.auth = oauth;
    assert_eq!(
        written(&with),
        format!(
            "{HEAD}[auth]\ntype = \"oauth2\"\ngrant_type = \"authorization_code\"\ntoken_url = \"https://idp.test/token\"\nauth_url = \"https://idp.test/authorize\"\nclient_id = \"app\"\nscope = \"read\"\nclient_auth = \"body\"\ntoken_location = \"header\"\n"
        )
    );
}

#[test]
fn postman_keys_read_as_the_typed_view() {
    let aws = folder(
        "[auth]\ntype = \"awsv4\"\naccessKey = \"AKID\"\nsecretKey = \"s\"\nsessionToken = \"t\"\nregion = \"eu-west-1\"\nservice = \"execute-api\"\naddAuthDataToQuery = false\n",
    );
    assert_eq!(
        typed(&aws.auth),
        TypedAuth::AwsV4(AwsV4Auth {
            access_key: "AKID".to_owned(),
            secret_key: "s".to_owned(),
            session_token: "t".to_owned(),
            region: "eu-west-1".to_owned(),
            service: "execute-api".to_owned(),
        })
    );
    let oauth = folder(
        "[auth]\ntype = \"oauth2\"\naccessTokenUrl = \"https://idp.test/token\"\nclientId = \"c\"\nclientSecret = \"cs\"\nclient_authentication = \"header\"\naddTokenTo = \"queryParams\"\ngrant_type = \"password_credentials\"\nusername = \"u\"\npassword = \"p\"\ntokenName = \"t\"\n",
    );
    let TypedAuth::OAuth2(oauth) = typed(&oauth.auth) else {
        panic!("expected oauth2");
    };
    assert_eq!(oauth.grant_type, OAuth2Grant::Password);
    assert_eq!(oauth.client_auth, ClientAuthentication::Header);
    assert_eq!(oauth.token_location, TokenLocation::Query);
    assert_eq!(oauth.token_url, "https://idp.test/token");
    assert_eq!(oauth.client_secret, "cs");
    assert_eq!(oauth.scheme(), "Bearer");
    assert_eq!(oauth.query_name(), "access_token");
    let pkce = folder(
        "[auth]\ntype = \"oauth2\"\naccessTokenUrl = \"u\"\ngrant_type = \"authorization_code_with_pkce\"\n",
    );
    let TypedAuth::OAuth2(pkce) = typed(&pkce.auth) else {
        panic!("expected oauth2");
    };
    assert_eq!(pkce.grant_type, OAuth2Grant::AuthorizationCode);
}

#[test]
fn invalid_fields_name_the_kind_and_no_value() {
    let digest = folder("[auth]\ntype = \"digest\"\nusername = \"u\"\npassword = 12345\n");
    let error = match digest.auth.typed() {
        Some(Err(error)) => error,
        other => panic!("expected an error, got {other:?}"),
    };
    assert_eq!(error.to_string(), "the digest auth fields are not valid");
    assert_eq!(
        Auth::Bearer {
            token: "t".to_owned()
        }
        .typed(),
        None
    );
    assert_eq!(folder("[auth]\ntype = \"hawk\"\n").auth.typed(), None);
}

#[test]
fn secret_values_and_fields_cover_every_credential() {
    let oauth = TypedAuth::OAuth2(Box::new(OAuth2Auth {
        client_secret: "cs".to_owned(),
        password: "pw".to_owned(),
        refresh_token: "rt".to_owned(),
        ..OAuth2Auth::default()
    }));
    assert_eq!(oauth.secret_values(), vec!["cs", "pw", "rt"]);
    assert!(secret_fields(AWS_V4).contains(&"session_token"));
    assert!(secret_fields(DIGEST).contains(&"password"));
    assert!(secret_fields("hawk").is_empty());
}
