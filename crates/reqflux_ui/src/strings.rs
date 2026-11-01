//! The words `bolt`'s surfaces draw (shared-bolt S1): `bolt` reads every text through its
//! seam, `bolt::strings`, and draws nothing until a product installs a translator. ReqFlux
//! answers in English from [`CATALOG`]; [`install`] runs with the theme. A key the catalog
//! does not have answers the key itself, so a text `bolt` gains later shows up as its key
//! instead of rendering blank.

use gpui::{App, SharedString};

/// Every key `bolt` asks for and its English text; `%{name}` is replaced by the argument
/// `name`.
const CATALOG: &[(&str, &str)] = &[
    ("controls.dropdown.placeholder", "Select"),
    ("controls.multi_select.placeholder", "Select…"),
    ("controls.multi_select.clear_all", "Clear all"),
    ("controls.multi_select.summary_all", "%{name} · all"),
    ("controls.multi_select.summary_count", "%{name} · %{count}"),
    ("controls.multi_select.more.one", "+%{count} more"),
    ("controls.multi_select.more.many", "+%{count} more"),
    ("toast.action.dismiss", "Dismiss"),
    ("toast.action.show_details", "Show details"),
    ("toast.action.hide_details", "Hide details"),
    ("toast.stack.hidden_more", "%{count} more"),
    ("palette.search.placeholder", "Search commands…"),
    ("palette.empty", "No matching items"),
    ("palette.footer.navigate", "move"),
    ("palette.footer.run", "run"),
    ("palette.footer.open_in_new_tab", "open in new tab"),
    ("palette.footer.commands_only", "commands only"),
    ("palette.footer.tables_only", "tables only"),
    ("palette.section.connections", "Connections"),
    ("palette.section.commands", "Commands"),
    ("palette.section.charts", "Charts"),
    ("palette.section.tables", "Tables and collections"),
    ("palette.section.scripts", "Scripts"),
    ("chart.legend.hide_series", "Click to hide this series"),
    ("chart.legend.show_series", "Click to show this series"),
];

/// Installs ReqFlux's translator into `bolt`.
pub(crate) fn install(cx: &mut App) {
    bolt::strings::install(
        cx,
        bolt::strings::Strings {
            text,
            text_with: |key, args| text_with(key, args).into(),
        },
    );
}

/// The text of `key`, or the key itself when the catalog does not have it.
pub(crate) fn text(key: &str) -> SharedString {
    CATALOG.iter().find(|(known, _)| *known == key).map_or_else(
        || SharedString::from(key.to_owned()),
        |(_, text)| (*text).into(),
    )
}

/// The text of `key` with each `%{name}` replaced by the value of the argument `name`.
pub(crate) fn text_with(key: &str, args: &[(&str, &str)]) -> String {
    args.iter()
        .fold(text(key).to_string(), |text, (name, value)| {
            text.replace(&format!("%{{{name}}}"), value)
        })
}

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
