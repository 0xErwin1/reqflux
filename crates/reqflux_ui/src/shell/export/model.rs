//! What the Export… dialog and its toasts say, as design/export/01-one-export-dialog writes
//! it: the scope, the formats it offers, the suggested file names and the sentences that
//! name secrets. Nothing here reads a secret value; only variable names reach a sentence.

use super::super::toasts::counted;

/// What the dialog exports, from where it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The whole workspace, from Export workspace…: a bundle only.
    Workspace {
        name: String,
    },
    Collection {
        id: String,
        name: String,
    },
    Folder {
        id: String,
        name: String,
    },
    Request {
        id: String,
        name: String,
    },
    /// An environment, from its menu: a Postman environment only.
    Environment {
        id: String,
        name: String,
    },
}

impl Scope {
    /// The name the title shows.
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Workspace { name }
            | Self::Collection { name, .. }
            | Self::Folder { name, .. }
            | Self::Request { name, .. }
            | Self::Environment { name, .. } => name,
        }
    }

    /// The selector the engine takes; `None` is the whole workspace.
    pub(crate) fn selector(&self) -> Option<&str> {
        match self {
            Self::Workspace { .. } => None,
            Self::Collection { id, .. }
            | Self::Folder { id, .. }
            | Self::Request { id, .. }
            | Self::Environment { id, .. } => Some(id),
        }
    }

    /// What the title calls the scope after its name.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Workspace { .. } => "whole workspace",
            Self::Collection { .. } => "collection",
            Self::Folder { .. } => "folder",
            Self::Request { .. } => "request",
            Self::Environment { .. } => "environment",
        }
    }

    /// The dialog title: `Export Billing API · collection`.
    pub(crate) fn title(&self) -> String {
        format!("Export {} · {}", self.name(), self.kind())
    }

    /// The formats the scope can be exported as, in the control's order.
    pub(crate) fn formats(&self) -> &'static [Format] {
        match self {
            Self::Workspace { .. } => &[Format::Bundle],
            Self::Environment { .. } => &[Format::Postman],
            Self::Collection { .. } => &[Format::Postman, Format::Bundle, Format::Docs],
            Self::Folder { .. } | Self::Request { .. } => &[Format::Postman, Format::Bundle],
        }
    }

    /// The format the dialog opens on: the first it offers.
    pub(crate) fn default_format(&self) -> Format {
        self.formats().first().copied().unwrap_or(Format::Bundle)
    }

    /// Where the secrets the bundle counts live, for `Secrets · 3 in this collection`.
    pub(crate) fn place(&self) -> &'static str {
        match self {
            Self::Workspace { .. } => "this workspace",
            Self::Collection { .. } => "this collection",
            Self::Folder { .. } => "this folder",
            Self::Request { .. } => "this request",
            Self::Environment { .. } => "this environment",
        }
    }
}

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Postman,
    Bundle,
    Docs,
}

impl Format {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Postman => "postman",
            Self::Bundle => "bundle",
            Self::Docs => "docs",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Postman => "Postman v2.1",
            Self::Bundle => "ReqFlux bundle",
            Self::Docs => "API docs",
        }
    }

    pub(crate) fn from_slug(slug: &str) -> Option<Self> {
        [Self::Postman, Self::Bundle, Self::Docs]
            .into_iter()
            .find(|format| format.slug() == slug)
    }
}

/// Why the passphrases were refused, checked before the save dialog opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassphraseError {
    Empty,
    Mismatch,
}

impl PassphraseError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Empty => "Enter a passphrase to encrypt the secrets.",
            Self::Mismatch => "Passphrases do not match.",
        }
    }
}

/// Checks the passphrase and its confirmation.
pub(crate) fn check_passphrase(passphrase: &str, confirm: &str) -> Result<(), PassphraseError> {
    if passphrase.is_empty() {
        Err(PassphraseError::Empty)
    } else if passphrase != confirm {
        Err(PassphraseError::Mismatch)
    } else {
        Ok(())
    }
}

/// `name` as a file stem: lowercase words joined by dashes, `export` when nothing is left.
pub(crate) fn file_stem(name: &str) -> String {
    let stem = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    if stem.is_empty() {
        "export".to_owned()
    } else {
        stem
    }
}

/// The name the save dialog suggests for `scope` as `format`.
pub(crate) fn suggested_name(scope: &Scope, format: Format) -> String {
    let stem = file_stem(scope.name());
    match (format, scope) {
        (Format::Postman, Scope::Environment { .. }) => format!("{stem}.postman_environment.json"),
        (Format::Postman, _) => format!("{stem}.postman_collection.json"),
        (Format::Bundle | Format::Docs, _) => format!("{stem}.reqflux"),
    }
}

/// `names` as a sentence lists them: `a`, `a and b`, `a, b and c`.
pub(crate) fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [name] => name.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The note under Include secret values while it is off.
pub(crate) fn secrets_off_note(secrets: &[String], environment: bool) -> String {
    match secrets {
        [] => "No secret variables.".to_owned(),
        [name] if environment => format!("Off: {name} is exported with an empty value."),
        names if environment => format!("Off: {} are exported with empty values.", listed(names)),
        names => format!(
            "Off: {} are exported with empty values.",
            counted(names.len(), "secret")
        ),
    }
}

/// The note under Include secret values while it is on.
pub(crate) fn secrets_on_note(secrets: &[String]) -> String {
    format!("{}: {}.", counted(secrets.len(), "secret"), listed(secrets))
}

/// The warning shown while Include secret values is on.
pub(crate) fn plaintext_warning(secrets: &[String]) -> String {
    format!(
        "Secrets are written in plain text. Anyone who can read the file can read {}. The \
         file is saved readable only by you (mode 0600); Postman keeps them unencrypted too.",
        listed(secrets)
    )
}

/// The warning over a bundle that leaves secrets out.
pub(crate) fn omitted_warning(secrets: &[String]) -> String {
    format!(
        "{} will be empty in the bundle: {}.",
        counted(secrets.len(), "secret"),
        listed(secrets)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn file_names_follow_the_artboard() {
        let collection = Scope::Collection {
            id: String::new(),
            name: "Billing API".to_owned(),
        };
        let environment = Scope::Environment {
            id: String::new(),
            name: "staging".to_owned(),
        };
        assert_eq!(
            suggested_name(&collection, Format::Postman),
            "billing-api.postman_collection.json"
        );
        assert_eq!(
            suggested_name(&collection, Format::Bundle),
            "billing-api.reqflux"
        );
        assert_eq!(
            suggested_name(&environment, Format::Postman),
            "staging.postman_environment.json"
        );
        assert_eq!(file_stem("  "), "export");
    }

    #[test]
    fn names_read_as_a_sentence() {
        assert_eq!(listed(&names(&["a"])), "a");
        assert_eq!(listed(&names(&["a", "b"])), "a and b");
        assert_eq!(listed(&names(&["a", "b", "c"])), "a, b and c");
    }

    #[test]
    fn an_empty_passphrase_is_refused_before_a_mismatch() {
        assert_eq!(check_passphrase("", "x"), Err(PassphraseError::Empty));
        assert_eq!(check_passphrase("a", "b"), Err(PassphraseError::Mismatch));
        assert_eq!(check_passphrase("a", "a"), Ok(()));
    }
}
