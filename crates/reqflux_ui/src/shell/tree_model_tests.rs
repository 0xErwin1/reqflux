//! The tree model of S8 and the navigation rules of S9.

use super::{Node, NodeKind, TreeState};

fn request(id: &str, method: &str) -> Node {
    Node {
        id: id.to_owned(),
        name: format!("{id} name"),
        kind: NodeKind::Request {
            method: method.to_owned(),
        },
        url: String::new(),
        children: Vec::new(),
    }
}

fn container(id: &str, kind: NodeKind, children: Vec<Node>) -> Node {
    Node {
        id: id.to_owned(),
        name: format!("{id} name"),
        kind,
        url: String::new(),
        children,
    }
}

/// `api` holds folder `users` (holding `list` and `create`) and request `health`; `auth`
/// holds request `token`; `empty` has no children.
fn tree() -> TreeState {
    let users = container(
        "users",
        NodeKind::Folder,
        vec![request("list", "GET"), request("create", "POST")],
    );
    TreeState::new(vec![
        container(
            "api",
            NodeKind::Collection,
            vec![users, request("health", "GET")],
        ),
        container("auth", NodeKind::Collection, vec![request("token", "POST")]),
        container("empty", NodeKind::Collection, Vec::new()),
    ])
}

fn visible(state: &TreeState) -> Vec<(String, usize)> {
    state
        .visible_rows()
        .iter()
        .map(|row| (row.node.id.clone(), row.depth))
        .collect()
}

fn ids(pairs: &[(&str, usize)]) -> Vec<(String, usize)> {
    pairs
        .iter()
        .map(|(id, depth)| ((*id).to_owned(), *depth))
        .collect()
}

#[test]
fn collections_start_expanded_and_folders_collapsed_in_store_order() {
    let state = tree();
    assert_eq!(
        visible(&state),
        ids(&[
            ("api", 0),
            ("users", 1),
            ("health", 1),
            ("auth", 0),
            ("token", 1),
            ("empty", 0),
        ])
    );
    assert_eq!(state.selected(), None);
}

#[test]
fn expanding_a_folder_shows_its_children_after_it() {
    let mut state = tree();
    state.toggle("users");
    assert_eq!(
        visible(&state),
        ids(&[
            ("api", 0),
            ("users", 1),
            ("list", 2),
            ("create", 2),
            ("health", 1),
            ("auth", 0),
            ("token", 1),
            ("empty", 0),
        ])
    );
}

#[test]
fn request_labels_carry_the_method() {
    let state = tree();
    let labels: Vec<String> = state
        .visible_rows()
        .iter()
        .map(|row| row.node.label())
        .collect();
    assert_eq!(labels.get(1).map(String::as_str), Some("users name"));
    assert_eq!(labels.get(2).map(String::as_str), Some("GET health name"));
}

#[test]
fn down_moves_through_visible_rows_and_stops_at_the_last() {
    let mut state = tree();
    state.select_next();
    assert_eq!(state.selected(), Some("api"));
    state.select_next();
    assert_eq!(state.selected(), Some("users"));
    state.select_next();
    assert_eq!(state.selected(), Some("health"), "skips collapsed children");
    state.select("empty");
    state.select_next();
    assert_eq!(state.selected(), Some("empty"));
}

#[test]
fn up_moves_back_and_stops_at_the_first() {
    let mut state = tree();
    state.select_previous();
    assert_eq!(state.selected(), Some("api"));
    state.select_previous();
    assert_eq!(state.selected(), Some("api"));
    state.select("auth");
    state.select_previous();
    assert_eq!(state.selected(), Some("health"));
}

#[test]
fn left_collapses_an_expanded_node_then_moves_to_the_parent() {
    let mut state = tree();
    state.select("api");
    state.collapse_or_select_parent();
    assert_eq!(state.selected(), Some("api"));
    assert_eq!(
        visible(&state),
        ids(&[("api", 0), ("auth", 0), ("token", 1), ("empty", 0)])
    );

    state.select("token");
    state.collapse_or_select_parent();
    assert_eq!(state.selected(), Some("auth"));
}

#[test]
fn left_on_a_top_level_collapsed_collection_does_nothing() {
    let mut state = tree();
    state.select("auth");
    state.collapse_or_select_parent();
    let before = state.clone();
    state.collapse_or_select_parent();
    assert_eq!(state, before);
    assert_eq!(state.selected(), Some("auth"));
}

#[test]
fn right_expands_a_collapsed_node_then_moves_to_the_first_child() {
    let mut state = tree();
    state.select("users");
    state.expand_or_select_child();
    assert_eq!(state.selected(), Some("users"));
    assert_eq!(visible(&state).get(2), Some(&("list".to_owned(), 2)));
    state.expand_or_select_child();
    assert_eq!(state.selected(), Some("list"));
}

#[test]
fn right_on_a_request_or_an_empty_expanded_collection_does_nothing() {
    let mut state = tree();
    state.select("health");
    let before = state.clone();
    state.expand_or_select_child();
    assert_eq!(state, before);

    state.select("empty");
    let before = state.clone();
    state.expand_or_select_child();
    assert_eq!(state, before);
}

#[test]
fn enter_toggles_containers_and_ignores_requests() {
    let mut state = tree();
    state.select("users");
    state.toggle_selected();
    assert_eq!(visible(&state).len(), 8);
    state.toggle_selected();
    assert_eq!(visible(&state).len(), 6);

    state.select("health");
    let before = state.clone();
    state.toggle_selected();
    assert_eq!(state, before);
}

#[test]
fn totals_count_every_request_and_the_collections() {
    let state = tree();
    assert_eq!(state.request_count(), 4);
    assert_eq!(state.collection_count(), 3);
    assert_eq!(TreeState::new(Vec::new()).request_count(), 0);
}

#[test]
fn containers_count_their_direct_children() {
    let state = tree();
    let counts: Vec<_> = state
        .visible_rows()
        .iter()
        .map(|row| (row.node.id.clone(), row.node.child_count()))
        .collect();
    assert_eq!(counts.first(), Some(&("api".to_owned(), Some(2))));
    assert_eq!(counts.get(1), Some(&("users".to_owned(), Some(2))));
    assert_eq!(counts.get(2), Some(&("health".to_owned(), None)));
    assert_eq!(counts.last(), Some(&("empty".to_owned(), Some(0))));
}
