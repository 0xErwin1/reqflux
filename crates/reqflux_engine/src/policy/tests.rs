#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use super::*;
use crate::config::{McpConfig, McpOverride, Rule};

const WORKSPACE_ID: &str = "01J0000000000000000000000W";

fn config_path() -> PathBuf {
    PathBuf::from("/home/u/.config/reqflux/config.toml")
}

fn manifest_path() -> PathBuf {
    PathBuf::from("/ws/reqflux.toml")
}

fn config(text: &str) -> McpConfig {
    let parsed: crate::config::UserConfig = toml::from_str(text).unwrap();
    parsed.mcp
}

fn manifest(text: &str) -> McpOverride {
    toml::from_str(text).unwrap()
}

fn global(text: &str) -> Policy {
    Policy::evaluate(&config(text), &config_path(), None)
}

fn scoped(text: &str, manifest_mcp: Option<&McpOverride>) -> Policy {
    let path = manifest_path();
    Policy::evaluate(
        &config(text),
        &config_path(),
        Some(WorkspaceScope {
            id: WORKSPACE_ID,
            manifest: manifest_mcp,
            manifest_path: &path,
        }),
    )
}

fn refusal(result: Result<(), Refusal>) -> String {
    match result {
        Ok(()) => panic!("expected a refusal"),
        Err(refusal) => refusal.to_string(),
    }
}

#[test]
fn the_defaults_allow_read_and_ask_for_the_rest() {
    let policy = global("");
    assert_eq!(policy.rule(ToolClass::Read), Rule::Allow);
    assert_eq!(policy.rule(ToolClass::Write), Rule::Ask);
    assert_eq!(policy.rule(ToolClass::Execute), Rule::Ask);
    assert_eq!(policy.rule(ToolClass::Destructive), Rule::Ask);
    assert!(policy.check_class("request_get", ToolClass::Read).is_ok());
}

#[test]
fn ask_refuses_naming_the_class_the_file_and_the_key() {
    let message = refusal(global("").check_class("request_execute", ToolClass::Execute));
    assert_eq!(
        message,
        "request_execute is an execute tool and execute is \"ask\" in \
         /home/u/.config/reqflux/config.toml ([mcp] execute); ask needs an approval in the \
         ReqFlux window"
    );
}

#[test]
fn deny_refuses_without_the_approvals_sentence() {
    let message = refusal(
        global("[mcp]\ndestructive = \"deny\"\n")
            .check_class("item_delete", ToolClass::Destructive),
    );
    assert_eq!(
        message,
        "item_delete is a destructive tool and destructive is \"deny\" in \
         /home/u/.config/reqflux/config.toml ([mcp] destructive)"
    );
}

#[test]
fn an_allowed_class_runs() {
    let policy = global("[mcp]\nwrite = \"allow\"\nexecute = \"allow\"\n");
    assert!(policy.check_class("item_rename", ToolClass::Write).is_ok());
    assert!(
        policy
            .check_class("request_execute", ToolClass::Execute)
            .is_ok()
    );
    assert!(
        policy
            .check_class("item_delete", ToolClass::Destructive)
            .is_err()
    );
}

#[test]
fn the_strictest_level_wins_and_names_its_table() {
    let text = format!(
        "[mcp]\nwrite = \"allow\"\nexecute = \"allow\"\n\
         [mcp.workspaces.\"{WORKSPACE_ID}\"]\nwrite = \"deny\"\n"
    );
    let policy = scoped(&text, None);
    assert_eq!(policy.rule(ToolClass::Write), Rule::Deny);
    assert_eq!(policy.rule(ToolClass::Execute), Rule::Allow);
    assert_eq!(
        refusal(policy.check_class("item_rename", ToolClass::Write)),
        format!(
            "item_rename is a write tool and write is \"deny\" in \
             /home/u/.config/reqflux/config.toml ([mcp.workspaces.\"{WORKSPACE_ID}\"] write)"
        )
    );
}

#[test]
fn a_workspace_override_cannot_loosen() {
    let text = format!(
        "[mcp]\nexecute = \"deny\"\n[mcp.workspaces.\"{WORKSPACE_ID}\"]\nexecute = \"allow\"\n\
         write = \"allow\"\n"
    );
    let policy = scoped(&text, None);
    assert_eq!(policy.rule(ToolClass::Execute), Rule::Deny);
    assert_eq!(
        policy.rule(ToolClass::Write),
        Rule::Ask,
        "the default stays"
    );
}

#[test]
fn another_workspaces_override_does_not_apply() {
    let text = "[mcp]\nwrite = \"allow\"\n[mcp.workspaces.\"other\"]\nwrite = \"deny\"\n";
    assert_eq!(scoped(text, None).rule(ToolClass::Write), Rule::Allow);
}

#[test]
fn a_manifest_cannot_loosen_and_its_refusal_names_the_manifest() {
    let allow_all = manifest("read = \"allow\"\nwrite = \"allow\"\ndestructive = \"allow\"\n");
    let policy = scoped("", Some(&allow_all));
    assert_eq!(policy.rule(ToolClass::Write), Rule::Ask);
    assert_eq!(policy.rule(ToolClass::Destructive), Rule::Ask);

    let deny = manifest("execute = \"deny\"\n");
    let policy = scoped("[mcp]\nexecute = \"allow\"\n", Some(&deny));
    assert_eq!(
        refusal(policy.check_class("request_execute", ToolClass::Execute)),
        "request_execute is an execute tool and execute is \"deny\" in /ws/reqflux.toml \
         ([mcp] execute)"
    );
}

#[test]
fn a_tie_names_the_users_file() {
    let deny = manifest("execute = \"deny\"\n");
    let policy = scoped("[mcp]\nexecute = \"deny\"\n", Some(&deny));
    assert!(
        refusal(policy.check_class("request_execute", ToolClass::Execute))
            .contains("config.toml ([mcp] execute)")
    );
}

#[test]
fn read_can_be_restricted_too() {
    let deny = manifest("read = \"deny\"\n");
    let policy = scoped("", Some(&deny));
    assert!(policy.check_class("request_get", ToolClass::Read).is_err());
}

#[test]
fn a_load_error_is_named_in_every_refusal() {
    let policy = global("").with_load_error("the config file /c does not parse: line 2, column 3");
    let message = refusal(policy.check_class("request_execute", ToolClass::Execute));
    assert!(
        message.starts_with("request_execute is an execute tool"),
        "{message}"
    );
    assert!(
        message.ends_with(
            "; the config file failed to load, so the defaults apply: the config file /c does \
             not parse: line 2, column 3"
        ),
        "{message}"
    );
}

fn hosts_policy(hosts: &str) -> Policy {
    global(&format!("[mcp]\nhosts = {hosts}\n"))
}

#[test]
fn host_matching() {
    let policy = hosts_policy(
        "[\"api.example.com\", \"*.Corp.test\", \"localhost:8080\", \"[::1]:9000\", \"10.0.0.1\"]",
    );
    let cases = [
        ("https://api.example.com/x", true),
        ("https://API.Example.COM:8443/x", true),
        ("http://api.example.com:1/x", true),
        ("https://example.com/x", false),
        ("https://www.api.example.com/x", false),
        ("https://a.corp.test/x", true),
        ("https://b.a.CORP.test/x", true),
        ("https://corp.test/x", false),
        ("https://evilcorp.test/x", false),
        ("http://localhost:8080/x", true),
        ("http://localhost:8081/x", false),
        ("http://localhost/x", false),
        ("http://[::1]:9000/x", true),
        ("http://[::1]:9001/x", false),
        ("http://10.0.0.1:5/x", true),
        ("not a url", false),
    ];
    for (url, allowed) in cases {
        assert_eq!(
            policy.check_host("request_execute", url).is_ok(),
            allowed,
            "{url}"
        );
    }
}

#[test]
fn an_entry_with_a_default_port_matches_a_url_without_one() {
    let policy = hosts_policy("[\"api.example.com:443\"]");
    assert!(
        policy
            .check_host("request_execute", "https://api.example.com/x")
            .is_ok()
    );
    assert!(
        policy
            .check_host("request_execute", "http://api.example.com/x")
            .is_err()
    );
}

#[test]
fn no_hosts_list_allows_any_host() {
    assert!(
        global("")
            .check_host("request_execute", "https://anything.test/")
            .is_ok()
    );
}

#[test]
fn a_host_refusal_names_the_host_the_list_and_its_origin() {
    let message = refusal(
        hosts_policy("[\"api.example.com\"]")
            .check_host("request_execute", "https://evil.test:8443/x"),
    );
    assert_eq!(
        message,
        "request_execute may not reach evil.test:8443: it is not in hosts = \
         [\"api.example.com\"] in /home/u/.config/reqflux/config.toml ([mcp] hosts)"
    );
}

#[test]
fn host_lists_intersect() {
    let narrow = manifest("hosts = [\"a.example.com\"]\n");
    let policy = scoped("[mcp]\nhosts = [\"*.example.com\"]\n", Some(&narrow));
    assert!(
        policy
            .check_host("request_execute", "https://a.example.com/")
            .is_ok()
    );
    let message = refusal(policy.check_host("request_execute", "https://b.example.com/"));
    assert!(
        message.ends_with("hosts = [\"a.example.com\"] in /ws/reqflux.toml ([mcp] hosts)"),
        "{message}"
    );
    // A manifest list cannot add a host the user's list lacks.
    let wide = manifest("hosts = [\"evil.test\", \"a.example.com\"]\n");
    let policy = scoped("[mcp]\nhosts = [\"a.example.com\"]\n", Some(&wide));
    assert!(
        policy
            .check_host("request_execute", "https://evil.test/")
            .is_err()
    );
}

#[test]
fn an_empty_hosts_list_allows_no_host() {
    assert!(
        hosts_policy("[]")
            .check_host("request_execute", "https://api.example.com/")
            .is_err()
    );
}

#[test]
fn environment_lists_intersect_and_no_environment_is_always_allowed() {
    let text = format!(
        "[mcp]\nenvironments = [\"dev\", \"staging\"]\n\
         [mcp.workspaces.\"{WORKSPACE_ID}\"]\nenvironments = [\"dev\", \"prod\"]\n"
    );
    let policy = scoped(&text, None);
    assert!(
        policy
            .check_environment("request_execute", Some("dev"))
            .is_ok()
    );
    assert!(policy.check_environment("request_execute", None).is_ok());
    assert!(
        policy
            .check_environment("request_execute", Some("prod"))
            .is_err()
    );
    assert_eq!(
        refusal(policy.check_environment("request_execute", Some("staging"))),
        format!(
            "request_execute may not use the environment staging: it is not in environments = \
             [\"dev\", \"prod\"] in /home/u/.config/reqflux/config.toml \
             ([mcp.workspaces.\"{WORKSPACE_ID}\"] environments)"
        )
    );
    assert!(
        global("")
            .check_environment("request_execute", Some("anything"))
            .is_ok()
    );
}

#[test]
fn a_workspace_override_matches_its_id_in_any_case() {
    let key = WORKSPACE_ID.to_ascii_lowercase();
    let policy = scoped(
        &format!("[mcp.workspaces.\"{key}\"]\nread = \"deny\"\n"),
        None,
    );
    assert_eq!(policy.rule(ToolClass::Read), Rule::Deny);
    let message = refusal(policy.check_class("request_get", ToolClass::Read));
    assert!(
        message.ends_with(&format!("([mcp.workspaces.\"{key}\"] read)")),
        "{message}"
    );
}

#[test]
fn warnings_are_named_in_every_refusal_and_change_no_rule() {
    let policy =
        global("").with_warnings(&["/c: unknown key `host` in [mcp]; it is ignored".to_owned()]);
    assert_eq!(policy.rule(ToolClass::Read), Rule::Allow);
    assert_eq!(policy.rule(ToolClass::Write), Rule::Ask);
    let message = refusal(policy.check_class("item_rename", ToolClass::Write));
    assert!(
        message.ends_with("; warning: /c: unknown key `host` in [mcp]; it is ignored"),
        "{message}"
    );
}
