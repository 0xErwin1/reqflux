//! Saved examples as tree children (v1-features S4, design/examples/01-tree-children): a
//! request with examples expands, collapsed at first; each example is a leaf with its status;
//! the filter matches example names and status codes.

use reqflux_engine::Engine;

use super::{ExampleLeaf, Node, NodeKind, TreeState, engine_nodes, example_node, nodes_with};
use crate::shell::examples::tests::{OK_EXAMPLE, OK_ID, write_example};
use crate::shell::sidebar_tests::{HEALTH, sidebar_workspace};

fn request(id: &str, name: &str, examples: &[(&str, &str, u16)]) -> Node {
    Node {
        id: id.to_owned(),
        name: name.to_owned(),
        kind: NodeKind::Request {
            method: "GET".to_owned(),
        },
        url: format!("https://api.test/{id}"),
        children: examples
            .iter()
            .map(|(example, name, status)| {
                example_node(
                    id,
                    id,
                    ExampleLeaf {
                        id: (*example).to_owned(),
                        name: (*name).to_owned(),
                        status: *status,
                        slug: (*example).to_owned(),
                    },
                )
            })
            .collect(),
    }
}

/// `api` holds `list` (examples `OK` 200 and `Token expired` 401) and `create` (none).
fn tree() -> TreeState {
    TreeState::new(vec![Node {
        id: "api".to_owned(),
        name: "API".to_owned(),
        kind: NodeKind::Collection,
        url: String::new(),
        children: vec![
            request(
                "list",
                "List invoices",
                &[("ok", "OK", 200), ("expired", "Token expired", 401)],
            ),
            request("create", "Create invoice", &[]),
        ],
    }])
}

fn ids(tree: &TreeState) -> Vec<String> {
    tree.visible_rows()
        .iter()
        .map(|row| row.node.id.clone())
        .collect()
}

#[test]
fn a_request_with_examples_starts_collapsed_and_expands_to_its_leaves() {
    let mut tree = tree();
    assert_eq!(ids(&tree), ["api", "list", "create"]);

    tree.toggle("list");
    assert_eq!(
        ids(&tree),
        ["api", "list", "list/ok", "list/expired", "create"]
    );
    let rows = tree.visible_rows();
    let leaf = rows.get(2).map(|row| (row.depth, row.node.label()));
    assert_eq!(leaf, Some((2, "OK 200".to_owned())));
}

#[test]
fn a_request_without_examples_is_a_leaf_and_one_with_them_counts_them() {
    let tree = tree();
    let list = tree.node("list");
    let create = tree.node("create");
    assert_eq!(list.map(Node::is_expandable), Some(true));
    assert_eq!(list.and_then(Node::child_count), Some(2));
    assert_eq!(create.map(Node::is_expandable), Some(false));
    assert_eq!(create.and_then(Node::child_count), None);
    assert_eq!(list.map(Node::is_container), Some(false));
}

#[test]
fn examples_are_not_requests_and_have_their_own_count() {
    let tree = tree();
    assert_eq!(tree.request_count(), 2);
    assert_eq!(tree.shown_counts(), (2, 1));
    assert_eq!(tree.shown_examples(), 2);
    assert!(tree.is_example("list/ok"));
    assert!(!tree.is_request("list/ok"));
    assert!(tree.is_request("list"));
    assert_eq!(
        tree.node("list").map(Node::request_ids),
        Some(vec!["list".to_owned()])
    );
}

#[test]
fn keys_walk_into_the_examples_of_a_request() {
    let mut tree = tree();
    tree.select("list");
    tree.expand_or_select_child();
    tree.expand_or_select_child();
    assert_eq!(tree.selected(), Some("list/ok"));
    tree.select_next();
    assert_eq!(tree.selected(), Some("list/expired"));
    tree.collapse_or_select_parent();
    assert_eq!(tree.selected(), Some("list"));
}

#[test]
fn revealing_an_example_expands_its_request() {
    let mut tree = tree();
    tree.reveal("list/expired");
    assert_eq!(tree.selected(), Some("list/expired"));
    assert!(ids(&tree).contains(&"list/expired".to_owned()));
}

#[test]
fn the_filter_matches_example_names_and_status_codes() {
    let mut tree = tree();
    tree.set_filter("EXPIRED");
    assert_eq!(ids(&tree), ["api", "list", "list/expired"]);
    assert_eq!(tree.shown_counts(), (0, 1));
    assert_eq!(tree.shown_examples(), 1);
    assert!(!tree.shows_nothing());

    tree.set_filter("401");
    assert_eq!(ids(&tree), ["api", "list", "list/expired"]);
}

#[test]
fn a_matching_request_shows_all_its_examples() {
    let mut tree = tree();
    tree.set_filter("list inv");
    assert_eq!(ids(&tree), ["api", "list", "list/ok", "list/expired"]);
    assert_eq!(tree.shown_counts(), (1, 1));
}

#[test]
fn a_filter_matching_nothing_shows_nothing() {
    let mut tree = tree();
    tree.set_filter("zzz");
    assert!(tree.shows_nothing());
    tree.set_filter("");
    assert!(!tree.shows_nothing());
}

#[test]
fn engine_nodes_put_the_saved_examples_under_their_request() {
    let dir = sidebar_workspace();
    write_example(dir.path(), "collections/api/health", "ok", OK_EXAMPLE);
    let engine = match Engine::open(dir.path()) {
        Ok(engine) => engine,
        Err(error) => panic!("the workspace opens: {error}"),
    };

    let tree = TreeState::new(engine_nodes(&engine));
    let health = tree.node(HEALTH);
    let leaves: Vec<_> = health
        .map(|node| node.children.iter().map(|leaf| leaf.kind.clone()).collect())
        .unwrap_or_default();
    assert_eq!(
        leaves,
        [NodeKind::Example {
            request: HEALTH.to_owned(),
            example: OK_ID.to_owned(),
            status: 200,
            file: "health.examples/ok.example.toml".to_owned(),
        }]
    );
    assert_eq!(
        health
            .and_then(|node| node.children.first())
            .map(|leaf| leaf.name.as_str()),
        Some("OK")
    );

    let plain = TreeState::new(nodes_with(engine.workspace(), &|_| Vec::new()));
    assert_eq!(
        plain.node(HEALTH).map(|node| node.children.len()),
        Some(0),
        "without examples the request is a leaf"
    );
}
