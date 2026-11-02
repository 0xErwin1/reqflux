//! Script writes of a window send persist as current values (variables-editor S5): a later
//! send resolves them, also from a new shell over the same app state.

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{click, render, state_file};
use super::send_tests::{Server, serve, settle};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000091";
pub(super) const LOGIN: &str = "01J00000000000000000000092";
const USE: &str = "01J00000000000000000000093";
const DEV: &str = "01J00000000000000000000094";

/// A workspace with the environment `dev` declaring `token = "shared"`, and the collection
/// `API` holding `GET Login`, whose pre-request script sets the environment's `token`, and
/// `GET Use` at `/use/{{token}}`, both on `port`.
pub(super) fn workspace(port: u16) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{DEV}\"\nname = \"dev\"\n\n[[variables]]\nkey = \"token\"\nvalue = \"shared\"\n"
        ),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/login.req.toml",
        &format!(
            "id = \"{LOGIN}\"\nname = \"Login\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{port}/login\"\n"
        ),
    );
    write(
        root,
        "collections/api/login.pre.js",
        "pm.environment.set('token', 'abc');\npm.variables.set('local', 'kept-out');\n",
    );
    write(
        root,
        "collections/api/use.req.toml",
        &format!(
            "id = \"{USE}\"\nname = \"Use\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{port}/use/{{{{token}}}}\"\n"
        ),
    );
    dir
}

pub(super) fn select_dev(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_environment(Some("dev".to_owned()), cx);
        });
    });
    cx.run_until_parked();
}

fn send(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) {
    click(cx, &format!("tree-row-{id}"));
    click(cx, "request-send");
    assert_eq!(settle(cx, shell, id).as_deref(), Some("200 OK"));
}

/// The URL of the newest history entry of the loaded workspace.
fn last_sent_url(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let (state, workspace) = (shell.state.as_ref()?, shell.workspace_id.as_deref()?);
        match state.history(workspace, 1) {
            Ok(entries) => entries.first().map(|entry| entry.record.sent.url.clone()),
            Err(error) => panic!("history: {error}"),
        }
    })
}

#[gpui::test]
fn a_script_environment_write_resolves_in_the_next_send_and_after_a_restart(
    cx: &mut TestAppContext,
) {
    let port = serve(Server::Ok);
    let dir = workspace(port);
    let (_state_dir, state_path) = state_file();
    {
        let (shell, cx) = render(cx, &dir, &state_path);
        select_dev(cx, &shell);
        send(cx, &shell, USE);
        assert_eq!(
            last_sent_url(cx, &shell),
            Some(format!("http://127.0.0.1:{port}/use/shared"))
        );

        send(cx, &shell, LOGIN);
        send(cx, &shell, USE);
        assert_eq!(
            last_sent_url(cx, &shell),
            Some(format!("http://127.0.0.1:{port}/use/abc"))
        );
    }

    let (shell, cx) = render(cx, &dir, &state_path);
    select_dev(cx, &shell);
    send(cx, &shell, USE);
    assert_eq!(
        last_sent_url(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/abc"))
    );
    let local = cx.update(|_, cx| {
        let shell = shell.read(cx);
        shell.current.iter().any(|(_, key, _)| key == "local")
    });
    assert!(!local, "pm.variables writes are never persisted");
}
