#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

/// Answers every request with a fixed decision, or holds it unanswered when `None`, and
/// remembers what it was asked.
struct Fake {
    decision: Option<ApprovalDecision>,
    seen: Mutex<Vec<ApprovalRequest>>,
    held: Mutex<Vec<Reply>>,
}

impl Fake {
    fn new(decision: Option<ApprovalDecision>) -> Arc<Self> {
        Arc::new(Self {
            decision,
            seen: Mutex::new(Vec::new()),
            held: Mutex::new(Vec::new()),
        })
    }
}

impl ApprovalHandler for Fake {
    fn request(&self, request: ApprovalRequest, reply: Reply) {
        self.seen.lock().unwrap().push(request);
        match self.decision {
            Some(decision) => assert!(reply.send(decision)),
            None => self.held.lock().unwrap().push(reply),
        }
    }
}

fn request(id: u64) -> ApprovalRequest {
    ApprovalRequest {
        version: PROTOCOL_VERSION,
        id,
        tool: "request_execute".to_owned(),
        class: "execute".to_owned(),
        workspace: Some(WorkspaceRef {
            id: "01W".to_owned(),
            name: "Demo".to_owned(),
        }),
        targets: vec!["Ping".to_owned()],
        method: Some("GET".to_owned()),
        url: Some("https://example.test/?key=*****".to_owned()),
    }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_listener_is_owner_only_advertised_and_removed_on_drop() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().join("reqflux");
    let pid = std::process::id();
    let listener = serve_in(&dir, pid, Fake::new(None)).unwrap();
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(listener.socket(), socket_path(&dir, pid));
    assert_eq!(mode(listener.socket()), 0o600);
    assert_eq!(mode(listener.descriptor()), 0o600);
    let found = find_window(&dir).unwrap();
    assert_eq!(found.pid, pid);
    assert_eq!(found.socket, socket_path(&dir, pid));
    let (socket, descriptor) = (
        listener.socket().to_path_buf(),
        listener.descriptor().to_path_buf(),
    );
    drop(listener);
    assert!(!socket.exists());
    assert!(!descriptor.exists());
    assert_eq!(find_window(&dir), None);
}

#[test]
fn a_descriptor_of_a_dead_pid_or_another_version_is_ignored() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path();
    // Pid 0 is never a user process, and u32::MAX is above every pid limit.
    for (pid, version) in [(u32::MAX - 1, PROTOCOL_VERSION), (std::process::id(), 99)] {
        let path = descriptor_path(dir, pid);
        let descriptor = WindowDescriptor {
            version,
            pid,
            socket: socket_path(dir, pid),
        };
        std::fs::write(&path, serde_json::to_string(&descriptor).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(socket_path(dir, pid), "").unwrap();
    }
    assert_eq!(find_window(dir), None);
    assert_eq!(find_window(&dir.join("missing")), None);
}

#[test]
fn a_decision_round_trips_with_the_request_unchanged() {
    for decision in [
        ApprovalDecision::AllowOnce,
        ApprovalDecision::AllowSession,
        ApprovalDecision::Deny,
    ] {
        let temp = tempfile::TempDir::new().unwrap();
        let fake = Fake::new(Some(decision));
        let listener = serve_in(temp.path(), 1, fake.clone()).unwrap();
        let answer = ask(listener.socket(), &request(7), Duration::from_secs(5)).unwrap();
        assert_eq!(answer, decision);
        assert_eq!(fake.seen.lock().unwrap().as_slice(), &[request(7)]);
    }
}

#[test]
fn an_unanswered_request_times_out_and_a_dropped_reply_is_no_answer() {
    let temp = tempfile::TempDir::new().unwrap();
    let fake = Fake::new(None);
    let listener = serve_in(temp.path(), 1, fake.clone()).unwrap();
    let error = ask(listener.socket(), &request(1), Duration::from_millis(200)).unwrap_err();
    assert!(matches!(error, AskError::NoAnswer(_)), "{error}");
    drop(listener);

    let listener = serve_in(temp.path(), 2, Arc::new(Dropper)).unwrap();
    let error = ask(listener.socket(), &request(2), Duration::from_secs(5)).unwrap_err();
    assert!(matches!(error, AskError::NoAnswer(_)), "{error}");
}

/// Drops every reply unanswered, as a window that closes does.
struct Dropper;

impl ApprovalHandler for Dropper {
    fn request(&self, _request: ApprovalRequest, reply: Reply) {
        drop(reply);
    }
}

#[test]
fn a_closed_socket_is_unreachable() {
    let temp = tempfile::TempDir::new().unwrap();
    let error = ask(
        &temp.path().join("approvals-1.sock"),
        &request(1),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(matches!(error, AskError::Unreachable(_)), "{error}");
}

#[test]
fn the_wire_format_is_versioned_snake_case_json() {
    let response = ApprovalResponse {
        version: PROTOCOL_VERSION,
        id: 3,
        decision: ApprovalDecision::AllowSession,
    };
    assert_eq!(
        serde_json::to_string(&response).unwrap(),
        r#"{"version":1,"id":3,"decision":"allow_session"}"#
    );
    let mut bare = request(1);
    bare.method = None;
    bare.url = None;
    bare.workspace = None;
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"version":1,"id":1,"tool":"request_execute","class":"execute","workspace":null,"targets":["Ping"]}"#
    );
}
