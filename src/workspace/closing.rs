use std::collections::{BTreeSet, VecDeque};

use crate::workspace::{BufferId, CloseDisposition, ViewId, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseTarget {
    pub view: ViewId,
    pub id: BufferId,
    pub disposition: CloseDisposition,
}

enum Scope {
    All {
        primary: Vec<BufferId>,
        secondary: Vec<BufferId>,
    },
    Conditional(ViewId),
    Selected(Vec<(ViewId, BufferId)>),
}

pub struct CloseBatch {
    pending: VecDeque<BufferId>,
    approved: Vec<(BufferId, CloseDisposition)>,
    automatic: BTreeSet<BufferId>,
    cancel_automatic: bool,
    scope: Scope,
}

pub fn individual_close_disposition(
    workspace: &Workspace,
    view: ViewId,
    id: BufferId,
) -> Option<CloseDisposition> {
    automatic_disposition(workspace, Some(view), id)
}

fn automatic_disposition(
    workspace: &Workspace,
    closing_view: Option<ViewId>,
    id: BufferId,
) -> Option<CloseDisposition> {
    let buffer = workspace
        .buffers()
        .iter()
        .find(|buffer| buffer.id() == id)?;
    if closing_view.is_some_and(|view| workspace.view_buffer_ids(view.other()).contains(&id)) {
        return Some(CloseDisposition::IfUnmodified);
    }
    if !buffer.is_modified() {
        return Some(CloseDisposition::IfUnmodified);
    }
    if buffer.disk_file.is_none() && buffer.editor.document().len_bytes() == 0 {
        return Some(CloseDisposition::DiscardConfirmed);
    }
    None
}

impl CloseBatch {
    pub fn dirty_count(&self, workspace: &Workspace) -> usize {
        match &self.scope {
            Scope::Selected(targets) => targets.iter().filter(|(_, id)| workspace.buffers().iter().any(|buffer| buffer.id() == *id && buffer.is_modified())).count(),
            _ => self.pending.iter().filter(|id| workspace.buffers().iter().any(|buffer| buffer.id() == **id && buffer.is_modified())).count(),
        }
    }

    pub fn uses_global_prompt_count(&self) -> bool {
        matches!(self.scope, Scope::All { .. }) || !self.cancel_automatic
    }

    pub fn prompt_view(&self, id: BufferId) -> ViewId {
        match &self.scope {
            Scope::Selected(targets) => targets
                .iter()
                .find(|(_, target)| *target == id)
                .map(|(view, _)| *view)
                .unwrap_or_default(),
            Scope::Conditional(view) => *view,
            Scope::All { secondary, .. } if secondary.contains(&id) => ViewId::Secondary,
            Scope::All { .. } => ViewId::Primary,
        }
    }

    pub fn conditional(view: ViewId, targets: Vec<BufferId>) -> Self {
        Self {
            pending: targets.into(),
            approved: Vec::new(),
            automatic: BTreeSet::new(),
            cancel_automatic: true,
            scope: Scope::Conditional(view),
        }
    }

    pub fn selected(targets: Vec<(ViewId, BufferId)>) -> Self {
        let mut pending = VecDeque::new();
        for (_, id) in &targets {
            if !pending.contains(id) {
                pending.push_back(*id);
            }
        }
        Self {
            pending,
            approved: Vec::new(),
            automatic: BTreeSet::new(),
            cancel_automatic: true,
            scope: Scope::Selected(targets),
        }
    }

    pub fn except_current(workspace: &Workspace) -> Self {
        let current = workspace.active().id();
        let mut targets = workspace.close_targets(crate::workspace::CloseCondition::ExceptCurrent);
        targets.sort_by_key(|(view, _)| *view == workspace.active_view());
        let mut batch = Self::selected(targets);
        batch.cancel_automatic = false;
        batch.pending.clear();
        for view in [ViewId::Primary, ViewId::Secondary] {
            for id in workspace.view_buffer_ids(view) {
                if *id != current && !batch.pending.contains(id) {
                    batch.pending.push_back(*id);
                }
            }
        }
        batch.approve_automatic(current, CloseDisposition::IfUnmodified);
        batch
    }

    pub fn approve_automatic(&mut self, id: BufferId, disposition: CloseDisposition) {
        self.automatic.insert(id);
        self.approve(id, disposition);
    }

    pub fn automatic_disposition(
        &self,
        workspace: &Workspace,
        id: BufferId,
    ) -> Option<CloseDisposition> {
        automatic_disposition(
            workspace,
            match &self.scope {
                Scope::Conditional(view) => Some(*view),
                Scope::Selected(targets) => {
                    [ViewId::Primary, ViewId::Secondary]
                        .into_iter()
                        .find(|view| {
                            workspace.view_buffer_ids(view.other()).contains(&id)
                                && targets.contains(&(*view, id))
                                && !targets.contains(&(view.other(), id))
                        })
                }
                Scope::All { .. } => None,
            },
            id,
        )
    }

    pub fn all(workspace: &Workspace) -> Self {
        let primary = workspace.view_buffer_ids(ViewId::Primary).to_vec();
        let secondary = workspace.view_buffer_ids(ViewId::Secondary).to_vec();
        let mut pending = primary.clone();
        pending.extend(secondary.iter().copied().filter(|id| !primary.contains(id)));
        Self {
            pending: pending.into(),
            approved: Vec::new(),
            automatic: BTreeSet::new(),
            cancel_automatic: true,
            scope: Scope::All { primary, secondary },
        }
    }

    pub fn approve(&mut self, id: BufferId, disposition: CloseDisposition) {
        self.approved.push((id, disposition));
    }

    pub fn finish(self, cancelled: bool, current_view: ViewId) -> Vec<CloseTarget> {
        let (primary, secondary) = match self.scope {
            Scope::Selected(targets) => {
                return targets
                    .into_iter()
                    .filter(|(_, id)| {
                        !cancelled || self.cancel_automatic || !self.automatic.contains(id)
                    })
                    .filter_map(|(view, id)| {
                        self.approved
                            .iter()
                            .find(|(approved, _)| *approved == id)
                            .map(|(_, disposition)| CloseTarget {
                                view,
                                id,
                                disposition: *disposition,
                            })
                    })
                    .collect();
            }
            Scope::Conditional(view) => {
                return self
                    .approved
                    .into_iter()
                    .map(|(id, disposition)| CloseTarget {
                        view,
                        id,
                        disposition,
                    })
                    .collect();
            }
            Scope::All { .. } if cancelled => return Vec::new(),
            Scope::All { primary, secondary } => (primary, secondary),
        };
        let members = |view| {
            if view == ViewId::Primary {
                &primary
            } else {
                &secondary
            }
        };
        [current_view.other(), current_view]
            .into_iter()
            .flat_map(|view| {
                members(view)
                    .iter()
                    .rev()
                    .filter_map(|id| {
                        self.approved
                            .iter()
                            .find(|(approved, _)| approved == id)
                            .map(|(_, disposition)| CloseTarget {
                                view,
                                id: *id,
                                disposition: *disposition,
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

impl Iterator for CloseBatch {
    type Item = BufferId;

    fn next(&mut self) -> Option<Self::Item> {
        self.pending.pop_front()
    }
}
