//! The configurable keymap (v1-features S7, design/layouts/01 D3): one registry
//! (`commands`) names every command and its default key, and `[keymap]` in the user's
//! config.toml rebinds any of them, in GPUI keystroke syntax (`ctrl-alt-k`).
//!
//! The window reads the table at startup ([`apply_startup`]) and after every edit
//! ([`apply_file`]), resolves it over the registry in registry order — the first command
//! that wants a chord keeps it, a later command whose chord is taken keeps its default,
//! and when that is taken too it has no key — and rebinds the app: the new chord runs the
//! command and the old one stops firing. An unknown slug, a chord that cannot be bound
//! and a chord two commands claim are warnings in the Problems panel that never block
//! startup. The App > Keymap section of the settings page (see `keymap/view`) shows every
//! command with its effective key, captures the next chord on a click through bolt's
//! `KeyCapture` (Escape cancels, Delete or Backspace returns the default), reports a conflict with the command that
//! holds it — Keep mine takes the chord away from the other command — and `Reset to
//! defaults` removes the whole table. Every change writes config.toml at once through the
//! engine's format-preserving editors.

use std::collections::BTreeMap;
use std::path::Path;

use std::rc::Rc;

use bolt::controls::{KeyCapture, KeyCaptureHandler};
use gpui::{
    App, AppContext as _, Context, Entity, Global, KeyBinding, Keystroke, Modifiers, Subscription,
    Unbind,
};
use reqflux_engine::config::{self, KeymapConfig};

use super::Shell;
use super::commands::{self, CommandId};

pub(super) mod view;

/// The hint the key capture draws while it listens.
const CAPTURE_HINT: &str = "listening\u{2026} press the new keys, Esc cancels";
/// The id of the key capture, which the UI automation tree and the tests find.
const CAPTURE_ID: &str = "keymap-capture-hint";
/// The keys that return the captured command to its default key.
const CLEAR_KEYS: [&str; 2] = ["delete", "backspace"];

/// The key every command answers after `[keymap]` is resolved: the chord, or `None` when
/// another command took its key. A command the resolution has not seen yet (before the
/// first apply) is absent.
#[derive(Default)]
struct Effective(BTreeMap<&'static str, Option<String>>);

impl Global for Effective {}

/// One command's key after `[keymap]` is resolved.
pub(crate) struct Resolved {
    /// The command.
    pub(crate) id: CommandId,
    /// The chord it answers now, `None` when another command took its key.
    pub(crate) key: Option<String>,
}

/// The warning `text` of the config file at `path`, as the Problems panel reads it.
fn warning(path: &Path, text: String) -> String {
    format!("{}: {text}", path.display())
}

/// The palette label of the command `slug`, for a warning about it.
fn label_of(slug: &str) -> &'static str {
    commands::COMMANDS
        .iter()
        .find(|command| command.name == slug)
        .map_or("", |command| command.label)
}

/// Resolves `overrides` (the file's `[keymap]` table) over the registry, in registry
/// order: the first command that wants a chord keeps it, a later command whose chord is
/// taken falls back to its default, and when that is taken too it has no key. Returns the
/// resolved commands and one warning per slug the registry does not know, per chord that
/// cannot be bound and per chord two commands claim. Nothing here blocks startup.
pub(crate) fn resolve(path: &Path, overrides: &KeymapConfig) -> (Vec<Resolved>, Vec<String>) {
    let mut warnings = Vec::new();
    for (slug, chord) in overrides.iter() {
        if commands::COMMANDS
            .iter()
            .all(|command| command.name != slug)
        {
            warnings.push(warning(
                path,
                format!("unknown command `{slug}` in [keymap]; it is ignored"),
            ));
        } else if gpui::Keystroke::parse(chord).is_err() {
            warnings.push(warning(
                path,
                format!(
                    "`{chord}` in [keymap] is not a key chord the window can bind, so \
                     `{slug}` keeps its default key"
                ),
            ));
        }
    }
    // The bindings that can be applied, by slug: known slugs with bindable chords.
    let bindable: BTreeMap<&str, &str> = overrides
        .iter()
        .filter(|(slug, chord)| {
            commands::COMMANDS
                .iter()
                .any(|command| command.name == *slug)
                && gpui::Keystroke::parse(chord).is_ok()
        })
        .collect();
    // A chord and the slug of the command that holds it, decided in registry order.
    let mut claimed: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut resolved = Vec::new();
    for command in commands::COMMANDS {
        let key = match bindable.get(command.name) {
            Some(chord) => match claimed.get(*chord) {
                None => Some((*chord).to_owned()),
                Some(holder) => match claimed.get(command.key) {
                    None => {
                        warnings.push(warning(
                            path,
                            format!(
                                "`{chord}` in [keymap] is already bound to {}, so `{}` \
                                 keeps its default key",
                                label_of(holder),
                                command.name
                            ),
                        ));
                        Some(command.key.to_owned())
                    }
                    Some(default_holder) => {
                        warnings.push(warning(
                            path,
                            format!(
                                "`{chord}` in [keymap] is already bound to {} and the \
                                 default `{}` is taken by {} too, so `{}` has no key until \
                                 the [keymap] table changes",
                                label_of(holder),
                                command.key,
                                label_of(default_holder),
                                command.name
                            ),
                        ));
                        None
                    }
                },
            },
            None => match claimed.get(command.key) {
                None => Some(command.key.to_owned()),
                Some(holder) => {
                    warnings.push(warning(
                        path,
                        format!(
                            "`{}` is already bound to {}, so `{}` has no key until the \
                             [keymap] table changes",
                            command.key,
                            label_of(holder),
                            command.name
                        ),
                    ));
                    None
                }
            },
        };
        if let Some(key) = &key {
            claimed.insert(key.clone(), command.name);
        }
        resolved.push(Resolved {
            id: command.id,
            key,
        });
    }
    (resolved, warnings)
}

/// Applies `resolved` over the app's bindings: rebinds every command whose effective key
/// changed, unbinding the chord it answered before, and remembers the resolution.
fn apply(resolved: &[Resolved], cx: &mut App) {
    let previous = cx
        .try_global::<Effective>()
        .map(|effective| effective.0.clone())
        .unwrap_or_default();
    let mut next = Effective::default();
    for entry in resolved {
        let Some(command) = commands::command(entry.id) else {
            continue;
        };
        // Before the first apply the defaults stand, bound by `bind_commands`.
        let was = previous
            .get(command.name)
            .cloned()
            .unwrap_or_else(|| Some(command.key.to_owned()));
        if was != entry.key {
            if let Some(old) = was
                && let Some(name) = commands::action_name(entry.id)
            {
                // The old chord stops firing: the keymap drops the action from it.
                cx.bind_keys([KeyBinding::new(
                    old.as_str(),
                    Unbind(name.into()),
                    Some(command.context),
                )]);
            }
            if let Some(new) = &entry.key
                && let Some(binding) = commands::binding(entry.id, new)
            {
                cx.bind_keys([binding]);
            }
        }
        next.0.insert(command.name, entry.key.clone());
    }
    cx.set_global(next);
}

/// Applies the `[keymap]` table of the config file at `path` over the default bindings
/// and returns its warnings (unknown slugs, chords that cannot be bound, chords two
/// commands claim) for the Problems panel. A file that does not parse changes nothing:
/// the window runs on the defaults and says so.
pub(crate) fn apply_file(path: &Path, cx: &mut App) -> Vec<String> {
    let loaded = match config::load(path) {
        Ok(loaded) => loaded,
        Err(error) => {
            log::warn!("{error}; the default keys stay");
            return Vec::new();
        }
    };
    let (resolved, warnings) = resolve(path, &loaded.keymap);
    apply(&resolved, cx);
    warnings
}

/// Reads `[keymap]` at startup, before the window opens: applies it over the bindings
/// [`super::bind_keys`] installed. Returns the table's warnings for the Problems panel. Without
/// a config path nothing is read and the defaults stand.
pub(crate) fn apply_startup(path: Option<&Path>, cx: &mut App) -> Vec<String> {
    match path {
        Some(path) => apply_file(path, cx),
        None => Vec::new(),
    }
}

/// The key the resolution gave `slug`: `None` before the first apply, and a key of `None`
/// inside when another command took its key. [`commands::effective_key`] turns this into
/// the key the palette and menus show.
pub(crate) fn resolved_key(cx: &App, slug: &str) -> Option<Option<String>> {
    cx.try_global::<Effective>()
        .and_then(|effective| effective.0.get(slug).cloned())
}

/// The command that holds `chord` now, other than `id`, as the resolution left the
/// keymap; `None` when the chord is free.
pub(crate) fn holder_of(chord: &str, id: CommandId, cx: &App) -> Option<CommandId> {
    commands::COMMANDS
        .iter()
        .find(|command| command.id != id && commands::effective_key(cx, command.id) == chord)
        .map(|command| command.id)
}

/// The chord a capture caught while another command holds it, and the two commands: Keep
/// mine rebinds anyway and takes the chord away from the other, Cancel changes nothing.
pub(crate) struct Conflict {
    /// The command being rebound.
    pub(crate) id: CommandId,
    /// The command that holds the chord.
    pub(crate) other: CommandId,
    /// The chord, as captured.
    pub(crate) chord: String,
}

/// The state of the Keymap section of the app settings.
#[derive(Default)]
pub(crate) struct KeymapState {
    /// The command listening for its new chord, while a capture runs.
    pub(crate) capture: Option<CommandId>,
    /// The conflict the last capture caught, while its banner shows.
    pub(crate) conflict: Option<Conflict>,
    /// bolt's key capture the capturing row draws, made on the first capture.
    pub(crate) field: Option<Entity<KeyCapture>>,
    /// Ends the page's capture when the field stops on its own (a bare Escape).
    watch: Option<Subscription>,
}

/// Whether `keystroke` is a bare printable key: typing, not a chord.
fn is_typing(keystroke: &Keystroke) -> bool {
    keystroke.modifiers == Modifiers::none() && keystroke.key.chars().count() == 1
}

impl Shell {
    /// A click on the key cell of `id` (App > Keymap): listens for the next chord, which
    /// rebinds the command and writes config.toml at once. Nothing happens while the
    /// config file cannot be written.
    pub(crate) fn start_keymap_capture(
        &mut self,
        id: CommandId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.app.editable() {
            return;
        }
        let field = self.keymap_field(window, cx);
        self.settings.keymap.capture = Some(id);
        self.settings.keymap.conflict = None;
        listen(&field, id, cx.entity().downgrade(), cx);
        cx.notify();
    }

    /// bolt's key capture of the page, made on the first capture with the watch that
    /// ends the page's capture when a bare Escape stops the field.
    fn keymap_field(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Entity<KeyCapture> {
        if let Some(field) = &self.settings.keymap.field {
            return field.clone();
        }
        let field = cx.new(|_| KeyCapture::new(CAPTURE_ID, ""));
        let watch = cx.observe_in(&field, window, |this, field, window, cx| {
            if this.settings.keymap.capture.is_some() && !field.read(cx).is_capturing() {
                this.cancel_keymap_capture(window, cx);
            }
        });
        self.settings.keymap.field = Some(field.clone());
        self.settings.keymap.watch = Some(watch);
        field
    }

    /// The next chord of the capture of `id`, from bolt's key capture (which cancels a
    /// bare Escape itself): Delete or Backspace returns the default, and any other chord rebinds the
    /// command — with the conflict banner first when another command holds the chord.
    pub(crate) fn captured_keymap_chord(
        &mut self,
        id: CommandId,
        chord: String,
        cx: &mut Context<Self>,
    ) {
        if self.settings.keymap.capture != Some(id) {
            return;
        }
        self.end_keymap_capture(cx);
        if CLEAR_KEYS.contains(&chord.as_str()) {
            self.clear_keymap_binding(id, cx);
            return;
        }
        if let Some(other) = holder_of(&chord, id, cx) {
            self.settings.keymap.conflict = Some(Conflict { id, other, chord });
            cx.notify();
            return;
        }
        self.rebind_keymap(id, &chord, cx);
    }

    /// Keep mine (the conflict banner): rebinds anyway and the command that held the
    /// chord loses it, since the file only carries this binding.
    pub(crate) fn keep_keymap_binding(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.settings.keymap.conflict.take() else {
            return;
        };
        self.rebind_keymap(conflict.id, &conflict.chord, cx);
    }

    /// Cancel (the conflict banner, or Escape): changes nothing.
    pub(crate) fn cancel_keymap_capture(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.keymap.conflict = None;
        self.end_keymap_capture(cx);
        self.restore_focus(window, cx);
        cx.notify();
    }

    /// The Clear button of `id` (or Delete during its capture): returns the command to
    /// its default key by removing its `[keymap]` entry.
    pub(crate) fn clear_keymap_binding(&mut self, id: CommandId, cx: &mut Context<Self>) {
        self.end_keymap_capture(cx);
        let Some(slug) = commands::command(id).map(|command| command.name) else {
            return;
        };
        if self.edit_app_config(
            |path| config::remove_keymap_binding(path, slug).map(|_| ()),
            cx,
        ) {
            self.refresh_keymap_bindings(cx);
        }
    }

    /// Reset to defaults: removes the whole `[keymap]` table, so every command answers
    /// its default key again, and any capture ends.
    pub(crate) fn reset_keymap(&mut self, cx: &mut Context<Self>) {
        self.end_keymap_capture(cx);
        if self.edit_app_config(|path| config::clear_keymap(path).map(|_| ()), cx) {
            self.refresh_keymap_bindings(cx);
        }
    }

    /// Ends the capture: the key capture stops listening.
    fn end_keymap_capture(&mut self, cx: &mut Context<Self>) {
        self.settings.keymap.capture = None;
        if let Some(field) = &self.settings.keymap.field {
            field.update(cx, |field, cx| field.stop(cx));
        }
    }

    /// Rebinds `id` to `chord`: writes config.toml at once and reapplies the table.
    fn rebind_keymap(&mut self, id: CommandId, chord: &str, cx: &mut Context<Self>) {
        let Some(slug) = commands::command(id).map(|command| command.name) else {
            return;
        };
        if self.edit_app_config(|path| config::set_keymap_binding(path, slug, chord), cx) {
            self.refresh_keymap_bindings(cx);
        }
    }

    /// Reapplies the `[keymap]` table of the config file after it changed: the new chords
    /// answer and the old ones stop, at once.
    fn refresh_keymap_bindings(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.config_path.clone() {
            apply_file(&path, cx);
        }
        cx.notify();
    }
}

/// Opens `field` for the capture of `id`: the next chord goes back to `shell`, and a bare printable key, which is typing rather than a chord, keeps it listening.
fn listen(field: &Entity<KeyCapture>, id: CommandId, shell: gpui::WeakEntity<Shell>, cx: &mut App) {
    let target = field.downgrade();
    let handler: KeyCaptureHandler = Rc::new(move |keystroke, chord, cx| {
        if is_typing(keystroke) {
            if let Some(field) = target.upgrade() {
                listen(&field, id, shell.clone(), cx);
            }
            return;
        }
        let outcome = shell.update(cx, |shell, cx| {
            shell.captured_keymap_chord(id, chord.to_string(), cx);
        });
        if let Err(error) = outcome {
            log::debug!("the shell of a keymap capture is gone: {error}");
        }
    });
    field.update(cx, |field, cx| field.start(CAPTURE_HINT, handler, cx));
}
