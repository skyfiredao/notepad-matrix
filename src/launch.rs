use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::editor::{ColumnPosition, Selection};
use crate::files::DiskFile;
use crate::syntax::Language;
use crate::workspace::{CloseDisposition, Workspace};
use ropey::LineType;

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub files: Vec<PathBuf>,
    pub no_session: bool,
    pub language: Option<Language>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn language_name(language: Language) -> io::Result<Option<String>> {
    Ok(match language {
        Language::NormalText => Some("normal".into()),
        Language::Json => Some("json".into()),
        Language::Builtin(id) => Some(
            crate::languages::catalog()
                .map_err(|error| invalid(error.to_string()))?
                .by_id(id)
                .ok_or_else(|| invalid("Unknown language"))?
                .name
                .clone(),
        ),
        Language::UserDefined(_) => None,
    })
}

impl LaunchOptions {
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> io::Result<Self> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();
        let mut paths_only = false;
        while let Some(argument) = arguments.next() {
            let value = argument.to_str();
            if paths_only || value.is_none_or(|value| !value.starts_with('-')) {
                options.files.push(argument.into());
                continue;
            }
            let flag = value.ok_or_else(|| invalid("Invalid option encoding"))?;
            match flag {
                "--" => paths_only = true,
                "-multiInst" => {}
                "-nosession" => options.no_session = true,
                "-noPlugin" => return Err(invalid("-noPlugin is excluded; no file was opened")),
                _ => {
                    let (key, tail) = if let Some(tail) = flag.strip_prefix("-l") {
                        ('l', tail)
                    } else if let Some(tail) = flag.strip_prefix("-n") {
                        ('n', tail)
                    } else if let Some(tail) = flag.strip_prefix("-c") {
                        ('c', tail)
                    } else {
                        return Err(invalid(format!("Unsupported launch option: {flag}")));
                    };
                    let tail = if tail.is_empty() {
                        arguments
                            .next()
                            .ok_or_else(|| invalid(format!("Missing value for {flag}")))?
                    } else {
                        OsString::from(tail)
                    };
                    let tail = tail
                        .to_str()
                        .ok_or_else(|| invalid("Invalid option value encoding"))?;
                    if key == 'l' {
                        options.language = Some(match tail {
                            "normal" => Language::NormalText,
                            "json" => Language::Json,
                            _ => Language::Builtin(
                                crate::languages::catalog()
                                    .map_err(|error| invalid(error.to_string()))?
                                    .by_name(tail)
                                    .ok_or_else(|| invalid(format!("Unknown language: {tail}")))?
                                    .id,
                            ),
                        });
                    } else {
                        let number = tail
                            .parse::<usize>()
                            .map_err(|_| invalid("Line and column must be unsigned integers"))?
                            .max(1);
                        if key == 'n' {
                            options.line = Some(number);
                        } else {
                            options.column = Some(number);
                        }
                    }
                }
            }
        }
        Ok(options)
    }

    pub fn for_transfer(workspace: &Workspace) -> io::Result<Option<Self>> {
        let buffer = workspace.active();
        let Some(file) = buffer.disk_file.as_ref().filter(|_| !buffer.is_modified()) else {
            return Ok(None);
        };
        let document = buffer.editor.document();
        let caret = document.selection().head;
        let line = document
            .text_snapshot()
            .byte_to_line_idx(caret.0, LineType::LF_CR);
        let column = document
            .display_column(caret, 4)
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Some(Self {
            files: vec![file.path().to_owned()],
            no_session: true,
            language: if matches!(buffer.language(), Language::UserDefined(_)) {
                None
            } else {
                Some(buffer.language())
            },
            line: Some(line.saturating_add(1)),
            column: Some(column.saturating_add(1)),
        }))
    }

    pub fn arguments(&self) -> io::Result<Vec<OsString>> {
        let mut arguments = vec![OsString::from("-multiInst")];
        if self.no_session {
            arguments.push("-nosession".into());
        }
        if let Some(language) = self.language
            && let Some(name) = language_name(language)?
        {
            arguments.push(format!("-l{name}").into());
        }
        if let Some(line) = self.line {
            arguments.push(format!("-n{}", line.max(1)).into());
        }
        if let Some(column) = self.column {
            arguments.push(format!("-c{}", column.max(1)).into());
        }
        arguments.push("--".into());
        arguments.extend(self.files.iter().map(|path| path.as_os_str().to_owned()));
        Ok(arguments)
    }

    pub fn command(&self, executable: &Path) -> io::Result<Command> {
        let mut command = Command::new(executable);
        command.args(self.arguments()?);
        Ok(command)
    }

    pub fn spawn(&self, executable: &Path) -> io::Result<Child> {
        self.command(executable)?
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }

    pub fn open_into(&self, workspace: &mut Workspace) -> io::Result<Vec<String>> {
        let initial = workspace.active();
        let placeholder = (workspace.buffers().len() == 1
            && initial.disk_file.is_none()
            && !initial.is_modified()
            && initial.editor.document().len_bytes() == 0)
            .then_some(initial.id());
        let mut warnings = Vec::new();
        let mut opened = false;
        for path in &self.files {
            match DiskFile::load_unicode(path) {
                Ok((file, text)) => {
                    workspace
                        .open_buffer(file, &text)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    if let Some(language) = self.language {
                        workspace.active_mut().set_language(language);
                    }
                    opened = true;
                }
                Err(error) => warnings.push(format!("{}: {error}", path.display())),
            }
        }
        if let Some(initial) = placeholder.filter(|_| opened && workspace.buffers().len() > 1) {
            workspace
                .close_buffer(initial, CloseDisposition::IfUnmodified)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        if opened || self.files.is_empty() {
            if self.files.is_empty()
                && let Some(language) = self.language
            {
                workspace.active_mut().set_language(language);
            }
            if self.line.is_some() || self.column.is_some() {
                let document = workspace.active().editor.document();
                let current = document.selection().head;
                let line = self
                    .line
                    .map_or_else(
                        || {
                            document
                                .text_snapshot()
                                .byte_to_line_idx(current.0, LineType::LF_CR)
                        },
                        |line| line.saturating_sub(1),
                    )
                    .min(document.line_count().saturating_sub(1));
                let column = self.column.map_or(0, |column| column.saturating_sub(1));
                let (position, _) = document
                    .position_at_column(ColumnPosition { line, column }, 4)
                    .map_err(|error| io::Error::other(error.to_string()))?;
                workspace
                    .active_mut()
                    .editor
                    .select(Selection {
                        anchor: position,
                        head: position,
                    })
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        }
        Ok(warnings)
    }
}
