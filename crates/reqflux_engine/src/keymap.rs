//! The `[keymap]` table of the user's config file (v1-features S7): the commands the user
//! bound to another key than the registry's default.
//!
//! Only the table is parsed here: which slugs exist and whether a chord can be bound is
//! the front end's registry knowledge, so a slug this build does not know, a chord that
//! cannot be bound and a chord two commands claim are warnings the window reports without
//! changing what the rest of the file means (see the window's `shell/keymap.rs`). A value
//! that is not a string is ignored, so a hand-edited file never fails to load over it.
//!
//! [`crate::config::set_keymap_binding`], [`crate::config::remove_keymap_binding`] and
//! [`crate::config::clear_keymap`] edit the table with the same format-preserving editor
//! as `theme` and `layout`, so comments, key order and unknown keys survive; `Reset to
//! defaults` removes the whole table.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The `[keymap]` entries: the slug of a command and the chord it is reachable by, in
/// GPUI keystroke syntax (`ctrl-alt-k`), exactly as the file stores them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeymapConfig(BTreeMap<String, String>);

impl KeymapConfig {
    /// The chord `slug` is bound to, as written.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<&str> {
        self.0.get(slug).map(String::as_str)
    }

    /// Every binding, in slug order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(slug, chord)| (slug.as_str(), chord.as_str()))
    }

    /// Whether the file overrides no key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'de> Deserialize<'de> for KeymapConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = BTreeMap::<String, toml::Value>::deserialize(deserializer)?;
        Ok(Self(
            raw.into_iter()
                .filter_map(|(slug, chord)| {
                    // A chord that is not a string binds nothing; it is never worth the
                    // whole file.
                    chord.as_str().map(|chord| (slug, chord.to_owned()))
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use crate::config::{
        self, UserConfig, clear_keymap, remove_keymap_binding, set_keymap_binding,
    };

    fn temp_dir() -> tempfile::TempDir {
        match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(error) => panic!("temp dir: {error}"),
        }
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            panic!("create {}: {error}", parent.display());
        }
        if let Err(error) = fs::write(path, text) {
            panic!("write {}: {error}", path.display());
        }
    }

    fn read(path: &Path) -> String {
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => panic!("read {}: {error}", path.display()),
        }
    }

    fn loaded(path: &Path) -> UserConfig {
        match config::load(path) {
            Ok(config) => config,
            Err(error) => panic!("load: {error}"),
        }
    }

    #[test]
    fn a_keymap_table_parses_and_ignores_non_string_chords() {
        let dir = temp_dir();
        let path = dir.path().join("config.toml");
        write(
            &path,
            "theme = \"dark\"\n\n[keymap]\n\"save-request\" = \"ctrl-alt-s\"\nbroken = 3\n",
        );
        let config = loaded(&path);
        assert_eq!(config.keymap.get("save-request"), Some("ctrl-alt-s"));
        assert_eq!(config.keymap.get("broken"), None, "a number is no chord");
        assert_eq!(config.keymap.iter().count(), 1);
    }

    #[test]
    fn a_file_without_the_table_is_the_default() {
        let dir = temp_dir();
        let path = dir.path().join("config.toml");
        write(&path, "theme = \"dark\"\n");
        assert!(loaded(&path).keymap.is_empty());
        assert_eq!(
            loaded(&path),
            UserConfig {
                theme: Some(config::ThemeMode::Dark),
                ..UserConfig::default()
            }
        );
    }

    #[test]
    fn set_keymap_binding_creates_the_table_and_keeps_the_file() {
        let dir = temp_dir();
        let path = dir.path().join("reqflux/config.toml");
        write(
            &path,
            "# Mine.\ntheme = \"dark\" # set by the app\nzeta = 1\n",
        );

        set_keymap_binding(&path, "save-request", "ctrl-alt-s").expect("set");
        assert_eq!(
            read(&path),
            "# Mine.\ntheme = \"dark\" # set by the app\nzeta = 1\n\n[keymap]\nsave-request = \"ctrl-alt-s\"\n"
        );

        // Replacing a chord keeps the key and its spelling; a second entry sorts beside it.
        set_keymap_binding(&path, "save-request", "ctrl-shift-s").expect("set");
        set_keymap_binding(&path, "toggle-theme", "ctrl-alt-t").expect("set");
        assert_eq!(
            loaded(&path).keymap.get("save-request"),
            Some("ctrl-shift-s")
        );
        assert!(read(&path).contains("save-request = \"ctrl-shift-s\""));
        assert!(read(&path).contains("toggle-theme = \"ctrl-alt-t\""));
        assert!(read(&path).starts_with("# Mine.\n"), "the comment survives");
    }

    #[test]
    fn remove_keymap_binding_removes_the_key_and_the_empty_table() {
        let dir = temp_dir();
        let path = dir.path().join("config.toml");
        write(
            &path,
            "theme = \"dark\"\n\n[keymap]\n\"save-request\" = \"ctrl-alt-s\"\n",
        );

        assert!(
            !remove_keymap_binding(&path, "toggle-theme").expect("remove"),
            "nothing is removed twice"
        );
        assert!(remove_keymap_binding(&path, "save-request").expect("remove"));
        assert_eq!(read(&path), "theme = \"dark\"\n", "no empty table is kept");
        assert!(!remove_keymap_binding(&path, "save-request").expect("remove"));
    }

    #[test]
    fn clear_keymap_removes_the_whole_table() {
        let dir = temp_dir();
        let path = dir.path().join("config.toml");
        write(
            &path,
            "theme = \"dark\"\n\n[keymap]\n\"save-request\" = \"ctrl-alt-s\"\n\"toggle-theme\" = \"ctrl-alt-t\"\n",
        );

        assert!(clear_keymap(&path).expect("clear"));
        assert_eq!(read(&path), "theme = \"dark\"\n");
        assert!(!clear_keymap(&path).expect("clear"));
        assert!(loaded(&path).keymap.is_empty());
    }
}
