//! The active environment is remembered per workspace (standalone-workspaces S8).

use std::fs;

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::Shell;
use super::workspace_picker::TestPicker;
use super::workspace_switch_tests::{FIRST_ID, click, pair, render, sibling};

fn active(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).active_environment.clone())
}

#[gpui::test]
fn the_picked_environment_comes_back_with_its_workspace(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    {
        let (shell, cx) = render(
            cx,
            Some(pair.first.path()),
            &pair.state_path,
            TestPicker::default(),
        );
        click(cx, "environment-selector");
        click(cx, "environment-option-dev");
        assert_eq!(active(cx, &shell).as_deref(), Some("dev"));
        // A switch away and back restores it too.
        click(cx, "workspace-selector");
        click(cx, "workspace-recent-2");
        assert_eq!(active(cx, &shell), None);
        click(cx, "workspace-selector");
        click(cx, "workspace-recent-2");
        assert_eq!(active(cx, &shell).as_deref(), Some("dev"));
    }
    {
        let (shell, cx) = render(
            cx,
            Some(pair.first.path()),
            &pair.state_path,
            TestPicker::default(),
        );
        assert_eq!(active(cx, &shell).as_deref(), Some("dev"));
    }
    let stored = super::workspace_switch_tests::open_state(&pair.state_path)
        .setting(&format!("active_environment/{FIRST_ID}"));
    assert_eq!(
        stored.ok().flatten().as_deref(),
        Some(sibling(FIRST_ID, '4').as_str())
    );
    if let Err(error) = fs::remove_file(pair.first.path().join("environments/dev.env.toml")) {
        panic!("remove: {error}");
    }
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    assert_eq!(active(cx, &shell), None);
}
