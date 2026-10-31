//! The `[mcp]` restrictions a workspace manifest and the user's config file share
//! (user-config-mcp-policy S3, S4). This module only holds the parsed values; evaluating them
//! is the engine's job.

use serde::{Deserialize, Serialize};

/// What an agent may do with one execution class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    /// The call runs.
    Allow,
    /// The call needs the user's approval; until approvals exist it is refused.
    Ask,
    /// The call is refused.
    Deny,
}

/// A set of MCP restrictions: `[mcp]` in a workspace's `reqflux.toml` and
/// `[mcp.workspaces."<workspace id>"]` in the user's config file. `None` leaves the rule or
/// list to the other levels.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McpOverride {
    /// The rule for read tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Rule>,
    /// The rule for write tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Rule>,
    /// The rule for execute tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execute: Option<Rule>,
    /// The rule for destructive tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive: Option<Rule>,
    /// The hosts `request_execute` may reach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<Vec<String>>,
    /// The environment slugs `request_execute` may use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<Vec<String>>,
    /// Keys this version of ReqFlux does not know, kept on write.
    #[serde(flatten)]
    pub unknown: toml::Table,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WorkspaceManifest;

    const ID: &str = "01J0000000000000000000000A";

    fn manifest(text: &str) -> WorkspaceManifest {
        match toml::from_str(text) {
            Ok(manifest) => manifest,
            Err(error) => panic!("parse: {error}"),
        }
    }

    fn written(manifest: &WorkspaceManifest) -> String {
        match toml::to_string(manifest) {
            Ok(text) => text,
            Err(error) => panic!("serialize: {error}"),
        }
    }

    #[test]
    fn a_manifest_mcp_table_parses_and_round_trips() {
        let text = format!(
            "id = \"{ID}\"\nname = \"Billing\"\nformat = 1\n\n[[globals]]\nkey = \"host\"\nvalue = \"api.test\"\n\n[mcp]\nwrite = \"deny\"\nexecute = \"ask\"\nhosts = [\"api.example.com\", \"*.example.com\"]\nenvironments = [\"dev\"]\nlater = true\n\n[extensions]\nzeta = 1\n"
        );
        let parsed = manifest(&text);
        let mut unknown = toml::Table::new();
        unknown.insert("later".to_owned(), toml::Value::Boolean(true));
        assert_eq!(
            parsed.mcp,
            Some(McpOverride {
                read: None,
                write: Some(Rule::Deny),
                execute: Some(Rule::Ask),
                destructive: None,
                hosts: Some(vec![
                    "api.example.com".to_owned(),
                    "*.example.com".to_owned()
                ]),
                environments: Some(vec!["dev".to_owned()]),
                unknown,
            })
        );
        assert!(!parsed.extensions.contains_key("mcp"));
        assert_eq!(written(&parsed), text);
    }

    #[test]
    fn a_manifest_without_mcp_round_trips_unchanged() {
        let text =
            format!("id = \"{ID}\"\nname = \"Billing\"\nformat = 1\n\n[extensions]\nzeta = 1\n");
        let parsed = manifest(&text);
        assert_eq!(parsed.mcp, None);
        assert_eq!(written(&parsed), text);
    }

    #[test]
    fn an_unknown_rule_does_not_parse() {
        let text = format!("id = \"{ID}\"\nname = \"B\"\n\n[mcp]\nread = \"maybe\"\n");
        assert!(toml::from_str::<WorkspaceManifest>(&text).is_err());
    }
}
