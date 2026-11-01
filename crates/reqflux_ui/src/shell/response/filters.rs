//! The Tests and Console filters without GPUI (response-views/02): which rows each choice
//! keeps, numbered as received so a row keeps its element id whatever the filter.

use reqflux_engine::{ConsoleEntry, ConsoleLevel, TestResult};

/// What the Tests pane lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TestsFilter {
    #[default]
    All,
    Failed,
}

/// What the Console pane lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ConsoleFilter {
    #[default]
    All,
    Warnings,
    Errors,
}

/// The tests `filter` keeps, each with its number from 1.
pub(crate) fn shown_tests(tests: &[TestResult], filter: TestsFilter) -> Vec<(usize, &TestResult)> {
    tests
        .iter()
        .enumerate()
        .filter(|(_, test)| filter == TestsFilter::All || !test.passed)
        .map(|(index, test)| (index + 1, test))
        .collect()
}

/// The console entries `filter` keeps, each with its number from 1.
pub(crate) fn shown_console(
    console: &[ConsoleEntry],
    filter: ConsoleFilter,
) -> Vec<(usize, &ConsoleEntry)> {
    console
        .iter()
        .enumerate()
        .filter(|(_, entry)| keeps(filter, entry.level))
        .map(|(index, entry)| (index + 1, entry))
        .collect()
}

/// How many entries of `console` `filter` keeps: the count on its segment.
pub(crate) fn console_count(console: &[ConsoleEntry], filter: ConsoleFilter) -> usize {
    shown_console(console, filter).len()
}

/// What an empty pane says under `filter`: `No failed tests`, `No warnings`, `No errors`.
pub(crate) fn tests_empty_text(filter: TestsFilter) -> Option<&'static str> {
    match filter {
        TestsFilter::All => None,
        TestsFilter::Failed => Some("No failed tests"),
    }
}

/// What the Console pane says when `filter` keeps nothing.
pub(crate) fn console_empty_text(filter: ConsoleFilter) -> Option<&'static str> {
    match filter {
        ConsoleFilter::All => None,
        ConsoleFilter::Warnings => Some("No warnings"),
        ConsoleFilter::Errors => Some("No errors"),
    }
}

/// Whether `level` is what `filter` keeps.
fn keeps(filter: ConsoleFilter, level: ConsoleLevel) -> bool {
    match filter {
        ConsoleFilter::All => true,
        ConsoleFilter::Warnings => level == ConsoleLevel::Warn,
        ConsoleFilter::Errors => level == ConsoleLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(name: &str, passed: bool) -> TestResult {
        TestResult {
            name: name.to_owned(),
            passed,
            message: (!passed).then(|| "bad".to_owned()),
        }
    }

    fn entry(level: ConsoleLevel) -> ConsoleEntry {
        ConsoleEntry {
            level,
            message: level.as_str().to_owned(),
        }
    }

    fn numbers<T>(rows: &[(usize, T)]) -> Vec<usize> {
        rows.iter().map(|(n, _)| *n).collect()
    }

    #[test]
    fn failed_keeps_the_failed_tests_with_their_numbers() {
        let tests = [test("a", true), test("b", false), test("c", true)];
        assert_eq!(numbers(&shown_tests(&tests, TestsFilter::All)), [1, 2, 3]);
        assert_eq!(numbers(&shown_tests(&tests, TestsFilter::Failed)), [2]);
    }

    #[test]
    fn warnings_and_errors_keep_their_level_only() {
        let console = [
            entry(ConsoleLevel::Log),
            entry(ConsoleLevel::Info),
            entry(ConsoleLevel::Warn),
            entry(ConsoleLevel::Error),
            entry(ConsoleLevel::Warn),
        ];
        assert_eq!(
            numbers(&shown_console(&console, ConsoleFilter::All)),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            numbers(&shown_console(&console, ConsoleFilter::Warnings)),
            [3, 5]
        );
        assert_eq!(
            numbers(&shown_console(&console, ConsoleFilter::Errors)),
            [4]
        );
        assert_eq!(console_count(&console, ConsoleFilter::Warnings), 2);
        assert_eq!(console_count(&console, ConsoleFilter::Errors), 1);
    }
}
