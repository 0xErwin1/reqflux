//! App > Network (v1-features S5, design/network/01-settings-sections): `[network]` is
//! read-only in the window and shows what the engine's own reading of each entry gives, a
//! proxy URL never shows its password, and Set, Change and Clear edit only the keyring. Every
//! test keeps its config file under a temp `XDG_CONFIG_HOME` layout.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::config;
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::variables_tests::variables_workspace;
use crate::theme::ConfigReport;

fn config_home() -> (TempDir, PathBuf) {
    let dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = match config::config_path_in(Some(dir.path()), None) {
        Some(path) => path,
        None => panic!("no config path"),
    };
    (dir, path)
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create dir: {error}");
    }
    if let Err(error) = fs::write(path, text) {
        panic!("write: {error}");
    }
}

fn read(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("read: {error}"),
    }
}

/// Renders the shell over `config_path` and opens App > Network.
fn render_network<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
    config_path: &Path,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = render(cx, dir, state_path);
    let path = config_path.to_owned();
    shell.update(cx, |shell, _| {
        shell.set_config(Some(path), ConfigReport::default());
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.open_network_settings(window, cx));
    });
    cx.run_until_parked();
    (shell, cx)
}

#[test]
fn a_proxy_url_never_shows_its_password() {
    let cases = [
        (
            "http://me:hunter2@proxy.test:3128",
            "http://me:••••••••@proxy.test:3128",
        ),
        ("socks5://proxy.test:1080", "socks5://proxy.test:1080"),
        ("http://me@proxy.test", "http://me@proxy.test"),
        ("not a url", "not a url"),
    ];
    for (url, expected) in cases {
        assert_eq!(
            crate::shell::settings_page::network_settings::masked_url(url),
            expected,
            "{url}"
        );
    }
}

#[gpui::test]
fn the_section_lists_what_network_holds(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let text = "[network]\nca_bundle = \"/etc/ssl/corp-ca.pem\"\n\n\
                [network.proxy]\nmode = \"manual\"\nurl = \"http://me:hunter2@proxy.test:3128\"\n\
                bypass = [\"localhost\"]\n";
    write(&config_path, text);
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    present(cx, "network-ca-bundle");
    present(cx, "network-proxy-url");
    let model = shell.read_with(cx, |shell, _| {
        shell.settings.network.model.clone().expect("a model")
    });
    assert_eq!(
        model.ca_bundle.as_ref().map(|row| row.path.clone()),
        Some("/etc/ssl/corp-ca.pem".to_owned())
    );
    assert_eq!(
        model.proxy.url.as_deref(),
        Some("http://me:••••••••@proxy.test:3128"),
        "the page never shows the password of the URL"
    );
    assert_eq!(model.proxy.bypass, vec!["localhost".to_owned()]);
    assert_eq!(read(&config_path), text, "the section never writes");
}

#[gpui::test]
fn a_skipped_entry_shows_the_warning_of_its_own_reading(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    write(
        &config_path,
        "[network]\nca_bundle = \"relative/ca.pem\"\n\n[[network.certificates]]\n\
         host = \"vault.acme.com\"\ncert = \"/c/client.pem\"\n",
    );
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    present(cx, "network-certificates");
    let model = shell.read_with(cx, |shell, _| {
        shell.settings.network.model.clone().expect("a model")
    });
    let ca = model.ca_bundle.clone().expect("a CA row");
    assert!(ca.skipped, "a relative path is skipped");
    assert!(!ca.warnings.is_empty(), "and it says why");
    assert_eq!(model.certificates.len(), 1);
    assert!(
        !model.certificates[0].warnings.is_empty(),
        "a certificate without a key is skipped with its warning"
    );
    assert!(model.warning_count() >= 2);
}

#[gpui::test]
fn set_and_clear_write_only_the_keyring(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let text = "[network.proxy]\nmode = \"manual\"\nurl = \"http://proxy.test:3128\"\n\
                username = \"me\"\n";
    write(&config_path, text);
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    let before = shell.read_with(cx, |shell, _| {
        shell
            .settings
            .network
            .model
            .clone()
            .expect("a model")
            .proxy
            .password
            .clone()
    });
    assert!(
        matches!(
            before.map(|row| row.state),
            Some(crate::shell::settings_page::network_settings::Keyring::NotSet)
        ),
        "a manual proxy with a username asks for its password"
    );

    click(cx, "network-proxy-password-set");
    present(cx, "network-secret-dialog");
    click(cx, "network-secret-input");
    cx.simulate_input("hunter2");
    click(cx, "network-secret-save");
    let after = shell.read_with(cx, |shell, _| {
        shell
            .settings
            .network
            .model
            .clone()
            .expect("a model")
            .proxy
            .password
            .clone()
    });
    assert!(matches!(
        after.map(|row| row.state),
        Some(crate::shell::settings_page::network_settings::Keyring::Set)
    ));
    assert_eq!(
        read(&config_path),
        text,
        "the keyring write never touches config.toml"
    );
}

#[gpui::test]
fn a_file_that_does_not_parse_says_so_and_sends_with_the_defaults(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    write(&config_path, "[network\nca_bundle = 1\n");
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    present(cx, "network-config-error");
    let error = shell.read_with(cx, |shell, _| shell.settings.network.config_error.clone());
    let error = error.expect("the error is reported");
    assert!(error.contains("config.toml"), "{error}");
    let model = shell.read_with(cx, |shell, _| shell.settings.network.model.clone());
    assert!(
        model.expect("a model").ca_bundle.is_none(),
        "the defaults apply"
    );
}
