//! App > Network (v1-features S5, design/network/01-settings-sections): `[network]` is
//! read-only in the window and shows what the engine's own reading of each entry gives, a
//! proxy URL never shows its password, and Set, Change and Clear edit only the keyring. Every
//! test keeps its config file under a temp `XDG_CONFIG_HOME` layout.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::config;
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::toasts::tests as toasts;
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

/// The proxy mode and URL the page shows.
fn shown_proxy(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
) -> (config::ProxyMode, Option<String>) {
    shell.read_with(cx, |shell, _| {
        let proxy = shell.settings.network.model.clone().expect("a model").proxy;
        (proxy.mode, proxy.url)
    })
}

#[gpui::test]
fn the_proxy_mode_control_writes_config_toml_and_never_raises_a_toast(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let mine = "# Mine.\ntheme = \"dark\"\n\n[network.proxy]\nmode = \"system\" # VPN\n";
    write(&config_path, mine);
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);

    click(cx, "network-proxy-mode-none");
    assert_eq!(read(&config_path), mine.replace("\"system\"", "\"none\""));
    assert_eq!(shown_proxy(cx, &shell).0, config::ProxyMode::None);
    present(cx, "network-proxy-mode");

    click(cx, "network-proxy-mode-manual");
    assert_eq!(
        config::load(&config_path)
            .expect("parses")
            .network
            .proxy
            .mode,
        config::ProxyMode::Manual
    );
    assert_eq!(shown_proxy(cx, &shell).0, config::ProxyMode::Manual);

    click(cx, "network-proxy-mode-system");
    assert_eq!(read(&config_path), mine);
    assert_eq!(
        toasts::pushed(cx),
        vec![],
        "choosing a mode never answers with a toast"
    );
}

#[gpui::test]
fn the_proxy_mode_is_written_into_a_file_that_does_not_exist_yet(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);

    click(cx, "network-proxy-mode-none");
    assert_eq!(read(&config_path), "[network.proxy]\nmode = \"none\"\n");
    assert_eq!(shown_proxy(cx, &shell).0, config::ProxyMode::None);
}

#[gpui::test]
fn manual_asks_for_the_url_and_writes_it_on_enter_and_on_blur(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    assert!(
        bounds(cx, "network-proxy-url").is_none(),
        "System has no URL"
    );

    click(cx, "network-proxy-mode-manual");
    click(cx, "network-proxy-url");
    cx.simulate_input("http://proxy.test:3128");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        read(&config_path),
        "[network.proxy]\nmode = \"manual\"\nurl = \"http://proxy.test:3128\"\n"
    );
    assert_eq!(
        shown_proxy(cx, &shell).1.as_deref(),
        Some("http://proxy.test:3128")
    );

    click(cx, "network-proxy-url");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("socks5://proxy.test:1080");
    let input = shell.read_with(cx, |shell, _| shell.settings.network.proxy_url.clone());
    let Some(input) = input else {
        panic!("the URL field is missing");
    };
    cx.update(|_, cx| {
        input.update(cx, |_, cx| cx.emit(gpui_component::input::InputEvent::Blur));
    });
    cx.run_until_parked();
    assert_eq!(
        config::load(&config_path)
            .expect("parses")
            .network
            .proxy
            .url
            .as_deref(),
        Some("socks5://proxy.test:1080"),
        "leaving the field commits it"
    );
    assert_eq!(toasts::pushed(cx), vec![]);
}

#[gpui::test]
fn a_url_with_a_password_is_shown_masked_and_left_alone_unless_replaced(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let text = "[network.proxy]\nmode = \"manual\"\nurl = \"http://me:hunter2@proxy.test:3128\"\n";
    write(&config_path, text);
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);

    click(cx, "network-proxy-url");
    click(cx, "settings-heading");
    assert_eq!(
        read(&config_path),
        text,
        "an untouched field writes nothing"
    );
    let shown = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .network
            .proxy_url
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
    });
    assert_eq!(shown.as_deref(), Some("http://me:••••••••@proxy.test:3128"));
    assert!(!read(&config_path).contains(reqflux_engine::MASK));
}

#[gpui::test]
fn the_proxy_controls_do_nothing_while_the_file_does_not_parse(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let broken = "[network\nca_bundle = 1\n";
    write(&config_path, broken);
    let (_shell, cx) = render_network(cx, &dir, &state_path, &config_path);

    click(cx, "network-proxy-mode-none");
    assert_eq!(read(&config_path), broken, "nothing overwrites the file");
    assert_eq!(
        toasts::pushed(cx),
        vec![],
        "and nothing answers with an error toast"
    );
}

const PASSWORD: &str = "secretpw";

#[gpui::test]
fn a_proxy_url_typed_with_a_password_keeps_it_out_of_config_toml(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);

    click(cx, "network-proxy-mode-manual");
    click(cx, "network-proxy-url");
    cx.simulate_input("http://u:secretpw@127.0.0.1:3999");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let text = read(&config_path);
    assert!(!text.contains(PASSWORD), "{text}");
    assert!(text.contains("url = \"http://u@127.0.0.1:3999\""), "{text}");
    let stored = shell.update(cx, |shell, _| {
        shell
            .network_secrets()
            .get_account("network/proxy")
            .ok()
            .flatten()
    });
    assert_eq!(stored.as_deref(), Some(PASSWORD));
    assert_eq!(
        shown_proxy(cx, &shell).1.as_deref(),
        Some("http://u:••••••••@127.0.0.1:3999"),
        "the field shows the password masked"
    );
    assert_eq!(toasts::pushed(cx), vec![], "no toast, so none carries it");
}

#[gpui::test]
fn a_keyring_failure_writes_nothing_and_the_toast_has_no_password(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    click(cx, "network-proxy-mode-manual");
    let before = read(&config_path);
    shell.update(cx, |shell, _| {
        // No open engine, so the page's own store is the one asked.
        shell.workspace = crate::workspace::OpenedWorkspace::None;
        shell.settings.network.secrets = Some(Arc::new(
            reqflux_engine::MemorySecretStore::unavailable("no keyring"),
        ));
    });

    click(cx, "network-proxy-url");
    cx.simulate_input("http://u:secretpw@127.0.0.1:3999");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(read(&config_path), before);
    let pushed = format!("{:?}", toasts::pushed(cx));
    assert!(
        pushed.contains("Cannot save the proxy password"),
        "{pushed}"
    );
    assert!(!pushed.contains("Cannot write config.toml"), "{pushed}");
    assert!(!pushed.contains(PASSWORD), "{pushed}");
}

#[gpui::test]
fn a_refused_url_stays_in_the_field_without_its_password(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    click(cx, "network-proxy-mode-manual");
    shell.update(cx, |shell, _| {
        shell.workspace = crate::workspace::OpenedWorkspace::None;
        shell.settings.network.secrets = Some(Arc::new(
            reqflux_engine::MemorySecretStore::unavailable("no keyring"),
        ));
    });

    click(cx, "network-proxy-url");
    cx.simulate_input("http://u:secretpw@127.0.0.1:3999");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        field_text(cx, &shell),
        "http://u@127.0.0.1:3999",
        "only the password has to be typed again"
    );
    assert!(!field_text(cx, &shell).contains(PASSWORD));
}

#[gpui::test]
fn the_bridge_does_not_expose_a_password_typed_in_the_url(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_config_dir, config_path) = config_home();
    let (_shell, cx) = render_network(cx, &dir, &state_path, &config_path);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    click(cx, "network-proxy-mode-manual");

    let exposed = |automation: &gpui_mcp::Automation| {
        let snapshot = automation.snapshot();
        format!("{:?}", snapshot.nodes["network-proxy-url"])
    };
    click(cx, "network-proxy-url");
    cx.simulate_input("http://u@127.0.0.1:3999");
    cx.run_until_parked();
    let dump = exposed(&automation);
    assert!(dump.contains("http://u@127.0.0.1:3999"), "{dump}");

    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("http://u:secretpw@127.0.0.1:3999");
    cx.run_until_parked();
    let dump = exposed(&automation);
    assert!(!dump.contains(PASSWORD), "{dump}");
    assert!(
        !dump.contains("127.0.0.1"),
        "the whole value is withheld: {dump}"
    );
}

/// The text of the URL field.
fn field_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .network
            .proxy_url
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}
