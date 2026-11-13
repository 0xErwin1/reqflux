//! The request line (S4), the sub-tab bar (S5), the unresolved notice (S6) and their
//! automation ids (S7), and the active environment of the window.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use super::request::SubTab;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000011";
const INVOICE: &str = "01J00000000000000000000012";
const PLAIN: &str = "01J00000000000000000000013";
const RICH: &str = "01J00000000000000000000014";
const OAUTH: &str = "01J00000000000000000000015";
const DEV: &str = "01J00000000000000000000016";

/// `Demo` with collection `API`, which declares `base`, holding `GET Invoice` at
/// `{{base}}/v1/{{missing}}` with enabled and disabled params and headers, a header that
/// uses the secret `token` and one with the unresolved `tenant`, `GET Plain` with nothing
/// else, `POST Rich` with a raw JSON body, bearer auth, a pre-request script and a timeout,
/// `GET OAuth` with an auth type the engine does not send, and the environment `dev`, which
/// defines `missing` and `tenant`.
fn request_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n[[variables]]\nkey = \"base\"\nvalue = \"https://h\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/api/invoice.req.toml",
        &format!(
            "id = \"{INVOICE}\"\nname = \"Invoice\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/v1/{{{{missing}}}}\"\n\n\
             [[params]]\nkey = \"a\"\nvalue = \"1\"\n\n\
             [[params]]\nkey = \"b\"\nvalue = \"2\"\nenabled = false\n\n\
             [[headers]]\nkey = \"X-One\"\nvalue = \"1\"\n\n\
             [[headers]]\nkey = \"X-Two\"\nvalue = \"2\"\n\n\
             [[headers]]\nkey = \"X-Off\"\nvalue = \"3\"\nenabled = false\n\n\
             [[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{{{token}}}}\"\n\n\
             [[headers]]\nkey = \"X-Tenant\"\nvalue = \"{{{{tenant}}}}\"\n"
        ),
    );
    write(
        root,
        "collections/api/plain.req.toml",
        &format!(
            "id = \"{PLAIN}\"\nname = \"Plain\"\nmethod = \"GET\"\nurl = \"{{{{base}}}}/ok\"\n"
        ),
    );
    write(
        root,
        "collections/api/rich.req.toml",
        &format!(
            "id = \"{RICH}\"\nname = \"Rich\"\nmethod = \"POST\"\nurl = \"{{{{base}}}}/rich\"\n\n\
             [auth]\ntype = \"bearer\"\ntoken = \"plain-token\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{\\n  \\\"a\\\": \\\"{{{{base}}}}\\\"\\n}}\"\n\n\
             [settings]\ntimeout_ms = 5000\n"
        ),
    );
    write(root, "collections/api/rich.pre.js", "console.log(1);\n");
    write(
        root,
        "collections/api/oauth.req.toml",
        &format!(
            "id = \"{OAUTH}\"\nname = \"OAuth\"\nmethod = \"GET\"\nurl = \"{{{{missing}}}}\"\n\n\
             [auth]\ntype = \"oauth2\"\n"
        ),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!(
            "id = \"{DEV}\"\nname = \"dev\"\n\n\
             [[variables]]\nkey = \"missing\"\nvalue = \"m\"\n\n\
             [[variables]]\nkey = \"tenant\"\nvalue = \"t\"\n"
        ),
    );
    dir
}

fn render(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    // The request panes draw with `bolt`, whose components read the theme.
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let dir = request_workspace();
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    drop(dir);
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    (shell, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

fn present(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    match bounds(cx, selector) {
        Some(found) => found,
        None => panic!("{selector} is missing"),
    }
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

fn selected_tab(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<SubTab> {
    cx.update(|_, cx| shell.read(cx).requests.get(id).map(|state| state.tab))
}

#[gpui::test]
fn the_request_line_sits_in_the_document_content(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    let content = present(cx, "document-content");
    for part in ["request-method", "request-url", "request-send"] {
        let found = present(cx, part);
        assert!(content.contains(&found.origin), "{part} in the content");
    }
    assert!(bounds(cx, "request-pane").is_some());
}

#[gpui::test]
fn the_six_sub_tabs_run_left_to_right(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    let lefts: Vec<_> = SubTab::ALL
        .iter()
        .map(|tab| present(cx, &tab.element_id()).origin.x)
        .collect();
    assert!(lefts.windows(2).all(|pair| pair.first() < pair.get(1)));
}

#[gpui::test]
fn headers_is_the_default_and_clicking_selects(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    assert_eq!(selected_tab(cx, &shell, INVOICE), Some(SubTab::Headers));
    let counts = cx.update(|_, cx| {
        shell.read(cx).requests.get(INVOICE).map(|state| {
            let request = &state.inspection.request.request;
            (
                super::request::count(request, SubTab::Params),
                super::request::count(request, SubTab::Headers),
            )
        })
    });
    assert_eq!(counts, Some((Some(1), Some(4))));

    click(cx, "request-tab-body");
    assert_eq!(selected_tab(cx, &shell, INVOICE), Some(SubTab::Body));
}

#[gpui::test]
fn the_unresolved_notice_follows_the_request(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    assert!(bounds(cx, "request-unresolved").is_some());

    click(cx, &format!("tree-row-{PLAIN}"));
    assert!(bounds(cx, "request-unresolved").is_none());
}

#[gpui::test]
fn closing_the_tab_drops_its_state(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    assert!(selected_tab(cx, &shell, INVOICE).is_some());

    click(cx, &format!("doc-tab-close-{INVOICE}"));
    assert_eq!(selected_tab(cx, &shell, INVOICE), None);
}

fn rows(cx: &mut VisualTestContext, table: &str) -> Vec<gpui::Bounds<gpui::Pixels>> {
    (1..=10)
        .map_while(|n| bounds(cx, &format!("request-{table}-row-{n}")))
        .collect()
}

#[gpui::test]
fn headers_render_every_row_in_order_inside_the_pane(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    let pane = present(cx, "request-pane");
    let headers = rows(cx, "headers");
    assert_eq!(
        headers.len(),
        5,
        "enabled, disabled, secret and unresolved rows"
    );
    assert!(headers.iter().all(|row| pane.contains(&row.origin)));
    assert!(headers.windows(2).all(|pair| match pair {
        [above, below] => above.origin.y < below.origin.y,
        _ => false,
    }));
    assert!(rows(cx, "params").is_empty());
}

#[gpui::test]
fn params_replace_the_headers_grid(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    click(cx, "request-tab-params");
    assert_eq!(rows(cx, "params").len(), 2);
    assert!(rows(cx, "headers").is_empty());
    assert!(bounds(cx, "request-grid-empty").is_none());
}

#[gpui::test]
fn an_empty_table_shows_its_columns_and_the_add_row_instead_of_an_empty_state(
    cx: &mut TestAppContext,
) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{PLAIN}"));
    assert!(rows(cx, "headers").is_empty());
    let pane = present(cx, "request-pane");
    assert!(bounds(cx, "request-grid-empty").is_none());
    assert!(pane.contains(&present(cx, "request-headers-header").origin));
    let add = present(cx, "request-headers-add");
    assert!(pane.contains(&add.origin));
    assert!(add.origin.y > present(cx, "request-headers-header").origin.y);
}

fn inside_pane(cx: &mut VisualTestContext, ids: &[&str]) {
    let pane = present(cx, "request-pane");
    for id in ids {
        assert!(pane.contains(&present(cx, id).origin), "{id} in the pane");
    }
}

#[gpui::test]
fn the_body_pane_shows_the_raw_text_line_by_line(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{RICH}"));
    click(cx, "request-tab-body");
    inside_pane(cx, &["request-body-mode", "request-body-code"]);
    let lines: Vec<_> = (1..=5)
        .map_while(|n| bounds(cx, &format!("request-body-line-{n}")))
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(lines.windows(2).all(|pair| match pair {
        [above, below] => above.origin.y < below.origin.y,
        _ => false,
    }));
}

#[gpui::test]
fn the_other_panes_show_their_content(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{RICH}"));
    click(cx, "request-tab-auth");
    inside_pane(cx, &["request-auth-type", "request-auth-field-token"]);
    click(cx, "request-tab-scripts");
    inside_pane(cx, &["request-script-pre", "request-script-test-empty"]);
    click(cx, "request-tab-settings");
    inside_pane(
        cx,
        &[
            "request-setting-timeout",
            "request-setting-redirects",
            "request-setting-tls",
        ],
    );
}

#[gpui::test]
fn a_request_without_body_or_scripts_shows_empty_states(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{PLAIN}"));
    click(cx, "request-tab-body");
    inside_pane(cx, &["request-body-mode", "request-body-empty"]);
    click(cx, "request-tab-scripts");
    inside_pane(cx, &["request-script-pre-empty"]);
    // The pane scrolls; the test script's empty state sits below the pre-request one.
    let pane = present(cx, "request-pane");
    let test = present(cx, "request-script-test-empty");
    assert!(
        test.top() >= pane.top(),
        "request-script-test-empty in the scrolled pane"
    );
}

fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> Option<usize> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(id)
            .map(|state| state.inspection.unresolved.len())
    })
}

#[gpui::test]
fn an_environment_resolves_open_requests_and_keeps_their_sub_tab(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    click(cx, "request-tab-body");
    assert_eq!(unresolved(cx, &shell, INVOICE), Some(3));
    assert!(bounds(cx, "request-unresolved").is_some());

    shell.update(cx, |shell, cx| {
        shell.set_environment(Some("dev".to_owned()), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        unresolved(cx, &shell, INVOICE),
        Some(1),
        "the valueless secret token"
    );
    assert_eq!(selected_tab(cx, &shell, INVOICE), Some(SubTab::Body));

    shell.update(cx, |shell, cx| shell.set_environment(None, cx));
    cx.run_until_parked();
    assert_eq!(unresolved(cx, &shell, INVOICE), Some(3));
}

#[gpui::test]
fn a_request_opened_after_choosing_an_environment_uses_it(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    shell.update(cx, |shell, cx| {
        shell.set_environment(Some("dev".to_owned()), cx)
    });
    click(cx, &format!("tree-row-{INVOICE}"));
    assert_eq!(unresolved(cx, &shell, INVOICE), Some(1));
}

#[gpui::test]
fn a_request_that_cannot_be_prepared_shows_the_problem(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{OAUTH}"));
    assert!(bounds(cx, "request-problem").is_some());
    assert!(bounds(cx, "request-unresolved").is_none());
}

#[gpui::test]
fn grid_cells_share_their_edges_as_drawn(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    // The header and rows take the heights of `bolt`'s data grid.
    let header = present(cx, "request-headers-header");
    assert_eq!(header.size.height, gpui::px(40.));
    let row = present(cx, "request-headers-row-1");
    assert_eq!(row.size.height, gpui::px(31.));
    assert_eq!(
        present(cx, "request-headers-enabled-1").size.width,
        gpui::px(16.)
    );

    let key = present(cx, "request-headers-key-1");
    let value = present(cx, "request-headers-value-1");
    let description = present(cx, "request-headers-description-1");
    let delete = present(cx, "request-headers-delete-1");
    assert_eq!(key.left() - row.left(), gpui::px(36.), "the enable column");
    assert_eq!(key.size.width, gpui::px(210.));
    assert_eq!(value.left(), key.right());
    assert_eq!(
        description.left(),
        value.right(),
        "the value fills the rest"
    );
    assert_eq!(description.size.width, gpui::px(230.));
    // `request-headers-delete-<n>` is bolt's Button, centered in the last 34 px column.
    assert_eq!(row.right() - description.right(), gpui::px(34.));
    assert_eq!(
        delete.left() - description.right(),
        row.right() - delete.right(),
        "the delete button is centered in its column"
    );
    for cell in [key, value, description] {
        assert_eq!((cell.top(), cell.size.height), (row.top(), row.size.height));
    }
}

#[gpui::test]
fn the_add_row_is_a_grid_row_under_the_last_one(cx: &mut TestAppContext) {
    let (_shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    let last = present(cx, "request-headers-row-5");
    let add = present(cx, "request-headers-add");
    assert_eq!(add.top(), last.bottom());
    assert_eq!(add.size.height, gpui::px(31.));
    assert_eq!(add.size.width, last.size.width);
}

#[gpui::test]
fn the_open_cell_is_a_well_in_the_cells_place(cx: &mut TestAppContext) {
    // The cell's text field reads the installed theme.
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    let closed = present(cx, "request-headers-value-2");
    click(cx, "request-headers-value-2");
    assert!(
        cx.update(|_, cx| shell.read(cx).cell_edit.is_some()),
        "the cell is open"
    );
    assert_eq!(present(cx, "request-headers-value-2"), closed);
}

#[gpui::test]
fn the_value_column_names_the_environment_it_resolves_in(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx);
    click(cx, &format!("tree-row-{INVOICE}"));
    assert!(bounds(cx, "request-headers-value-hint").is_none());
    shell.update(cx, |shell, cx| {
        shell.set_environment(Some("dev".to_owned()), cx)
    });
    cx.run_until_parked();
    assert!(bounds(cx, "request-headers-value-hint").is_some());
}

// Editors fill the request pane (editor-fill S1-S3): every code editor takes the height the
// request/response split leaves and follows it.

const FILL: &str = "01J00000000000000000000017";

/// `POST Fill`, whose raw JSON body and pre-request script fit in a few lines and which
/// targets a port nothing listens on, so a send fails at once and brings the response in.
fn fill_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/fill.req.toml",
        &format!(
            "id = \"{FILL}\"\nname = \"Fill\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:{}/p\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{}}\"\n",
            super::send_tests::closed_port()
        ),
    );
    write(root, "collections/api/fill.pre.js", "console.log(1);\n");
    dir
}

/// Opens `POST Fill` on the sub-tab `tab`.
fn open_fill<'a>(
    cx: &'a mut TestAppContext,
    tab: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let dir = fill_workspace();
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    drop(dir);
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(workspace, cx));
    cx.run_until_parked();
    click(cx, &format!("tree-row-{FILL}"));
    click(cx, &format!("request-tab-{tab}"));
    (shell, cx)
}

/// How far the pane's bottom is under `id`'s: the pane's own bottom padding at most.
fn gap_to_pane_bottom(cx: &mut VisualTestContext, id: &str) -> gpui::Pixels {
    present(cx, "request-pane").bottom() - present(cx, id).bottom()
}

fn assert_reaches_pane_bottom(cx: &mut VisualTestContext, id: &str) {
    let gap = gap_to_pane_bottom(cx, id);
    assert!(
        gap >= gpui::px(0.) && gap <= gpui::px(24.),
        "{id} ends {gap:?} above the pane's bottom"
    );
}

fn height(cx: &mut VisualTestContext, id: &str) -> gpui::Pixels {
    present(cx, id).size.height
}

#[gpui::test]
fn the_raw_body_editor_fills_the_pane_and_the_well_does_not_jump(cx: &mut TestAppContext) {
    let (_shell, cx) = open_fill(cx, "body");
    assert_reaches_pane_bottom(cx, "request-body-raw");
    let reading = present(cx, "request-body-raw");

    click(cx, "request-body-code");
    assert_reaches_pane_bottom(cx, "request-body-input");
    assert_reaches_pane_bottom(cx, "request-body-input-editor");
    let editing = present(cx, "request-body-input");
    assert_eq!(
        editing.top(),
        reading.top(),
        "editing starts where reading was"
    );
    assert_eq!(
        editing.bottom(),
        reading.bottom(),
        "and ends where it ended"
    );
    let rows = bolt::tokens::EditorMetrics::CODE_LINE_HEIGHT * 12.;
    assert!(
        height(cx, "request-body-input-editor") > rows,
        "the editor is taller than the 12 rows it used to have"
    );
}

#[gpui::test]
fn a_taller_window_gives_the_raw_body_editor_the_extra_height(cx: &mut TestAppContext) {
    let (_shell, cx) = open_fill(cx, "body");
    click(cx, "request-body-code");
    let before = height(cx, "request-body-input-editor");
    let window = cx.update(|window, _| window.viewport_size());
    cx.simulate_resize(gpui::size(window.width, window.height + gpui::px(120.)));
    cx.run_until_parked();
    assert_eq!(
        height(cx, "request-body-input-editor") - before,
        gpui::px(120.),
        "the editor took the whole 120px"
    );
    assert_reaches_pane_bottom(cx, "request-body-input");
}

#[gpui::test]
fn dragging_the_split_resizes_the_editor_down_to_a_few_lines(cx: &mut TestAppContext) {
    let (shell, cx) = open_fill(cx, "body");
    // Tall enough that the request pane keeps room above its minimum when the split moves.
    let width = cx.update(|window, _| window.viewport_size().width);
    cx.simulate_resize(gpui::size(width, gpui::px(1400.)));
    click(cx, "request-body-code");
    click(cx, "request-send");
    cx.run_until_parked();
    assert!(
        bounds(cx, "response-area").is_some(),
        "the send brought the response in"
    );
    // The split opens short; give the request pane room to shrink in.
    shell.update(cx, |shell, cx| shell.step_split(gpui::px(300.), cx));
    cx.run_until_parked();
    assert_reaches_pane_bottom(cx, "request-body-input");
    let before = height(cx, "request-body-input-editor");

    shell.update(cx, |shell, cx| shell.step_split(gpui::px(-100.), cx));
    cx.run_until_parked();
    assert_eq!(
        before - height(cx, "request-body-input-editor"),
        gpui::px(100.),
        "moving the split up shrinks the editor by as much ({before:?} before)"
    );
    assert_reaches_pane_bottom(cx, "request-body-input");

    shell.update(cx, |shell, cx| shell.step_split(gpui::px(100.), cx));
    cx.run_until_parked();
    assert_eq!(height(cx, "request-body-input-editor"), before, "and back");

    shell.update(cx, |shell, cx| shell.step_split(gpui::px(-5000.), cx));
    cx.run_until_parked();
    assert!(
        height(cx, "request-body-input-editor")
            >= bolt::tokens::EditorMetrics::CODE_LINE_HEIGHT * super::code_edit::MIN_ROWS as f32,
        "a split squeezed shut still leaves a usable editor"
    );
}

#[gpui::test]
fn both_graphql_editors_fill_the_pane(cx: &mut TestAppContext) {
    let (_shell, cx) = open_fill(cx, "body");
    click(cx, "body-mode-graphql");
    for (well, editor) in [
        ("request-body-query", "request-body-query-input"),
        ("request-body-variables", "request-body-variables-input"),
    ] {
        assert_reaches_pane_bottom(cx, well);
        click(cx, &format!("{well}-code"));
        assert_reaches_pane_bottom(cx, editor);
        assert_reaches_pane_bottom(cx, &format!("{editor}-editor"));
    }
}

#[gpui::test]
fn the_script_editor_fills_the_pane(cx: &mut TestAppContext) {
    let (_shell, cx) = open_fill(cx, "scripts");
    click(cx, "request-script-pre-code");
    // The edited box takes what the note and the other box leave: the stack ends at the pane.
    assert_reaches_pane_bottom(cx, "request-script-test-editor");
    let editor = present(cx, "request-script-pre-input");
    let boxed = present(cx, "request-script-pre-editor");
    assert!(boxed.contains(&editor.origin) && boxed.contains(&editor.bottom_right()));
    assert!(
        editor.size.height > bolt::tokens::EditorMetrics::CODE_LINE_HEIGHT * 12.,
        "taller than the 12 rows it used to have"
    );
}

#[gpui::test]
fn the_docs_editor_fills_the_pane(cx: &mut TestAppContext) {
    let (_shell, cx) = open_fill(cx, "docs");
    click(cx, "request-docs-add");
    assert_reaches_pane_bottom(cx, "request-docs-input");
    assert_reaches_pane_bottom(cx, "request-docs-input-editor");
}

#[gpui::test]
fn tabs_without_an_editor_keep_their_natural_height_and_the_pane_scrolls_them(
    cx: &mut TestAppContext,
) {
    let (_shell, cx) = open_fill(cx, "settings");
    let pane = present(cx, "request-pane");
    let content = present(cx, "request-setting-timeout-text");
    assert!(
        pane.bottom() - content.bottom() > gpui::px(100.),
        "a settings row stays its own height instead of stretching to the pane"
    );
    assert!(pane.top() < content.top());
}
