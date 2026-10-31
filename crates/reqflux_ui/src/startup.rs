//! The workspace the window opens with (standalone-workspaces S2): the requested directory,
//! else the most recent workspace that still loads, else a new managed workspace named
//! `Personal`. The current directory decides nothing.

use std::path::PathBuf;

use crate::workspace::OpenedWorkspace;

/// The name of the workspace a first start creates.
pub(crate) const PERSONAL: &str = "Personal";

/// Opens `requested` when given, even when it does not load, so the window shows why.
/// Otherwise opens the first of `recents` (newest first) that loads, else the directory
/// `create_personal` creates, else nothing, which shows the empty state.
pub(crate) fn startup_workspace(
    requested: Option<PathBuf>,
    recents: &[PathBuf],
    mut open: impl FnMut(PathBuf) -> OpenedWorkspace,
    create_personal: impl FnOnce() -> Option<PathBuf>,
) -> OpenedWorkspace {
    if let Some(path) = requested {
        return open(path);
    }
    for recent in recents {
        let opened = open(recent.clone());
        if matches!(opened, OpenedWorkspace::Loaded(_)) {
            return opened;
        }
    }
    match create_personal() {
        Some(path) => open(path),
        None => OpenedWorkspace::None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::workspace_tests::write;

    fn temp_dir() -> tempfile::TempDir {
        match tempfile::TempDir::new() {
            Ok(dir) => dir,
            Err(error) => panic!("temp dir: {error}"),
        }
    }

    fn workspace(name: &str) -> tempfile::TempDir {
        let dir = temp_dir();
        write(
            dir.path(),
            "reqflux.toml",
            &format!(
                "id = \"{}\"\nname = \"{name}\"\nformat = 1\n",
                reqflux_core::model::Id::new()
            ),
        );
        dir
    }

    fn name(opened: &OpenedWorkspace) -> Option<&str> {
        opened.name()
    }

    /// The startup over `requested` and `recents`, counting the `Personal` creations; it
    /// creates `Personal` in `managed` when asked.
    fn start(
        requested: Option<&Path>,
        recents: &[PathBuf],
        managed: &Path,
        created: &Cell<usize>,
    ) -> OpenedWorkspace {
        startup_workspace(
            requested.map(Path::to_path_buf),
            recents,
            |path| OpenedWorkspace::open(Some(path)),
            || {
                created.set(created.get() + 1);
                reqflux_engine::create_managed_workspace_in(managed, PERSONAL)
                    .ok()
                    .map(|(_, dir)| dir)
            },
        )
    }

    #[test]
    fn a_requested_directory_wins() {
        let (asked, recent, managed) = (workspace("Asked"), workspace("Recent"), temp_dir());
        let created = Cell::new(0);
        let opened = start(
            Some(asked.path()),
            &[recent.path().to_path_buf()],
            managed.path(),
            &created,
        );
        assert_eq!(name(&opened), Some("Asked"));
        assert_eq!(created.get(), 0);
    }

    #[test]
    fn the_most_recent_loading_recent_wins_and_a_broken_one_is_skipped() {
        let (newer, older, managed) = (workspace("Newer"), workspace("Older"), temp_dir());
        let created = Cell::new(0);
        let recents = [newer.path().to_path_buf(), older.path().to_path_buf()];
        assert_eq!(
            name(&start(None, &recents, managed.path(), &created)),
            Some("Newer")
        );
        let broken = [managed.path().join("gone"), older.path().to_path_buf()];
        assert_eq!(
            name(&start(None, &broken, managed.path(), &created)),
            Some("Older")
        );
        assert_eq!(created.get(), 0);
    }

    #[test]
    fn without_a_loading_recent_personal_is_created_once() {
        let managed = temp_dir();
        let created = Cell::new(0);
        let gone = [managed.path().join("gone")];
        let opened = start(None, &gone, managed.path(), &created);
        assert_eq!(name(&opened), Some(PERSONAL));
        assert_eq!(created.get(), 1);
        let OpenedWorkspace::Loaded(engine) = &opened else {
            panic!("Personal did not load");
        };
        // The next start finds it among the recents and creates nothing.
        let again = start(
            None,
            &[engine.root().to_path_buf()],
            managed.path(),
            &created,
        );
        assert_eq!(name(&again), Some(PERSONAL));
        assert_eq!(created.get(), 1);
    }

    #[test]
    fn without_the_app_state_personal_is_still_created() {
        let managed = temp_dir();
        let created = Cell::new(0);
        assert_eq!(
            name(&start(None, &[], managed.path(), &created)),
            Some(PERSONAL)
        );
        assert_eq!(created.get(), 1);
    }

    #[test]
    fn personal_that_cannot_be_created_leaves_the_empty_state() {
        let opened =
            startup_workspace(None, &[], |path| OpenedWorkspace::open(Some(path)), || None);
        assert!(matches!(opened, OpenedWorkspace::None));
    }
}
