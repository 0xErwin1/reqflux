//! The authorization code grant with PKCE (RFC 7636, S256) through the system browser and a
//! loopback redirect on `127.0.0.1` with a random port (RFC 8252).

use std::fmt::Debug;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqflux_core::model::OAuth2Auth;
use reqflux_http::HttpError;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The path the authorization server redirects back to.
const CALLBACK: &str = "/callback";

/// The most bytes read from a callback request before its head must have ended.
const MAX_HEAD: usize = 16 * 1024;

/// Opens an authorization URL for the user. The app opens the system browser; a test drives
/// the redirect itself.
pub trait Browser: Debug + Send + Sync {
    /// Opens `url`.
    ///
    /// # Errors
    ///
    /// The URL could not be opened; the text says why without repeating the URL.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The system browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBrowser;

impl Browser for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        let opened = if crate::child_environment::inherits_driver_selection() {
            open_without_driver_selection(url)
        } else {
            open::that_detached(url)
        };
        opened.map_err(|error| format!("the browser did not open: {error}"))
    }
}

/// Opens `url` the way `open::that_detached` does, with the Vulkan driver selection GPUI's
/// startup made removed from each launcher's environment. `open::that_detached` builds its own
/// commands, so it cannot drop the selection, and the browser would keep the boot GPU's driver.
fn open_without_driver_selection(url: &str) -> std::io::Result<()> {
    let mut commands = open::commands(url);
    for command in &mut commands {
        crate::child_environment::restore_child_environment(command);
    }
    open_with(commands, LAUNCHER_FAILURE_WINDOW)
}

/// How long a launcher has to fail before it counts as started, as `open::that_detached` waits.
const LAUNCHER_FAILURE_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

/// Runs each launcher in turn, detached, until one starts: a launcher that cannot spawn or that
/// exits with a failure within `window` gives way to the next one. One still running after
/// `window`, or one that exits successfully, counts as started and is reaped off this thread.
/// Without any that starts, the first failure is reported.
fn open_with(
    commands: Vec<std::process::Command>,
    window: std::time::Duration,
) -> std::io::Result<()> {
    let mut first_error = None;
    for mut command in commands {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let failure = match command.spawn() {
            Ok(child) => match settle(child, window) {
                Ok(()) => return Ok(()),
                Err(error) => error,
            },
            Err(error) => error,
        };
        first_error.get_or_insert(failure);
    }
    Err(first_error.unwrap_or_else(|| std::io::Error::other("no launcher is available")))
}

/// Waits up to `window` for a launcher to fail; one that has not exited by then is left to run.
fn settle(mut child: std::process::Child, window: std::time::Duration) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + window;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "the launcher exited with {status}"
                )))
            };
        }
        if std::time::Instant::now() >= deadline {
            // The launcher may outlive the request; reap it off this thread.
            std::thread::spawn(move || {
                if let Err(error) = child.wait() {
                    log::warn!("the browser launcher did not finish: {error}");
                }
            });
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// What the browser came back with.
pub(super) struct Authorization {
    pub(super) code: String,
    pub(super) redirect_uri: String,
    pub(super) verifier: String,
}

/// URL-safe random text from `bytes` random bytes.
pub(super) fn random_text(bytes: usize) -> String {
    let random: Vec<u8> = (0..bytes).map(|_| rand::random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(random)
}

/// Listens on a random loopback port, opens the authorization URL and waits for the
/// redirect whose `state` matches. A request to another path is answered 404 and the wait
/// goes on; a wrong `state` or an `error` ends it.
pub(super) async fn authorize(
    browser: &dyn Browser,
    config: &OAuth2Auth,
) -> Result<Authorization, HttpError> {
    let failed = |reason: String| HttpError::Auth(reason);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| failed(format!("the loopback redirect did not start: {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| failed(format!("the loopback redirect did not start: {error}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK}");
    let verifier = random_text(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text(16);

    let mut url = url::Url::parse(&config.auth_url)
        .map_err(|_| failed("the OAuth 2.0 authorization URL is not valid".to_owned()))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if !config.scope.is_empty() {
            query.append_pair("scope", &config.scope);
        }
    }
    browser.open(url.as_str()).map_err(failed)?;

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|error| failed(format!("the loopback redirect failed: {error}")))?;
        if let Some(outcome) = callback(stream, &state).await {
            return outcome.map(|code| Authorization {
                code,
                redirect_uri,
                verifier,
            });
        }
    }
}

/// Answers one loopback request: `None` when it is not the callback, else the code or why
/// the authorization failed.
async fn callback(mut stream: TcpStream, state: &str) -> Option<Result<String, HttpError>> {
    let head = read_head(&mut stream).await?;
    let target = head.lines().next()?.split_whitespace().nth(1)?.to_owned();
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        respond(&mut stream, "400 Bad Request", "Bad request.").await;
        return None;
    };
    if url.path() != CALLBACK {
        respond(&mut stream, "404 Not Found", "Not found.").await;
        return None;
    }
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    let outcome = if param("state").as_deref() != Some(state) {
        Err("the authorization redirect has the wrong state".to_owned())
    } else if let Some(error) = param("error") {
        Err(format!("the authorization server answered {error}"))
    } else {
        param("code").ok_or_else(|| "the authorization redirect has no code".to_owned())
    };
    match &outcome {
        Ok(_) => {
            respond(
                &mut stream,
                "200 OK",
                "ReqFlux is authorized. You can close this window.",
            )
            .await;
        }
        Err(_) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "ReqFlux was not authorized. You can close this window.",
            )
            .await;
        }
    }
    Some(outcome.map_err(HttpError::Auth))
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        if head.len() > MAX_HEAD {
            return None;
        }
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            break;
        }
        head.extend(buffer.get(..read)?);
    }
    String::from_utf8(head).ok()
}

async fn respond(stream: &mut TcpStream, status: &str, text: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    if let Err(error) = stream.write_all(response.as_bytes()).await {
        log::debug!("the loopback redirect answer was not written: {error}");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::open_with;

    const WINDOW: Duration = Duration::from_secs(2);

    #[test]
    fn launchers_that_all_exit_with_failure_report_it() {
        let error = open_with(vec![Command::new("false"), Command::new("false")], WINDOW)
            .expect_err("no launcher started");
        assert!(error.to_string().contains("exited with"), "{error}");
    }

    #[test]
    fn a_later_launcher_opens_after_an_earlier_one_fails() {
        assert!(open_with(vec![Command::new("false"), Command::new("true")], WINDOW).is_ok());
    }

    #[test]
    fn a_launcher_that_cannot_spawn_gives_way_to_the_next() {
        let missing = Command::new("reqflux-no-such-launcher");
        assert!(open_with(vec![missing, Command::new("true")], WINDOW).is_ok());
    }

    #[test]
    fn a_launcher_still_running_after_the_window_counts_as_started() {
        let mut slow = Command::new("sleep");
        slow.arg("5");
        assert!(open_with(vec![slow], Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn no_launcher_at_all_is_an_error() {
        assert!(open_with(Vec::new(), WINDOW).is_err());
    }
}
