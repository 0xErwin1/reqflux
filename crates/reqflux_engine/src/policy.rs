//! The MCP policy (user-config-mcp-policy S3-S6): which classes of tools an agent may run,
//! and which hosts and environments `request_execute` may use.
//!
//! [`Policy::evaluate`] combines the user's `[mcp]` (with the built-in defaults: read
//! `allow`, the other classes `ask`), `[mcp.workspaces."<workspace id>"]` and the open
//! workspace's manifest `[mcp]`. Every level can only restrict: a class takes the strictest
//! rule of every level (`deny` > `ask` > `allow`), and a host or environment must be in every
//! list given. Until in-app approvals exist, `ask` refuses. A refusal names the class or the
//! host or environment, the rule or list that refused it and the file and table it comes
//! from, followed by any warning about the files it was read from. A
//! `[mcp.workspaces]` key matches the workspace id in any case. Front ends that will ask the
//! user (a future GUI approval) reuse the same evaluation.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{McpConfig, McpOverride, Rule};

/// The execution class of a tool (S5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolClass {
    /// Reads the workspace or the history.
    Read,
    /// Creates or changes files.
    Write,
    /// Sends a request.
    Execute,
    /// Deletes files or secrets.
    Destructive,
}

impl ToolClass {
    /// The class name, as the config key and the audit log spell it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Execute => "execute",
            Self::Destructive => "destructive",
        }
    }

    fn with_article(self) -> &'static str {
        match self {
            Self::Read => "a read",
            Self::Write => "a write",
            Self::Execute => "an execute",
            Self::Destructive => "a destructive",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Write => 1,
            Self::Execute => 2,
            Self::Destructive => 3,
        }
    }
}

const CLASSES: [ToolClass; 4] = [
    ToolClass::Read,
    ToolClass::Write,
    ToolClass::Execute,
    ToolClass::Destructive,
];

/// The open workspace a policy is evaluated for.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceScope<'a> {
    /// The workspace id, the key of `[mcp.workspaces."<id>"]`.
    pub id: &'a str,
    /// The manifest's `[mcp]`, if it has one.
    pub manifest: Option<&'a McpOverride>,
    /// The manifest file, named in a refusal.
    pub manifest_path: &'a Path,
}

/// Where a rule or list comes from: a file and the table in it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    path: PathBuf,
    table: String,
}

impl Origin {
    /// `<path> (<table> <key>)`.
    fn describe(&self, key: &str) -> String {
        format!("{} ({} {key})", self.path.display(), self.table)
    }
}

/// A list of allowed hosts or environments and where it comes from.
#[derive(Debug, Clone)]
struct AllowList {
    entries: Vec<String>,
    origin: Origin,
}

impl AllowList {
    fn describe(&self, key: &str) -> String {
        let entries: Vec<String> = self
            .entries
            .iter()
            .map(|entry| format!("\"{entry}\""))
            .collect();
        format!(
            "{key} = [{}] in {}",
            entries.join(", "),
            self.origin.describe(key)
        )
    }
}

/// Why a call was refused, as the tool error shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    message: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// The effective policy for one call.
#[derive(Debug, Clone)]
pub struct Policy {
    rules: [(Rule, Origin); 4],
    hosts: Vec<AllowList>,
    environments: Vec<AllowList>,
    load_error: Option<String>,
    warnings: Vec<String>,
}

impl Policy {
    /// The policy of `config`, read from `config_path`, for `workspace` when one is open.
    #[must_use]
    pub fn evaluate(
        config: &McpConfig,
        config_path: &Path,
        workspace: Option<WorkspaceScope<'_>>,
    ) -> Self {
        let global = Origin {
            path: config_path.to_owned(),
            table: "[mcp]".to_owned(),
        };
        let defaults = [Rule::Allow, Rule::Ask, Rule::Ask, Rule::Ask];
        let given = [
            config.read,
            config.write,
            config.execute,
            config.destructive,
        ];
        let rules = CLASSES.map(|class| {
            let index = class.index();
            let rule = given
                .get(index)
                .copied()
                .flatten()
                .or_else(|| defaults.get(index).copied())
                .unwrap_or(Rule::Deny);
            (rule, global.clone())
        });
        let mut policy = Self {
            rules,
            hosts: Vec::new(),
            environments: Vec::new(),
            load_error: None,
            warnings: Vec::new(),
        };
        policy.restrict_lists(config.hosts.as_ref(), config.environments.as_ref(), &global);
        if let Some(scope) = workspace {
            if let Some((key, level)) = config.workspace(scope.id) {
                let origin = Origin {
                    path: config_path.to_owned(),
                    table: format!("[mcp.workspaces.\"{key}\"]"),
                };
                policy.restrict(level, &origin);
            }
            if let Some(level) = scope.manifest {
                let origin = Origin {
                    path: scope.manifest_path.to_owned(),
                    table: "[mcp]".to_owned(),
                };
                policy.restrict(level, &origin);
            }
        }
        policy
    }

    /// The same policy, with every refusal saying the config file failed to load and the
    /// defaults apply; `error` names the path, line and column.
    #[must_use]
    pub fn with_load_error(mut self, error: &str) -> Self {
        self.load_error = Some(error.to_owned());
        self
    }

    /// The same policy, with every refusal also naming `warnings` (unknown or invalid keys
    /// in the files it was read from). They change no rule or list.
    #[must_use]
    pub fn with_warnings(mut self, warnings: &[String]) -> Self {
        self.warnings.extend_from_slice(warnings);
        self
    }

    /// The effective rule of `class`.
    #[must_use]
    pub fn rule(&self, class: ToolClass) -> Rule {
        self.rules
            .get(class.index())
            .map_or(Rule::Deny, |(rule, _)| *rule)
    }

    /// Whether `tool`, of `class`, may run.
    ///
    /// # Errors
    ///
    /// The class's rule is `ask` or `deny`.
    pub fn check_class(&self, tool: &str, class: ToolClass) -> Result<(), Refusal> {
        let Some((rule, origin)) = self.rules.get(class.index()) else {
            return Err(self.refusal(format!("{tool} has no rule")));
        };
        let name = class.name();
        let head = format!(
            "{tool} is {} tool and {name} is \"{}\" in {}",
            class.with_article(),
            rule_name(*rule),
            origin.describe(name)
        );
        match rule {
            Rule::Allow => Ok(()),
            Rule::Ask => Err(self.refusal(format!(
                "{head}; in-app approvals do not exist yet, so ask refuses"
            ))),
            Rule::Deny => Err(self.refusal(head)),
        }
    }

    /// Whether `tool` may send to `url`, the prepared request URL: its host must match an
    /// entry of every `hosts` list given.
    ///
    /// # Errors
    ///
    /// A list does not hold the host, or a list is given and the URL has no host.
    pub fn check_host(&self, tool: &str, url: &str) -> Result<(), Refusal> {
        if self.hosts.is_empty() {
            return Ok(());
        }
        let parsed = url::Url::parse(url).ok();
        let host = parsed
            .as_ref()
            .and_then(url::Url::host_str)
            .map(str::to_ascii_lowercase);
        let Some(host) = host else {
            return Err(self.refusal(format!(
                "{tool} may not send to a URL without a host while hosts is set in {}",
                self.hosts
                    .first()
                    .map_or_else(String::new, |list| list.origin.describe("hosts"))
            )));
        };
        let port = parsed.as_ref().and_then(url::Url::port_or_known_default);
        let explicit = parsed.as_ref().and_then(url::Url::port);
        for list in &self.hosts {
            if !list
                .entries
                .iter()
                .any(|entry| host_matches(entry, &host, port))
            {
                let shown = match explicit {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                };
                return Err(self.refusal(format!(
                    "{tool} may not reach {shown}: it is not in {}",
                    list.describe("hosts")
                )));
            }
        }
        Ok(())
    }

    /// Whether `tool` may use the environment `slug`; no environment is always allowed.
    ///
    /// # Errors
    ///
    /// An `environments` list does not hold the slug.
    pub fn check_environment(&self, tool: &str, slug: Option<&str>) -> Result<(), Refusal> {
        let Some(slug) = slug else {
            return Ok(());
        };
        for list in &self.environments {
            if !list.entries.iter().any(|entry| entry == slug) {
                return Err(self.refusal(format!(
                    "{tool} may not use the environment {slug}: it is not in {}",
                    list.describe("environments")
                )));
            }
        }
        Ok(())
    }

    /// Applies one override level: each rule stricter than the current one replaces it, and
    /// each list given is one more list a host or environment must be in.
    fn restrict(&mut self, level: &McpOverride, origin: &Origin) {
        let given = [level.read, level.write, level.execute, level.destructive];
        for (current, rule) in self.rules.iter_mut().zip(given) {
            if let Some(rule) = rule
                && rule > current.0
            {
                *current = (rule, origin.clone());
            }
        }
        self.restrict_lists(level.hosts.as_ref(), level.environments.as_ref(), origin);
    }

    fn restrict_lists(
        &mut self,
        hosts: Option<&Vec<String>>,
        environments: Option<&Vec<String>>,
        origin: &Origin,
    ) {
        if let Some(hosts) = hosts {
            self.hosts.push(AllowList {
                entries: hosts.clone(),
                origin: origin.clone(),
            });
        }
        if let Some(environments) = environments {
            self.environments.push(AllowList {
                entries: environments.clone(),
                origin: origin.clone(),
            });
        }
    }

    fn refusal(&self, message: String) -> Refusal {
        let mut message = match &self.load_error {
            Some(error) => {
                format!("{message}; the config file failed to load, so the defaults apply: {error}")
            }
            None => message,
        };
        for warning in &self.warnings {
            message.push_str("; warning: ");
            message.push_str(warning);
        }
        Refusal { message }
    }
}

fn rule_name(rule: Rule) -> &'static str {
    match rule {
        Rule::Allow => "allow",
        Rule::Ask => "ask",
        Rule::Deny => "deny",
    }
}

/// Whether the hosts entry `entry` matches `host` (lowercase) on `port`: an exact host, or
/// `*.<domain>` for its subdomains (not the domain itself); an entry with a port matches that
/// port only, one without matches any port. Case-insensitive.
fn host_matches(entry: &str, host: &str, port: Option<u16>) -> bool {
    let entry = entry.trim().to_ascii_lowercase();
    let (pattern, entry_port) = split_port(&entry);
    if entry_port.is_some_and(|entry_port| Some(entry_port) != port) {
        return false;
    }
    match pattern.strip_prefix("*.") {
        Some(domain) => host
            .strip_suffix(domain)
            .and_then(|prefix| prefix.strip_suffix('.'))
            .is_some_and(|label| !label.is_empty()),
        None => host == pattern,
    }
}

/// `host[:port]`, with an IPv6 host in brackets. A port that is not a number is part of
/// the host, so the entry matches nothing.
fn split_port(entry: &str) -> (&str, Option<u16>) {
    let split = if entry.starts_with('[') {
        entry
            .find("]:")
            .and_then(|end| entry.get(..=end).zip(entry.get(end + 2..)))
    } else {
        entry
            .rsplit_once(':')
            .filter(|(host, _)| !host.contains(':'))
    };
    match split {
        Some((host, port)) => match port.parse() {
            Ok(port) => (host, Some(port)),
            Err(_) => (entry, None),
        },
        None => (entry, None),
    }
}

#[cfg(test)]
mod tests;
