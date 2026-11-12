use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

/// Keep child ownership until it exits without blocking a shared executor worker.
/// The UI supplies its asynchronous timer; core tests supply a yielding clock.
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

/// Explicit Run invocation only. Npp variable expansion is still a separate
/// unimplemented capability; do not accidentally execute its names as shell
/// command substitutions. Ordinary POSIX substitutions remain valid syntax.
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
    for variable in [
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
    ] {
        if command.contains(&format!("$({variable})")) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Notepad++ Run variable expansion is not implemented yet; no command was launched",
            ));
        }
    }
    Ok(())
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

#[cfg(all(unix, feature = "native-input-tests"))]
pub(crate) fn launch_terminal_in_test(command: &str, directory: &Path) -> io::Result<Child> {
    launch_terminal_using(command, directory, |script| {
        Command::new("/bin/sh")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn terminal_launch_preserves_ls_output_argv_cwd_and_cleans_its_private_script() -> io::Result<()>
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir()?;
        let working = directory.path().join("working ' directory\nsecond line");
        std::fs::create_dir(&working)?;
        std::fs::write(working.join("visible-ls-result"), b"test")?;
        let opener = directory.path().join("isolated open helper");
        std::fs::write(
            &opener,
            "#!/bin/sh\ntest \"$1\" = -a || exit 2\ntest \"$2\" = Terminal || exit 3\ntest \"$#\" = 3 || exit 4\nexec /bin/sh \"$3\"\n",
        )?;
        std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o700))?;
        let mut path = None;
        let child = launch_terminal_using("ls", &working, |script| {
            path = Some(script.to_owned());
            assert_eq!(
                std::fs::metadata(script)?.permissions().mode() & 0o777,
                0o700
            );
            Command::new(&opener)
                .args([
                    std::ffi::OsStr::new("-a"),
                    std::ffi::OsStr::new("Terminal"),
                    script.as_os_str(),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })?;
        let output = child.wait_with_output()?;
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(output.stdout, b"visible-ls-result\n");
        assert!(!path.unwrap().exists());
        let mut failed_path = None;
        assert!(
            launch_terminal_using("ls", &working, |script| {
                failed_path = Some(script.to_owned());
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "isolated failed opener",
                ))
            })
            .is_err()
        );
        assert!(!failed_path.unwrap().exists());
        Ok(())
    }
}
