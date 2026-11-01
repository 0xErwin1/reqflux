//! The sidebar filter over the tree model (S1): requests match by name, method or URL,
//! case-insensitive; their folders and collections show expanded; the counts follow.

use super::{Node, NodeKind, TreeState};

fn request(id: &str, name: &str, method: &str, url: &str) -> Node {
    Node {
        id: id.to_owned(),
        name: name.to_owned(),
        kind: NodeKind::Request {
            method: method.to_owned(),
        },
        url: url.to_owned(),
        children: Vec::new(),
    }
}

fn container(id: &str, name: &str, kind: NodeKind, children: Vec<Node>) -> Node {
    Node {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
        url: String::new(),
        children,
    }
}

/// `API` (folder `users` holding `GET List users`, then `GET Health`) and `Auth`
/// (`POST Token`).
fn tree() -> TreeState {
    TreeState::new(vec![
        container(
            "api",
            "API",
            NodeKind::Collection,
            vec![
                container(
                    "users",
                    "users",
                    NodeKind::Folder,
                    vec![request("list", "List users", "GET", "{{base}}/users")],
                ),
                request("health", "Health", "GET", "https://api.test/health"),
            ],
        ),
        container(
            "auth",
            "Auth",
            NodeKind::Collection,
            vec![request(
                "token",
                "Token",
                "POST",
                "https://login.test/oauth",
            )],
        ),
    ])
}

fn ids(tree: &TreeState) -> Vec<&str> {
    tree.visible_rows()
        .iter()
        .map(|row| row.node.id.as_str())
        .collect()
}

#[test]
fn a_name_match_shows_the_request_with_its_folders_expanded() {
    let mut tree = tree();
    tree.set_filter("LIST");
    assert_eq!(ids(&tree), ["api", "users", "list"]);
    let rows = tree.visible_rows();
    assert!(
        rows.iter()
            .filter(|row| row.node.is_expandable())
            .all(|row| row.expanded)
    );
    assert_eq!(tree.shown_counts(), (1, 1));
}

#[test]
fn a_method_match_is_case_insensitive() {
    let mut tree = tree();
    tree.set_filter("post");
    assert_eq!(ids(&tree), ["auth", "token"]);
    assert_eq!(tree.shown_counts(), (1, 1));
}

#[test]
fn a_url_match_shows_the_request() {
    let mut tree = tree();
    tree.set_filter("OAUTH");
    assert_eq!(ids(&tree), ["auth", "token"]);
    tree.set_filter("api.test/health");
    assert_eq!(ids(&tree), ["api", "health"]);
}

#[test]
fn folder_and_collection_names_alone_do_not_match() {
    let mut tree = tree();
    tree.set_filter("API");
    assert_eq!(
        ids(&tree),
        ["api", "health"],
        "only the URL of Health holds api"
    );
}

#[test]
fn several_collections_count_what_is_shown() {
    let mut tree = tree();
    tree.set_filter("get");
    assert_eq!(ids(&tree), ["api", "users", "list", "health"]);
    tree.set_filter("t");
    assert_eq!(tree.shown_counts(), (3, 2));
}

#[test]
fn no_match_shows_no_row_and_counts_nothing() {
    let mut tree = tree();
    tree.set_filter("zzz");
    assert!(tree.visible_rows().is_empty());
    assert_eq!(tree.shown_counts(), (0, 0));
    assert!(tree.is_filtered());
}

#[test]
fn clearing_the_filter_restores_the_expansion_it_had() {
    let mut tree = tree();
    tree.set_filter("list");
    tree.set_filter("");
    assert!(!tree.is_filtered());
    assert_eq!(ids(&tree), ["api", "users", "health", "auth", "token"]);
    assert_eq!(tree.shown_counts(), (3, 2));
}

#[test]
fn navigation_runs_over_the_filtered_rows() {
    let mut tree = tree();
    tree.set_filter("token");
    tree.select_next();
    tree.select_next();
    assert_eq!(tree.selected(), Some("token"));
}
