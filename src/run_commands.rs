use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

/// Keep child ownership until it exits without blocking a shared executor worker.
pub async fn reap<F, Tick>(mut child: Child, mut wait_tick: F) -> io::Result<ExitStatus>
where
    F: FnMut() -> Tick,
    Tick: std::future::Future<Output = ()>,
{
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        wait_tick().await;
    }
}

pub fn launch_literal(command: &str, directory: &Path) -> io::Result<Child> {
    validate_command(command)?;
    Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn validate_command(command: &str) -> io::Result<()> {
    if command.trim().is_empty() || command.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a nonempty POSIX shell command without NUL",
        ));
    }
    Ok(())
}

pub const VARIABLE_NAMES: [&str; 11] = [
        "FULL_CURRENT_PATH",
        "CURRENT_DIRECTORY",
        "FILE_NAME",
        "NAME_PART",
        "EXT_PART",
        "CURRENT_WORD",
        "NPP_DIRECTORY",
        "NPP_FULL_FILE_PATH",
        "CURRENT_LINE",
        "CURRENT_COLUMN",
        "CURRENT_LINESTR",
];

pub fn shell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn expand(command: &str, values: &[String; 11]) -> io::Result<String> {
    if command.trim().is_empty() || command.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "运行命令和变量不能包含 NUL，命令不能为空"));
    }
    if !VARIABLE_NAMES.iter().any(|name| command.contains(&format!("$({name})"))) { return Ok(command.into()); }
    let mut output = String::new();
    for (index, value) in values.iter().enumerate() {
        if !command.contains(&format!("$({})", VARIABLE_NAMES[index])) { continue; }
        if value.contains('\0') { return Err(io::Error::new(io::ErrorKind::InvalidInput, "运行变量不能包含 NUL")); }
        output.push_str(&format!("__NOTEPAD_MATRIX_RUN_VALUE_{index}={}\n", shell_literal(value)));
    }
    let mut quote = '\0';
    let mut word_start = true;
    let mut parameter_depth = 0;
    let mut contexts: Vec<(char, char, bool, usize)> = Vec::new();
    let mut rest = command;
    while !rest.is_empty() {
        if quote == '\0' && parameter_depth == 0 && word_start && rest.starts_with('#') {
            if contexts.iter().any(|(_, delimiter, _, _)| *delimiter == '`') {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "含运行变量时，请用 $(command) 代替带注释的反引号命令替换"));
            }
            let end = rest.find('\n').unwrap_or(rest.len());
            output.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        if quote != '\'' && rest.starts_with("${") {
            parameter_depth += 1;
            word_start = false;
            output.push_str("${");
            rest = &rest[2..];
            continue;
        }
        let variable = VARIABLE_NAMES.iter().enumerate().find_map(|(index, name)| {
            rest.strip_prefix(&format!("$({name})")).map(|tail| (index, tail))
        });
        if let Some((index, tail)) = variable {
            let reference = format!("${{__NOTEPAD_MATRIX_RUN_VALUE_{index}}}");
            let mut insertion = match quote {
                '\'' => format!("'\"{reference}\"'"),
                '"' => reference,
                _ => format!("\"{reference}\""),
            };
            for _ in contexts.iter().filter(|(_, delimiter, _, _)| *delimiter == '`') {
                insertion = insertion.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
            }
            output.push_str(&insertion);
            word_start = false;
            rest = tail;
            continue;
        }
        let Some(ch) = rest.chars().next() else { break; };
        if ch == '\\' && quote != '\'' {
            output.push(ch);
            rest = &rest[ch.len_utf8()..];
            if VARIABLE_NAMES.iter().any(|name| rest.starts_with(&format!("$({name})"))) {
                output.pop();
            } else if let Some(escaped) = rest.chars().next() {
                output.push(escaped);
                rest = &rest[escaped.len_utf8()..];
                if escaped != '\n' { word_start = false; }
            }
            continue;
        }
        if rest.starts_with("<<") && quote == '\0' {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "请将运行变量作为命令参数传入，不要放入 here-document"));
        }
        if rest.starts_with("$(") && quote != '\'' {
            contexts.push((quote, ')', false, parameter_depth));
            quote = '\0';
            word_start = true;
            parameter_depth = 0;
            output.push_str("$(");
            rest = &rest[2..];
            continue;
        }
        match ch {
            '\'' | '"' if quote == '\0' => { quote = ch; word_start = false; }
            '\'' | '"' if quote == ch => { quote = '\0'; word_start = false; }
            '}' if quote != '\'' && parameter_depth != 0 => { parameter_depth -= 1; word_start = false; }
            '(' if quote == '\0' => { contexts.push((quote, ')', true, parameter_depth)); word_start = true; parameter_depth = 0; }
            ')' if quote == '\0' && contexts.last().is_some_and(|(_, delimiter, _, _)| *delimiter == ')') => if let Some((parent, _, parent_word_start, parent_parameter_depth)) = contexts.pop() { quote = parent; word_start = parent_word_start; parameter_depth = parent_parameter_depth; },
            '`' if quote != '\'' => {
                if contexts.last().is_some_and(|(_, delimiter, _, _)| *delimiter == '`') {
                    if let Some((parent, _, parent_word_start, parent_parameter_depth)) = contexts.pop() { quote = parent; word_start = parent_word_start; parameter_depth = parent_parameter_depth; }
                } else { contexts.push((quote, '`', false, parameter_depth)); quote = '\0'; word_start = true; parameter_depth = 0; }
            }
            _ => word_start = quote == '\0' && matches!(ch, ' ' | '\t' | '\n' | ';' | '&' | '|' | '<' | '>' | ')'),
        }
        output.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    Ok(output)
}

#[cfg(unix)]
fn launch_terminal_using(
    command: &str,
    directory: &Path,
    launcher: impl FnOnce(&Path) -> io::Result<Child>,
) -> io::Result<Child> {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    validate_command(command)?;
    let directory = directory.canonicalize()?;
    let mut script = tempfile::Builder::new()
        .prefix("notepad-matrix-run-")
        .suffix(".command")
        .tempfile()?;
    script.write_all(b"#!/bin/sh\n/bin/rm -- \"$0\"\ncd '")?;
    for (index, part) in directory
        .as_os_str()
        .as_bytes()
        .split(|byte| *byte == b'\'')
        .enumerate()
    {
        if index != 0 {
            script.write_all(b"'\"'\"'")?;
        }
        script.write_all(part)?;
    }
    script.write_all(b"' || exit\n")?;
    script.write_all(command.as_bytes())?;
    script.write_all(b"\n")?;
    script
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o700))?;
    script.as_file().sync_all()?;
    let (file, path) = script.keep().map_err(|error| error.error)?;
    drop(file);
    match launcher(&path) {
        Ok(child) => Ok(child),
        Err(error) => {
            if let Err(cleanup) = std::fs::remove_file(&path)
                && cleanup.kind() != io::ErrorKind::NotFound
            {
                return Err(io::Error::other(format!(
                    "{error}; private Run script cleanup failed: {cleanup}"
                )));
            }
            Err(error)
        }
    }
}

#[cfg(unix)]
pub fn launch_terminal(command: &str, directory: &Path) -> io::Result<Child> {
    launch_terminal_using(command, directory, |script| {
        Command::new("/usr/bin/open")
            .args([
                std::ffi::OsStr::new("-a"),
                std::ffi::OsStr::new("Terminal"),
                script.as_os_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    })
}
