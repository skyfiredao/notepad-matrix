use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::files::DiskFile;
use crate::input::DisplayRevision;
use crate::workspace::{BufferId, CloseDisposition, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileObservation {
    pub length: u64,
    pub modified: SystemTime,
    pub identity: Option<(u64, u64)>,
}

impl FileObservation {
    pub fn read(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(io::Error::other("The file should exist to be monitored."));
        }
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let identity = None;
        Ok(Self {
            length: metadata.len(),
            modified: metadata.modified()?,
            identity,
        })
    }
}

struct Entry {
    path: PathBuf,
    observation: FileObservation,
    generation: u64,
}

#[derive(Clone)]
pub struct PollRequest {
    id: BufferId,
    path: PathBuf,
    observation: FileObservation,
    generation: u64,
    revision: DisplayRevision,
}

pub struct PollResult {
    request: PollRequest,
    result: io::Result<(FileObservation, Option<(DiskFile, String)>)>,
}

impl PollRequest {
    pub fn buffer_id(&self) -> BufferId {
        self.id
    }

    pub fn poll(self) -> PollResult {
        let observed = FileObservation::read(&self.path);
        self.poll_with_observation(observed)
    }

    pub fn poll_with_observation(self, observed: io::Result<FileObservation>) -> PollResult {
        let result = observed.and_then(|observation| {
            if observation.identity != self.observation.identity {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "monitored file was replaced",
                ));
            }
            if observation == self.observation {
                return Ok((observation, None));
            }
            let loaded = DiskFile::load_unicode(&self.path)?;
            if FileObservation::read(&self.path)? != observation {
                return Err(io::Error::other("monitored file changed while loading"));
            }
            Ok((observation, Some(loaded)))
        });
        PollResult {
            request: self,
            result,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorUpdate {
    Unchanged,
    Stale,
    Stopped { id: BufferId },
    Reloaded { id: BufferId, follow_tail: bool },
}

#[derive(Default)]
pub struct Monitoring {
    entries: BTreeMap<BufferId, Entry>,
    generation: u64,
    last_polled: Option<BufferId>,
}

impl Monitoring {
    pub fn restore(&mut self, workspace: &mut Workspace) -> Vec<String> {
        let ids = workspace
            .buffers()
            .iter()
            .filter(|buffer| buffer.is_monitoring())
            .map(|buffer| buffer.id())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for id in ids {
            if let Err(error) = self.start(workspace, id) {
                if let Some(buffer) = workspace.buffers_mut().find(|buffer| buffer.id() == id) {
                    buffer.set_monitoring(false);
                }
                errors.push(format!(
                    "buffer {}: monitoring could not resume: {error}",
                    id.0
                ));
            }
        }
        errors
    }
    pub fn is_monitoring(&self, id: BufferId) -> bool {
        self.entries.contains_key(&id)
    }

    pub fn start(&mut self, workspace: &mut Workspace, id: BufferId) -> io::Result<()> {
        let buffer = workspace
            .buffers_mut()
            .find(|buffer| buffer.id() == id)
            .ok_or_else(|| io::Error::other("monitoring buffer no longer exists"))?;
        let file = buffer
            .disk_file
            .as_ref()
            .ok_or_else(|| io::Error::other("The file should exist to be monitored."))?;
        let observation = FileObservation::read(file.path())?;
        if buffer.is_modified() {
            return Err(io::Error::other(
                "The document is dirty. Please save the modification before monitoring it.",
            ));
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("monitoring generation exhausted"))?;
        self.entries.insert(
            id,
            Entry {
                path: file.path().to_owned(),
                observation,
                generation,
            },
        );
        self.generation = generation;
        buffer.editor.set_read_only(true);
        buffer.set_monitoring(true);
        Ok(())
    }

    pub fn stop(&mut self, workspace: &mut Workspace, id: BufferId) {
        if self.entries.remove(&id).is_some()
            && let Some(buffer) = workspace.buffers_mut().find(|buffer| buffer.id() == id)
        {
            buffer.set_monitoring(false);
            let system_read_only = buffer
                .disk_file
                .as_ref()
                .is_some_and(|file| file.is_read_only());
            buffer.editor.set_read_only(system_read_only);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn requests(&mut self, workspace: &mut Workspace) -> Vec<PollRequest> {
        let invalid: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&id, entry)| {
                let matches = workspace.buffers().iter().any(|buffer| {
                    buffer.id() == id
                        && buffer
                            .disk_file
                            .as_ref()
                            .is_some_and(|file| file.path() == entry.path)
                });
                (!matches).then_some(id)
            })
            .collect();
        for id in invalid {
            self.stop(workspace, id);
        }
        let cursor = self.last_polled.unwrap_or(BufferId(0));
        let ordered = self
            .entries
            .iter()
            .filter(|(id, _)| **id > cursor)
            .chain(self.entries.iter().filter(|(id, _)| **id <= cursor));
        let requests: Vec<_> = ordered
            .filter_map(|(&id, entry)| {
                let buffer = workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.id() == id)?;
                if buffer.is_modified() || buffer.editor.has_composition() {
                    return None;
                }
                Some(PollRequest {
                    id,
                    path: entry.path.clone(),
                    observation: entry.observation.clone(),
                    generation: entry.generation,
                    revision: buffer.editor.display_revision(),
                })
            })
            .take(16)
            .collect();
        self.last_polled = requests.last().map(|request| request.id);
        requests
    }

    pub fn apply(
        &mut self,
        workspace: &mut Workspace,
        result: PollResult,
    ) -> io::Result<MonitorUpdate> {
        let PollResult { request, result } = result;
        let id = request.id;
        let Some(entry) = self.entries.get(&id) else {
            return Ok(MonitorUpdate::Stale);
        };
        if entry.generation != request.generation || entry.observation != request.observation {
            return Ok(MonitorUpdate::Stale);
        }
        let Some(buffer) = workspace.buffers().iter().find(|buffer| buffer.id() == id) else {
            self.stop(workspace, id);
            return Ok(MonitorUpdate::Stale);
        };
        if !buffer
            .disk_file
            .as_ref()
            .is_some_and(|file| file.path() == request.path)
            || buffer.editor.display_revision() != request.revision
        {
            return Ok(MonitorUpdate::Stale);
        }
        if buffer.is_modified() || buffer.editor.has_composition() {
            return Err(io::Error::other(
                "monitoring reload refused: buffer has unsaved changes",
            ));
        }
        let (observation, loaded) = match result {
            Ok(result) => result,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.stop(workspace, id);
                return Ok(MonitorUpdate::Stopped { id });
            }
            Err(error) => return Err(error),
        };
        let Some((file, text)) = loaded else {
            return Ok(MonitorUpdate::Unchanged);
        };
        let follow_tail = workspace.active().id() == id;
        workspace
            .reload_buffer(id, file, &text, CloseDisposition::IfUnmodified)
            .map_err(|error| io::Error::other(error.to_string()))?;
        if let Some(buffer) = workspace.buffers_mut().find(|buffer| buffer.id() == id) {
            buffer.editor.set_read_only(true);
            if follow_tail {
                buffer
                    .editor
                    .move_to_document_edge(true, false)
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        }
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.observation = observation;
        }
        Ok(MonitorUpdate::Reloaded { id, follow_tail })
    }
}
