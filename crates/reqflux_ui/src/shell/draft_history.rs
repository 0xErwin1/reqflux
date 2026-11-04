//! The undo and redo history of request drafts (feedback-nov3 S4). Every edit of a draft
//! pushes the state before it as a snapshot; undo and redo walk the snapshots. A snapshot is
//! the whole draft, or `None` for "no draft", the request as saved, so discarding a draft
//! and undoing the first edit are steps like any other.
//!
//! A burst of edits to one slot of the request, typing in the URL or in a body, is one step:
//! an edit within [`BURST`] of the previous one in the same slot joins it. A structural edit
//! (a mode switch, a revert) never does. The history keeps the last [`DEPTH`] steps and lives
//! as long as the app, beside the drafts and not in them, because discarding a draft removes
//! it and must stay undoable. Saving forgets the request's history: a draft restored across a
//! save would be made from a file that no longer exists.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use gpui::Global;
use reqflux_core::model::{Auth, Body};

use super::RequestDraft;

/// The steps a request keeps.
pub(crate) const DEPTH: usize = 100;

/// How long after an edit the next edit of the same slot still joins its step.
pub(crate) const BURST: Duration = Duration::from_millis(800);

/// The time of an edit. Tests freeze it, so a slow machine never splits a typing burst.
pub(super) fn now() -> Instant {
    #[cfg(test)]
    {
        static FROZEN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *FROZEN.get_or_init(Instant::now)
    }
    #[cfg(not(test))]
    Instant::now()
}

/// What an edit changed, which decides whether it joins the previous step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Method,
    Url,
    Params,
    Headers,
    Auth,
    Body,
    Settings,
    Scripts,
    /// Anything else, or a change of several slots at once: a step of its own.
    Structural,
}

impl Slot {
    /// The slot `after` changed from `before`: a body or auth that changed its mode is
    /// structural, a request that changed in several slots too.
    pub(crate) fn of(before: &RequestDraft, after: &RequestDraft) -> Self {
        let (old, new) = (&before.request, &after.request);
        let changed = [
            (old.method != new.method, Self::Method),
            (old.url != new.url, Self::Url),
            (old.params != new.params, Self::Params),
            (old.headers != new.headers, Self::Headers),
            (old.auth != new.auth, Self::Auth),
            (old.body != new.body, Self::Body),
            (old.settings != new.settings, Self::Settings),
            (before.scripts != after.scripts, Self::Scripts),
        ];
        let mut slots = changed
            .into_iter()
            .filter_map(|(changed, slot)| changed.then_some(slot));
        match (slots.next(), slots.next()) {
            (Some(Self::Body), None) if body_mode(&old.body) != body_mode(&new.body) => {
                Self::Structural
            }
            (Some(Self::Auth), None) if auth_mode(&old.auth) != auth_mode(&new.auth) => {
                Self::Structural
            }
            (Some(slot), None) => slot,
            _ => Self::Structural,
        }
    }
}

fn body_mode(body: &Body) -> std::mem::Discriminant<Body> {
    std::mem::discriminant(body)
}

fn auth_mode(auth: &Auth) -> &str {
    auth.kind()
}

/// A state of a request's draft; `None` is no draft.
pub(crate) type Snapshot = Option<RequestDraft>;

#[derive(Default)]
struct History {
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    /// The slot and time of the last recorded edit, while a burst can still grow.
    last: Option<(Slot, Instant)>,
}

/// The histories of every request with one, by request id.
#[derive(Default)]
pub(crate) struct DraftHistories(HashMap<String, History>);

impl Global for DraftHistories {}

impl DraftHistories {
    /// Records that the draft of `id` was `before` when an edit of `slot` happened at `now`.
    /// An edit that continues a burst of the same slot adds no step; any edit drops the
    /// redo steps.
    pub(crate) fn record(&mut self, id: &str, before: Snapshot, slot: Slot, now: Instant) {
        let history = self.0.entry(id.to_owned()).or_default();
        history.redo.clear();
        let joins = slot != Slot::Structural
            && !history.undo.is_empty()
            && history.last.is_some_and(|(last, at)| {
                last == slot && now.saturating_duration_since(at) <= BURST
            });
        history.last = Some((slot, now));
        if joins {
            return;
        }
        history.undo.push_back(before);
        if history.undo.len() > DEPTH {
            history.undo.pop_front();
        }
    }

    /// Moves one step: back when `undo`, else forward. `current` is the draft now, which
    /// becomes the step the other direction returns to. `None` when there is no step.
    pub(crate) fn step(&mut self, id: &str, undo: bool, current: Snapshot) -> Option<Snapshot> {
        let history = self.0.get_mut(id)?;
        history.last = None;
        if undo {
            let target = history.undo.pop_back()?;
            history.redo.push(current);
            Some(target)
        } else {
            let target = history.redo.pop()?;
            history.undo.push_back(current);
            Some(target)
        }
    }

    pub(crate) fn can_undo(&self, id: &str) -> bool {
        self.0
            .get(id)
            .is_some_and(|history| !history.undo.is_empty())
    }

    pub(crate) fn can_redo(&self, id: &str) -> bool {
        self.0
            .get(id)
            .is_some_and(|history| !history.redo.is_empty())
    }

    /// Drops the history of `id`.
    pub(crate) fn forget(&mut self, id: &str) {
        self.0.remove(id);
    }
}

#[cfg(test)]
#[path = "draft_history_tests.rs"]
mod tests;
