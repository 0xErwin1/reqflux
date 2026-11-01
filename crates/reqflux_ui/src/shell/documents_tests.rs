//! The open documents model (S1) and the request location the path bar shows (S2).

use super::{Documents, Location, locate};
use crate::shell::tree_model::{Node, NodeKind};

fn opened(ids: &[&str]) -> Documents {
    let mut documents = Documents::default();
    for id in ids {
        documents.open(id);
    }
    documents
}

fn ids(documents: &Documents) -> Vec<&str> {
    documents.open_ids().iter().map(String::as_str).collect()
}

#[test]
fn starts_empty() {
    let documents = Documents::default();
    assert!(documents.open_ids().is_empty());
    assert_eq!(documents.active(), None);
}

#[test]
fn open_appends_and_activates() {
    let documents = opened(&["a", "b"]);
    assert_eq!(ids(&documents), ["a", "b"]);
    assert_eq!(documents.active(), Some("b"));
}

#[test]
fn opening_an_open_request_focuses_it_without_a_duplicate() {
    let mut documents = opened(&["a", "b", "c"]);
    documents.open("a");
    assert_eq!(ids(&documents), ["a", "b", "c"]);
    assert_eq!(documents.active(), Some("a"));
}

#[test]
fn activate_selects_an_open_tab_and_ignores_unknown_ids() {
    let mut documents = opened(&["a", "b"]);
    documents.activate("a");
    assert_eq!(documents.active(), Some("a"));
    documents.activate("zzz");
    assert_eq!(documents.active(), Some("a"));
    assert_eq!(ids(&documents), ["a", "b"]);
}

#[test]
fn closing_the_active_tab_activates_the_next_one() {
    let mut documents = opened(&["a", "b", "c"]);
    documents.activate("b");
    documents.close("b");
    assert_eq!(ids(&documents), ["a", "c"]);
    assert_eq!(documents.active(), Some("c"));
}

#[test]
fn closing_the_active_last_tab_activates_the_previous_one() {
    let mut documents = opened(&["a", "b", "c"]);
    documents.close("c");
    assert_eq!(ids(&documents), ["a", "b"]);
    assert_eq!(documents.active(), Some("b"));
}

#[test]
fn closing_an_inactive_tab_keeps_the_active_one() {
    let mut documents = opened(&["a", "b", "c"]);
    documents.close("a");
    assert_eq!(ids(&documents), ["b", "c"]);
    assert_eq!(documents.active(), Some("c"));
    documents.close("zzz");
    assert_eq!(ids(&documents), ["b", "c"]);
}

#[test]
fn closing_the_only_tab_leaves_nothing_active() {
    let mut documents = opened(&["a"]);
    documents.close("a");
    assert!(documents.open_ids().is_empty());
    assert_eq!(documents.active(), None);
}

fn node(id: &str, name: &str, kind: NodeKind, children: Vec<Node>) -> Node {
    Node {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
        url: String::new(),
        children,
    }
}

fn request(id: &str, name: &str, method: &str) -> Node {
    let kind = NodeKind::Request {
        method: method.to_owned(),
    };
    node(id, name, kind, Vec::new())
}

/// `Billing API` › `invoices` › `drafts` › `POST Create invoice`, then `GET Health` at the
/// top of `Billing API`, and `Auth` with `POST Token`.
fn roots() -> Vec<Node> {
    let drafts = node(
        "drafts",
        "drafts",
        NodeKind::Folder,
        vec![request("create", "Create invoice", "POST")],
    );
    let invoices = node("invoices", "invoices", NodeKind::Folder, vec![drafts]);
    vec![
        node(
            "billing",
            "Billing API",
            NodeKind::Collection,
            vec![invoices, request("health", "Health", "GET")],
        ),
        node(
            "auth",
            "Auth",
            NodeKind::Collection,
            vec![request("token", "Token", "POST")],
        ),
    ]
}

#[test]
fn locate_walks_collection_and_folders() {
    assert_eq!(
        locate(&roots(), "create"),
        Some(Location {
            collection: "Billing API".to_owned(),
            folders: vec!["invoices".to_owned(), "drafts".to_owned()],
            name: "Create invoice".to_owned(),
            method: "POST".to_owned(),
        })
    );
}

#[test]
fn locate_finds_requests_at_the_collection_top_level() {
    let token = locate(&roots(), "token");
    assert_eq!(
        token,
        Some(Location {
            collection: "Auth".to_owned(),
            folders: Vec::new(),
            name: "Token".to_owned(),
            method: "POST".to_owned(),
        })
    );
    let health = locate(&roots(), "health").map(|location| location.folders);
    assert_eq!(health, Some(Vec::new()));
}

#[test]
fn locate_ignores_unknown_ids_and_non_requests() {
    for id in ["missing", "billing", "invoices"] {
        assert_eq!(locate(&roots(), id), None, "{id}");
    }
}
