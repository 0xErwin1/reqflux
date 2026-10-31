//! One MCP session's choice of bridge target: automatic with a single live
//! app, explicit with several, and never a silent switch to another app.
//!
//! A window reinstalls its bridge with a new instance id and token after an approval
//! (mcp-approvals L5), so the selected target's id changes while the window stays. When the
//! selected id is gone, the session follows the only live target of the same process that
//! was not already running beside it: another window of that process is never a successor.

use std::path::PathBuf;

use gpui_mcp_protocol::AppId;
use thiserror::Error;

use super::client::BridgeClient;
use super::descriptor::{TargetId, TargetSummary, ValidatedTarget};
use super::discover::{DiscoveryError, discover};

/// Why a session could not pick a target to talk to.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The endpoint directory could not be read.
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    /// No app with this identifier is running.
    #[error("no {app_id} app is running; start it, then call ui_list_targets again")]
    NoTarget {
        /// The application identifier the session looks for.
        app_id: String,
    },
    /// Several apps are running and none was chosen.
    #[error("{count} {app_id} apps are running; choose one with ui_select_target")]
    Ambiguous {
        /// The application identifier the session looks for.
        app_id: String,
        /// How many targets are live.
        count: usize,
    },
    /// The target id is not a live target.
    #[error("no live target has the id {0}")]
    UnknownTarget(TargetId),
    /// The selected target stopped running.
    #[error("the selected target {0} is no longer running; call ui_select_target")]
    SelectedTargetGone(TargetId),
}

/// The targets a session may talk to and the one it talks to.
#[derive(Debug)]
pub struct BridgeSession {
    endpoint_dir: PathBuf,
    app_id: AppId,
    selected: Option<TargetId>,
    /// The process of the selected target, to follow its window across bridge reinstalls.
    selected_pid: Option<u32>,
    /// The other targets of that process last seen live beside it; never its successor.
    siblings: Vec<TargetId>,
}

impl BridgeSession {
    /// A session that looks for `app_id` under `endpoint_dir`, with nothing
    /// selected yet.
    #[must_use]
    pub fn new(endpoint_dir: PathBuf, app_id: AppId) -> Self {
        Self {
            endpoint_dir,
            app_id,
            selected: None,
            selected_pid: None,
            siblings: Vec::new(),
        }
    }

    /// The process of the selected target, once one is chosen.
    #[must_use]
    pub fn selected_pid(&self) -> Option<u32> {
        self.selected_pid
    }

    /// Selects `target`, one of the live `targets`.
    fn select(&mut self, target: &ValidatedTarget, targets: &[ValidatedTarget]) {
        let id = target.target_id();
        let pid = target.descriptor().pid;
        self.siblings = targets
            .iter()
            .filter(|other| other.descriptor().pid == pid)
            .map(ValidatedTarget::target_id)
            .filter(|other| *other != id)
            .collect();
        self.selected = Some(id);
        self.selected_pid = Some(pid.get());
    }

    /// The target the session talks to, once one is chosen.
    #[must_use]
    pub fn selected(&self) -> Option<&TargetId> {
        self.selected.as_ref()
    }

    /// The live targets, each marked when it is the session's. The only live
    /// target is selected automatically.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Discovery`] when the endpoint directory cannot
    /// be read.
    pub async fn list_targets(&mut self) -> Result<Vec<TargetSummary>, SessionError> {
        let targets = discover(&self.endpoint_dir, Some(&self.app_id)).await?;
        if self.selected.is_none()
            && let [only] = targets.as_slice()
        {
            self.select(only, &targets);
        }
        let selected = self.selected.clone();
        Ok(targets
            .iter()
            .map(|target| {
                let mut summary = target.summary();
                summary.selected = selected.as_ref() == Some(&summary.target_id);
                summary
            })
            .collect())
    }

    /// Chooses the live target `target_id` for the rest of the session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::UnknownTarget`] when no live target has that
    /// id, and [`SessionError::Discovery`] when the endpoint directory cannot
    /// be read.
    pub async fn select_target(&mut self, target_id: &TargetId) -> Result<(), SessionError> {
        let targets = discover(&self.endpoint_dir, Some(&self.app_id)).await?;
        let target = targets
            .iter()
            .find(|target| target.target_id() == *target_id)
            .ok_or_else(|| SessionError::UnknownTarget(target_id.clone()))?;
        self.select(target, &targets);
        Ok(())
    }

    /// A client for the session's target, selecting the only live one when
    /// nothing is selected, and following the selected window to its new bridge when the
    /// only live target of the same process replaced it.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NoTarget`] with no live target,
    /// [`SessionError::Ambiguous`] with several and none selected,
    /// [`SessionError::SelectedTargetGone`] when the selected target stopped
    /// running, and [`SessionError::Discovery`] when the endpoint directory
    /// cannot be read.
    pub async fn client(&mut self) -> Result<BridgeClient, SessionError> {
        let targets = discover(&self.endpoint_dir, Some(&self.app_id)).await?;
        let index = match &self.selected {
            Some(selected) => targets
                .iter()
                .position(|target| target.target_id() == *selected)
                .or_else(|| self.successor(&targets))
                .ok_or_else(|| SessionError::SelectedTargetGone(selected.clone()))?,
            None => match targets.len() {
                0 => {
                    return Err(SessionError::NoTarget {
                        app_id: self.app_id.as_str().to_owned(),
                    });
                }
                1 => 0,
                count => {
                    return Err(SessionError::Ambiguous {
                        app_id: self.app_id.as_str().to_owned(),
                        count,
                    });
                }
            },
        };
        let target = targets
            .get(index)
            .cloned()
            .ok_or_else(|| SessionError::NoTarget {
                app_id: self.app_id.as_str().to_owned(),
            })?;
        self.select(&target, &targets);
        Ok(BridgeClient::new(target))
    }

    /// The index of the only live target of the selected process that was not live beside
    /// it, when there is exactly one: the same window's reinstalled bridge.
    fn successor(&self, targets: &[ValidatedTarget]) -> Option<usize> {
        let pid = self.selected_pid?;
        let mut same = targets.iter().enumerate().filter(|(_, target)| {
            target.descriptor().pid.get() == pid && !self.siblings.contains(&target.target_id())
        });
        match (same.next(), same.next()) {
            (Some((index, _)), None) => Some(index),
            _ => None,
        }
    }
}
