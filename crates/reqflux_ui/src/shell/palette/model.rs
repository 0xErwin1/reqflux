//! What the palette lists, without GPUI (design-catch-up T9): the scope a query prefix picks,
//! the sections of rows under their headers and the count of each scope.

use reqflux_core::model::{Variable, Workspace};

use super::super::commands::{COMMANDS, CommandId};
use super::super::palette_match::{RequestEntry, score};

/// The most rows a section lists.
const MAX_ROWS: usize = 50;

/// What the palette searches: everything or one kind of row, picked by a query prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    All,
    Requests,
    Commands,
    Variables,
}

impl Scope {
    /// The scopes in tab order.
    pub(crate) const ORDER: [Scope; 4] = [
        Scope::All,
        Scope::Requests,
        Scope::Commands,
        Scope::Variables,
    ];

    /// The query prefix that picks the scope; All has none.
    pub(crate) fn prefix(self) -> Option<char> {
        match self {
            Self::All => None,
            Self::Requests => Some('@'),
            Self::Commands => Some('>'),
            Self::Variables => Some('$'),
        }
    }

    /// The tab title.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Requests => "Requests",
            Self::Commands => "Commands",
            Self::Variables => "Variables",
        }
    }

    /// The lowercase plural the counts and the empty states use.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::All => "items",
            Self::Requests => "requests",
            Self::Commands => "commands",
            Self::Variables => "variables",
        }
    }

    /// The element id suffix of the scope's tab.
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Requests => "requests",
            Self::Commands => "commands",
            Self::Variables => "variables",
        }
    }

    /// The scope after this one, or before it, wrapping around.
    pub(crate) fn step(self, forward: bool) -> Scope {
        let index = Self::ORDER
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or_default();
        let count = Self::ORDER.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        Self::ORDER.get(next).copied().unwrap_or(Scope::All)
    }

    /// `text` with this scope's prefix in front.
    pub(crate) fn query(self, text: &str) -> String {
        match self.prefix() {
            Some(prefix) => format!("{prefix}{text}"),
            None => text.to_owned(),
        }
    }
}

/// The scope `query` picks with its prefix, and the text after the prefix.
pub(crate) fn parse(query: &str) -> (Scope, &str) {
    for scope in Scope::ORDER {
        if let Some(rest) = scope.prefix().and_then(|prefix| query.strip_prefix(prefix)) {
            return (scope, rest);
        }
    }
    (Scope::All, query)
}

/// The value a variable row shows. A secret is always [`VariableValue::Secret`], whatever
/// the variable holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VariableValue {
    Secret,
    Empty,
    Plain(String),
}

/// A variable as the palette lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariableEntry {
    pub(crate) key: String,
    /// Where the variable is declared: the environment name, `<name> collection` or `global`.
    pub(crate) scope: String,
    pub(crate) value: VariableValue,
}

impl VariableEntry {
    /// `{{key}}`, the row's title.
    pub(crate) fn title(&self) -> String {
        format!("{{{{{}}}}}", self.key)
    }

    /// The value as the automation tree reads it; a secret never shows.
    pub(crate) fn value_label(&self) -> String {
        match &self.value {
            VariableValue::Secret => "secret".to_owned(),
            VariableValue::Empty => "empty".to_owned(),
            VariableValue::Plain(value) => value.clone(),
        }
    }
}

/// A group of variables under one header: the active environment, a collection or the
/// globals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariableGroup {
    pub(crate) header: Header,
    pub(crate) variables: Vec<VariableEntry>,
}

/// The enabled variables of the workspace as the palette groups them: the active
/// environment `active` (a slug) first, then the collections in workspace order, then the
/// globals. Other environments do not resolve, so they are left out.
pub(crate) fn variables(workspace: &Workspace, active: Option<&str>) -> Vec<VariableGroup> {
    let mut groups = Vec::new();
    if let Some(node) =
        active.and_then(|slug| workspace.environments.iter().find(|node| node.slug == slug))
    {
        let name = node.environment.name.clone();
        groups.push(VariableGroup {
            header: Header::new("environment", name.clone()).hint("active environment"),
            variables: entries(&node.environment.variables, &name),
        });
    }
    for (index, node) in workspace.collections.iter().enumerate() {
        let scope = format!("{} collection", node.collection.name);
        groups.push(VariableGroup {
            header: Header::new(format!("collection-{}", index + 1), scope.clone()),
            variables: entries(&node.collection.variables, &scope),
        });
    }
    groups.push(VariableGroup {
        header: Header::new("global", "Global"),
        variables: entries(&workspace.manifest.globals, "global"),
    });
    groups
}

fn entries(variables: &[Variable], scope: &str) -> Vec<VariableEntry> {
    variables
        .iter()
        .filter(|variable| variable.enabled)
        .map(|variable| VariableEntry {
            key: variable.key.clone(),
            scope: scope.to_owned(),
            value: if variable.secret {
                VariableValue::Secret
            } else {
                match variable.value.as_deref() {
                    None | Some("") => VariableValue::Empty,
                    Some(value) => VariableValue::Plain(value.to_owned()),
                }
            },
        })
        .collect()
}

/// A row of the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaletteRow {
    /// Opens the request.
    Request(RequestEntry),
    /// Runs the command.
    Command(CommandId),
    /// Shows the Variables panel.
    Variable(VariableEntry),
    /// Leaves the scope and searches everything for the text.
    SearchEverything(String),
}

/// A section header: element id `palette-group-<slug>`, its title and an optional hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) slug: String,
    pub(crate) title: String,
    pub(crate) hint: Option<String>,
}

impl Header {
    fn new(slug: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            title: title.into(),
            hint: None,
        }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Rows under a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) header: Option<Header>,
    pub(crate) rows: Vec<PaletteRow>,
}

/// How many rows each scope holds for a query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) requests: usize,
    pub(crate) commands: usize,
    pub(crate) variables: usize,
}

impl Counts {
    /// The count of `scope`.
    pub(crate) fn of(self, scope: Scope) -> usize {
        match scope {
            Scope::All => self.requests + self.commands + self.variables,
            Scope::Requests => self.requests,
            Scope::Commands => self.commands,
            Scope::Variables => self.variables,
        }
    }
}

/// What the palette shows for a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) scope: Scope,
    /// The query without its prefix, trimmed.
    pub(crate) text: String,
    pub(crate) sections: Vec<Section>,
    /// What matches the text in each scope.
    pub(crate) matched: Counts,
    /// What each scope holds.
    pub(crate) total: Counts,
}

impl Listing {
    /// Every row, in order, as the highlight counts them.
    pub(crate) fn rows(&self) -> Vec<PaletteRow> {
        self.sections
            .iter()
            .flat_map(|section| section.rows.iter().cloned())
            .collect()
    }

    /// Whether nothing of the scope is listed: the empty or no-match state shows.
    pub(crate) fn is_empty(&self) -> bool {
        self.matched.of(self.scope) == 0
    }

    /// The count by the query field: `19 commands` without a text, `matched / total` with
    /// one.
    pub(crate) fn count_text(&self) -> String {
        if self.text.is_empty() {
            format!("{} {}", self.total.of(self.scope), self.scope.noun())
        } else {
            format!(
                "{} / {}",
                self.matched.of(self.scope),
                self.total.of(self.scope)
            )
        }
    }
}

/// The listing for `query` over `requests` and the variable `groups`.
///
/// Without a text, All lists every command as Commands does, grouped by area; Requests and
/// Variables list everything they hold. With a text, each kind lists what matches, best
/// first, ties in list order; All shows them under Requests, Commands and Variables.
/// A scope with nothing that matches offers to search everything instead.
pub(crate) fn listing(
    query: &str,
    requests: Vec<RequestEntry>,
    groups: Vec<VariableGroup>,
) -> Listing {
    let (scope, text) = parse(query);
    let text = text.trim().to_owned();
    let total = Counts {
        requests: requests.len(),
        commands: COMMANDS.len(),
        variables: groups.iter().map(|group| group.variables.len()).sum(),
    };
    let found_requests = ranked(requests, |entry| entry.score(&text), PaletteRow::Request);
    let found_commands = ranked(
        COMMANDS.iter().map(|command| command.id).collect(),
        |id| {
            COMMANDS
                .iter()
                .find(|command| command.id == *id)
                .and_then(|command| score(&text, command.label))
        },
        PaletteRow::Command,
    );
    let found_groups: Vec<(Header, Vec<PaletteRow>)> = groups
        .into_iter()
        .map(|group| {
            let rows = ranked(
                group.variables,
                |entry| score(&text, &entry.key),
                PaletteRow::Variable,
            );
            (group.header, rows)
        })
        .collect();
    let matched = Counts {
        requests: found_requests.len(),
        commands: found_commands.len(),
        variables: found_groups.iter().map(|(_, rows)| rows.len()).sum(),
    };
    let mut sections = Vec::new();
    let only = |scope: Scope| Header::new(scope.slug(), scope.title());
    match scope {
        _ if text.is_empty() && matches!(scope, Scope::All | Scope::Commands) => {
            sections = by_area(found_commands);
        }
        Scope::All => {
            for (kind, rows) in [
                (Scope::Requests, found_requests),
                (Scope::Commands, found_commands),
                (Scope::Variables, flatten(found_groups)),
            ] {
                if let Some(prefix) = kind.prefix().filter(|_| !rows.is_empty()) {
                    sections.push(Section {
                        header: Some(only(kind).hint(format!("{prefix} only {}", kind.noun()))),
                        rows,
                    });
                }
            }
        }
        Scope::Requests => sections.push(Section {
            header: Some(only(Scope::Requests)),
            rows: found_requests,
        }),
        Scope::Commands => sections.push(Section {
            header: Some(only(Scope::Commands)),
            rows: found_commands,
        }),
        Scope::Variables => {
            sections = found_groups
                .into_iter()
                .filter(|(_, rows)| !rows.is_empty())
                .map(|(header, rows)| Section {
                    header: Some(header),
                    rows,
                })
                .collect();
        }
    }
    sections.retain(|section| !section.rows.is_empty());
    if sections.is_empty() && scope != Scope::All && !text.is_empty() {
        sections.push(Section {
            header: Some(Header::new("instead", "Instead")),
            rows: vec![PaletteRow::SearchEverything(text.clone())],
        });
    }
    Listing {
        scope,
        text,
        sections,
        matched,
        total,
    }
}

/// The items `score` matches, best first, ties in list order, at most [`MAX_ROWS`].
fn ranked<T>(
    items: Vec<T>,
    score: impl Fn(&T) -> Option<u32>,
    row: impl Fn(T) -> PaletteRow,
) -> Vec<PaletteRow> {
    let mut scored: Vec<(u32, T)> = items
        .into_iter()
        .filter_map(|item| Some((score(&item)?, item)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MAX_ROWS)
        .map(|(_, item)| row(item))
        .collect()
}

fn flatten(groups: Vec<(Header, Vec<PaletteRow>)>) -> Vec<PaletteRow> {
    let mut rows: Vec<PaletteRow> = groups.into_iter().flat_map(|(_, rows)| rows).collect();
    rows.truncate(MAX_ROWS);
    rows
}

/// The command `rows` grouped by area, areas in their order, commands in registry order.
fn by_area(rows: Vec<PaletteRow>) -> Vec<Section> {
    let mut commands: Vec<_> = rows
        .into_iter()
        .filter_map(|row| match row {
            PaletteRow::Command(id) => COMMANDS
                .iter()
                .find(|command| command.id == id)
                .map(|command| (command.area, id)),
            _ => None,
        })
        .collect();
    commands.sort_by_key(|(area, _)| *area);
    let mut sections: Vec<Section> = Vec::new();
    let mut current = None;
    for (area, id) in commands {
        if current != Some(area) {
            current = Some(area);
            sections.push(Section {
                header: Some(Header::new(area.slug(), area.title())),
                rows: Vec::new(),
            });
        }
        if let Some(section) = sections.last_mut() {
            section.rows.push(PaletteRow::Command(id));
        }
    }
    sections
}
