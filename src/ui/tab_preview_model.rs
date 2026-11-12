use crate::workspace::{BufferId, ViewId, Workspace};
use ropey::{LineType, Rope};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewLocation {
    pub pane: ViewId,
    pub id: BufferId,
}

impl PreviewLocation {
    pub fn allowed(self, workspace: &Workspace) -> bool {
        workspace.view_buffer_ids(self.pane).contains(&self.id)
            && [ViewId::Primary, ViewId::Secondary]
                .into_iter()
                .all(|pane| workspace.active_id_in_view(pane) != Some(self.id))
    }
}

#[derive(Default)]
pub struct PreviewHover {
    target: Option<PreviewLocation>,
    dismissed: Option<PreviewLocation>,
}

impl PreviewHover {
    pub fn target(&self) -> Option<PreviewLocation> {
        self.target
    }

    pub fn tab(&mut self, target: Option<PreviewLocation>) {
        if let Some(target) = target {
            if self.dismissed != Some(target) {
                self.target = Some(target);
                self.dismissed = None;
            }
        } else {
            self.target = None;
            self.dismissed = None;
        }
    }

    pub fn dismiss(&mut self) {
        self.dismissed = self.target.take();
    }
}

pub struct DocumentPreview {
    text: Rope,
}

impl DocumentPreview {
    pub fn capture(workspace: &Workspace, pane: ViewId, id: BufferId) -> Option<Self> {
        Some(Self {
            text: workspace.editor_in_view(pane, id)?.display_snapshot(),
        })
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines(LineType::LF_CR)
    }

    pub fn line(&self, index: usize) -> Option<String> {
        (index < self.line_count()).then(|| {
            self.text
                .line(index, LineType::LF_CR)
                .to_string()
                .trim_end_matches(['\r', '\n'])
                .to_owned()
        })
    }
}
