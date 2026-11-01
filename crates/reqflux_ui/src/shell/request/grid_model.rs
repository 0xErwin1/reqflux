//! The Params and Headers grids without elements: row ids, automation labels, secret masking,
//! the unresolved-row decision, all from the spans the engine reports, and the delete icon's
//! colors.

use bolt::controls::{IconButtonVariant, icon_button_colors};
use bolt::tokens::Colors;
use gpui::Hsla;
use reqflux_engine::Span;

/// What a secret value shows instead of anything it resolves to.
pub(crate) const MASK: &str = "••••••••";

/// The automation id of the 1-based row `n` of `table` (`params` or `headers`).
pub(crate) fn row_id(table: &str, n: usize) -> String {
    format!("request-{table}-row-{n}")
}

/// `<key>: <value as written>`, or `<key>: ••••••••` when the value references a secret.
pub(crate) fn row_label(key: &str, value: &str, secret: bool) -> String {
    if secret {
        format!("{key}: {MASK}")
    } else {
        format!("{key}: {value}")
    }
}

/// The key placeholder of the add row of a grid of `kind` entries: `header` reads `Header`.
pub(crate) fn placeholder(kind: &str) -> String {
    let mut chars = kind.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The note beside the value column's name: `resolved in <environment>` while an
/// environment is active, nothing for No environment.
pub(crate) fn value_hint(environment: Option<&str>) -> Option<String> {
    environment.map(|name| format!("resolved in {name}"))
}

/// Whether any of `spans` is a reference that does not resolve.
pub(crate) fn has_unresolved(spans: &[Span]) -> bool {
    spans.iter().any(|span| {
        matches!(
            span,
            Span::Reference {
                resolves: false,
                ..
            }
        )
    })
}

/// The text a secret value shows: its literal text followed by [`MASK`].
pub(crate) fn masked_text(spans: &[Span]) -> String {
    let mut shown: String = spans
        .iter()
        .filter_map(|span| match span {
            Span::Literal(literal) => Some(literal.as_str()),
            Span::Reference { .. } => None,
        })
        .collect();
    shown.push_str(MASK);
    shown
}

/// The delete icon of a grid row at rest and under the pointer: the ghost icon button's icon
/// role, muted, then the body text color.
pub(crate) fn delete_icon_colors(colors: &Colors) -> (Hsla, Hsla) {
    let rest = icon_button_colors(IconButtonVariant::Ghost, colors).icon;
    (rest, colors.foreground)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delete_icon_uses_the_ghost_icon_role_and_brightens_on_hover() {
        for colors in [Colors::dark(), Colors::light()] {
            let (rest, hover) = delete_icon_colors(&colors);
            assert_eq!(
                rest,
                icon_button_colors(IconButtonVariant::Ghost, &colors).icon
            );
            assert_eq!(rest, colors.muted);
            assert_eq!(hover, colors.foreground);
        }
    }

    fn reference(name: &str, resolves: bool, secret: bool) -> Span {
        Span::Reference {
            raw: format!("{{{{{name}}}}}"),
            name: name.to_owned(),
            resolves,
            secret,
        }
    }

    #[test]
    fn row_ids_are_one_based_per_table() {
        assert_eq!(row_id("headers", 1), "request-headers-row-1");
        assert_eq!(row_id("params", 3), "request-params-row-3");
    }

    #[test]
    fn labels_show_the_value_as_written() {
        assert_eq!(row_label("X-One", "{{base}}/1", false), "X-One: {{base}}/1");
    }

    #[test]
    fn secret_labels_never_show_the_value() {
        let label = row_label("Authorization", "Bearer {{token}}", true);
        assert_eq!(label, "Authorization: ••••••••");
        assert!(!label.contains("token"));
    }

    #[test]
    fn a_row_is_unresolved_only_with_an_unresolved_reference() {
        let literal = Span::Literal("a".to_owned());
        assert!(!has_unresolved(&[]));
        assert!(!has_unresolved(&[
            literal.clone(),
            reference("base", true, false)
        ]));
        assert!(has_unresolved(&[
            literal,
            reference("missing", false, false)
        ]));
    }

    #[test]
    fn the_add_row_placeholder_names_the_kind() {
        assert_eq!(placeholder("header"), "Header");
        assert_eq!(placeholder("param"), "Param");
        assert_eq!(placeholder(""), "");
    }

    #[test]
    fn the_value_hint_names_the_active_environment() {
        assert_eq!(
            value_hint(Some("staging")).as_deref(),
            Some("resolved in staging")
        );
        assert_eq!(value_hint(None), None);
    }

    #[test]
    fn masked_text_keeps_literals_and_drops_references() {
        let spans = [
            Span::Literal("Bearer ".to_owned()),
            reference("token", true, true),
        ];
        assert_eq!(masked_text(&spans), "Bearer ••••••••");
    }
}
