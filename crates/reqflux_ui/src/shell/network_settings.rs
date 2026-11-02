//! App > Network on the settings page (v1-features S5, design/network/01-settings-sections):
//! what `[network]` of `config.toml` holds, read-only, and the keyring entries of its proxy
//! password and PKCS#12 passphrases, which Set, Change and Clear edit.
//!
//! The window applies `[network]` to every engine it opens ([`Shell::connect_workspace`]):
//! `Engine::apply_network` with the warnings in the Problems panel, and `Engine::persist_cookies`
//! on the app state, so the cookie jar outlives the window. Reload reads the file again and
//! applies it; a keyring change applies it too, so the next send uses the new secret.
//!
//! Each entry's warnings and whether it was skipped come from the engine's own
//! [`network_settings`] run over that entry alone, so the page says what sends do. Whether a
//! secret is set comes from `SecretStore::get_account(..).is_some()`; no value reaches the
//! page, and the dialog's field starts empty.

use std::sync::Arc;

use bolt::controls::InputState;
use bolt::modals::ModalFocus;
use gpui::{App, AppContext as _, Context, Entity, Focusable, Window};
use reqflux_core::model::Id;
use reqflux_engine::config::{
    self, CertificateConfig, ConfigError, NetworkConfig, ProxyConfig, ProxyMode,
    WorkspaceNetworkConfig,
};
use reqflux_engine::{MASK, NetworkSecret, SecretStore, network_settings};

use super::{AppSection, Section};
use crate::shell::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

#[path = "network_settings/view.rs"]
pub(crate) mod view;

/// Whether a secret is in the keyring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keyring {
    Set,
    NotSet,
    /// The keyring could not be asked.
    Unavailable,
}

/// A secret of the page: its keyring account and whether it is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecretRow {
    pub(crate) secret: NetworkSecret,
    pub(crate) state: Keyring,
}

/// `ca_bundle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaRow {
    pub(crate) path: String,
    /// Not used by sends; `warnings` say why.
    pub(crate) skipped: bool,
    pub(crate) warnings: Vec<String>,
}

/// What a certificate entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CertificateKind {
    Pem,
    Pkcs12,
    /// Not used by sends; the entry's warnings say why.
    Skipped,
}

/// One `[[network.certificates]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CertificateRow {
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) kind: CertificateKind,
    /// The files it names, as written.
    pub(crate) paths: Vec<String>,
    /// The passphrase of a PKCS#12 entry that applies.
    pub(crate) passphrase: Option<SecretRow>,
    pub(crate) warnings: Vec<String>,
}

/// A proxy table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxyRow {
    pub(crate) mode: ProxyMode,
    /// The URL with any password in it replaced by the mask.
    pub(crate) url: Option<String>,
    pub(crate) username: Option<String>,
    /// The password of a manual proxy.
    pub(crate) password: Option<SecretRow>,
    pub(crate) bypass: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

/// The open workspace and its own proxy, if `[network.workspaces."<id>"]` has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceProxy {
    pub(crate) name: String,
    pub(crate) id: String,
    pub(crate) own: Option<ProxyRow>,
}

/// What the page shows of `[network]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkModel {
    pub(crate) ca_bundle: Option<CaRow>,
    pub(crate) certificates: Vec<CertificateRow>,
    pub(crate) proxy: ProxyRow,
    pub(crate) workspace: Option<WorkspaceProxy>,
}

impl NetworkModel {
    /// How many entries were skipped or carry a warning.
    pub(crate) fn warning_count(&self) -> usize {
        let proxies = std::iter::once(&self.proxy).chain(
            self.workspace
                .as_ref()
                .and_then(|workspace| workspace.own.as_ref()),
        );
        self.ca_bundle
            .iter()
            .map(|row| row.warnings.len())
            .chain(self.certificates.iter().map(|row| row.warnings.len()))
            .chain(proxies.map(|row| row.warnings.len()))
            .sum()
    }
}

/// `url` with the password of its userinfo, if any, replaced by [`MASK`].
pub(crate) fn masked_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let Some((authority, tail)) = rest.split_at_checked(end) else {
        return url.to_owned();
    };
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    let user = match userinfo.split_once(':') {
        Some((user, _)) => format!("{user}:{MASK}"),
        None => userinfo.to_owned(),
    };
    format!("{scheme}://{user}@{host}{tail}")
}

/// Whether `secret` is in `store`; the value is dropped at once.
fn keyring(store: &dyn SecretStore, secret: &NetworkSecret) -> Keyring {
    match store
        .get_account(&secret.account())
        .map(|value| value.is_some())
    {
        Ok(true) => Keyring::Set,
        Ok(false) => Keyring::NotSet,
        Err(_) => Keyring::Unavailable,
    }
}

fn proxy_row(
    proxy: &ProxyConfig,
    workspace: Option<Id>,
    warnings: Vec<String>,
    store: &dyn SecretStore,
) -> ProxyRow {
    let manual = proxy.mode == ProxyMode::Manual;
    let username = proxy.username.clone().filter(|name| !name.is_empty());
    let password = (manual && username.is_some()).then(|| {
        let secret = NetworkSecret::ProxyPassword { workspace };
        SecretRow {
            state: keyring(store, &secret),
            secret,
        }
    });
    ProxyRow {
        mode: proxy.mode,
        url: proxy
            .url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .map(|url| masked_url(url.trim())),
        username,
        password,
        bypass: proxy.bypass.clone(),
        warnings,
    }
}

fn certificate_row(
    entry: &CertificateConfig,
    workspace: Id,
    store: &dyn SecretStore,
) -> CertificateRow {
    let alone = NetworkConfig {
        certificates: vec![entry.clone()],
        ..NetworkConfig::default()
    };
    let (settings, warnings) = network_settings(&alone, workspace, store);
    let skipped = settings.certificates.is_empty();
    let kind = match (skipped, &entry.pkcs12) {
        (true, _) => CertificateKind::Skipped,
        (false, Some(_)) => CertificateKind::Pkcs12,
        (false, None) => CertificateKind::Pem,
    };
    let paths = [&entry.cert, &entry.key, &entry.pkcs12]
        .into_iter()
        .flatten()
        .map(|path| path.display().to_string())
        .collect();
    let passphrase = (kind == CertificateKind::Pkcs12).then(|| {
        let secret = NetworkSecret::CertificatePassphrase {
            host: entry.host.trim().to_owned(),
            port: entry.port,
        };
        SecretRow {
            state: keyring(store, &secret),
            secret,
        }
    });
    CertificateRow {
        host: entry.host.trim().to_owned(),
        port: entry.port,
        kind,
        paths,
        passphrase,
        warnings,
    }
}

/// What the page shows of `network` for `workspace` (its id and name), every warning the
/// engine's own reading of each entry gives.
pub(crate) fn model(
    network: &NetworkConfig,
    workspace: Option<(Id, &str)>,
    store: &dyn SecretStore,
) -> NetworkModel {
    let id = workspace.map_or_else(Id::new, |(id, _)| id);
    let ca_bundle = network.ca_bundle.as_ref().map(|path| {
        let alone = NetworkConfig {
            ca_bundle: Some(path.clone()),
            ..NetworkConfig::default()
        };
        let (settings, warnings) = network_settings(&alone, id, store);
        CaRow {
            path: path.display().to_string(),
            skipped: settings.ca_bundle.is_none(),
            warnings,
        }
    });
    let certificates = network
        .certificates
        .iter()
        .map(|entry| certificate_row(entry, id, store))
        .collect();
    let global = NetworkConfig {
        proxy: network.proxy.clone(),
        ..NetworkConfig::default()
    };
    let (_, global_warnings) = network_settings(&global, id, store);
    let proxy = proxy_row(&network.proxy, None, global_warnings, store);
    let workspace = workspace.map(|(id, name)| {
        let own = network.workspaces.iter().find_map(|(key, entry)| {
            let matches = key.parse::<Id>().is_ok_and(|key| key == id);
            matches
                .then_some(entry.proxy.as_ref())
                .flatten()
                .map(|proxy| (key.clone(), proxy))
        });
        let own = own.map(|(key, proxy)| {
            let alone = NetworkConfig {
                workspaces: [(
                    key,
                    WorkspaceNetworkConfig {
                        proxy: Some(proxy.clone()),
                    },
                )]
                .into(),
                ..NetworkConfig::default()
            };
            let (_, warnings) = network_settings(&alone, id, store);
            proxy_row(proxy, Some(id), warnings, store)
        });
        WorkspaceProxy {
            name: name.to_owned(),
            id: id.to_string(),
            own,
        }
    });
    NetworkModel {
        ca_bundle,
        certificates,
        proxy,
        workspace,
    }
}

/// The open Set or Change dialog of a secret.
pub(crate) struct SecretDialog {
    pub(crate) secret: NetworkSecret,
    /// `Passphrase for vault.acme.com:8200` or `Proxy password`.
    pub(crate) title: String,
    /// What it unlocks and where it is kept.
    pub(crate) detail: String,
    /// The masked field; it starts empty even when a value exists.
    pub(crate) input: Entity<InputState>,
}

/// The section's state.
pub(crate) struct NetworkState {
    /// What the page shows, read when `[network]` was last applied.
    pub(crate) model: Option<NetworkModel>,
    /// Why `config.toml` was not used when `[network]` was last applied: sends use the
    /// defaults.
    pub(crate) config_error: Option<String>,
    /// The warnings the last apply added to the Problems panel.
    applied: Vec<String>,
    /// The keyring without a workspace (the app's, or an in-memory one in tests).
    secrets: Option<Arc<dyn SecretStore>>,
    pub(crate) dialog: Option<SecretDialog>,
    pub(crate) focus: ModalFocus,
}

impl NetworkState {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            model: None,
            config_error: None,
            applied: Vec::new(),
            secrets: None,
            dialog: None,
            focus: ModalFocus::new(cx),
        }
    }
}

impl Shell {
    /// Applies `[network]` to the engine of the workspace that just opened and keeps its
    /// cookie jar in the app state. The shell calls it whenever its engine or config file
    /// changes.
    pub(crate) fn connect_workspace(&mut self) {
        if let (OpenedWorkspace::Loaded(engine), Some(state)) = (&self.workspace, &self.state)
            && let Err(error) = engine.persist_cookies(state.path())
        {
            log::warn!("the cookie jar stays in memory: {error}");
        }
        self.apply_network_config();
    }

    /// Reads `[network]` from `config.toml`, applies it to the open engine, puts its warnings
    /// in the Problems panel in place of the previous ones, and refreshes the page.
    pub(crate) fn apply_network_config(&mut self) {
        let loaded = match self.config_path.as_deref() {
            Some(path) => config::load(path),
            None => Err(ConfigError::NoConfigDirectory),
        };
        let (network, error) = match loaded {
            Ok(config) => (config.network, None),
            Err(ConfigError::NoConfigDirectory) => (NetworkConfig::default(), None),
            Err(error) => {
                let detail = match &error {
                    ConfigError::Parse { path, location } => {
                        format!("{}, {location}.", path.display())
                    }
                    other => format!("{other}."),
                };
                (NetworkConfig::default(), Some(detail))
            }
        };
        let warnings = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => engine.apply_network(&network),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
        };
        let previous = std::mem::take(&mut self.settings.network.applied);
        self.config_report
            .warnings
            .retain(|warning| !previous.contains(warning));
        self.config_report.warnings.extend(warnings.iter().cloned());
        self.settings.network.applied = warnings;
        let store = self.network_secrets();
        let workspace = match &self.workspace {
            OpenedWorkspace::Loaded(engine) => {
                let manifest = &engine.workspace().manifest;
                Some((manifest.id, manifest.name.clone()))
            }
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
        };
        let workspace = workspace.as_ref().map(|(id, name)| (*id, name.as_str()));
        self.settings.network.model = Some(model(&network, workspace, store.as_ref()));
        self.settings.network.config_error = error;
    }

    /// The store the page's secrets live in: the open engine's, else the app's.
    fn network_secrets(&mut self) -> Arc<dyn SecretStore> {
        match &self.workspace {
            OpenedWorkspace::Loaded(engine) => engine.secret_store().clone(),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => self
                .settings
                .network
                .secrets
                .get_or_insert_with(crate::workspace::secret_store)
                .clone(),
        }
    }

    /// The nav's Network: reads `config.toml` and shows the section.
    pub(crate) fn open_network_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_app_controls(window, cx);
        self.apply_network_config();
        self.open_settings(Section::App(AppSection::Network), cx);
        self.restore_focus(window, cx);
    }

    /// Set… or Change…: opens the dialog of `secret` with an empty masked field.
    pub(crate) fn open_network_secret(
        &mut self,
        secret: NetworkSecret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title, detail) = secret_text(&secret);
        let input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let field = input.read(cx).focus_handle(cx);
        self.settings.network.focus.focus(Some(&field), window, cx);
        self.settings.network.dialog = Some(SecretDialog {
            secret,
            title,
            detail,
            input,
        });
        cx.notify();
    }

    /// Cancel, Escape or the backdrop: closes the dialog and writes nothing.
    pub(crate) fn close_network_secret(&mut self, cx: &mut Context<Self>) {
        if self.settings.network.dialog.take().is_some() {
            self.settings.network.focus.restore(cx);
        }
        cx.notify();
    }

    /// Save to keyring: writes the typed value to the keyring, closes the dialog and applies
    /// `[network]` again. An empty field saves nothing.
    pub(crate) fn save_network_secret(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.settings.network.dialog.as_ref() else {
            return;
        };
        let value = dialog.input.read(cx).value().to_string();
        if value.is_empty() {
            return;
        }
        let account = dialog.secret.account();
        let store = self.network_secrets();
        match store.set_account(&account, &value) {
            Ok(()) => {
                self.settings.network.dialog = None;
                self.settings.network.focus.restore(cx);
                self.apply_network_config();
            }
            Err(error) => toasts::failure("Cannot save to the keyring", error.to_string(), cx),
        }
        cx.notify();
    }

    /// Clear: deletes the keyring entry of `secret` and applies `[network]` again.
    pub(crate) fn clear_network_secret(&mut self, secret: &NetworkSecret, cx: &mut Context<Self>) {
        let store = self.network_secrets();
        match store.delete_account(&secret.account()) {
            Ok(()) => self.apply_network_config(),
            Err(error) => toasts::failure("Cannot clear the keyring entry", error.to_string(), cx),
        }
        cx.notify();
    }
}

/// The dialog's title and detail for `secret`.
fn secret_text(secret: &NetworkSecret) -> (String, String) {
    let account = secret.account();
    match secret {
        NetworkSecret::ProxyPassword { workspace: None } => (
            "Proxy password".to_owned(),
            format!(
                "The password of the app proxy. Stored in the OS keyring as {account}, never \
                 in config.toml."
            ),
        ),
        NetworkSecret::ProxyPassword { workspace: Some(_) } => (
            "Workspace proxy password".to_owned(),
            format!(
                "The password of this workspace's own proxy. Stored in the OS keyring as \
                 {account}, never in config.toml."
            ),
        ),
        NetworkSecret::CertificatePassphrase { host, port } => {
            let target = match port {
                Some(port) => format!("{host}:{port}"),
                None => host.clone(),
            };
            (
                format!("Passphrase for {target}"),
                format!(
                    "Unlocks the PKCS#12 file of {target}. Stored in the OS keyring as \
                     {account}, never in config.toml."
                ),
            )
        }
    }
}

#[cfg(test)]
#[path = "network_settings_tests.rs"]
mod tests;
