use notepad_matrix::run_commands::launch_literal;

#[test]
fn child_reaping_yields_while_the_process_is_live_and_eventually_reaps_it()
-> Result<(), Box<dyn std::error::Error>> {
    use std::future::Future;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::task::{Context, Poll, Waker};
    let mut child = Command::new("/bin/sh")
        .args(["-c", "read value; test \"$value\" = released"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut gate = child.stdin.take().ok_or("child input pipe")?;
    let mut reaping = std::pin::pin!(notepad_matrix::run_commands::reap(child, || async {
        let mut yielded = false;
        std::future::poll_fn(move |_| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                Poll::Pending
            }
        })
        .await
    }));
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(reaping.as_mut().poll(&mut context), Poll::Pending),
        "a live process must not retain an executor thread"
    );
    writeln!(gate, "released")?;
    drop(gate);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Poll::Ready(result) = reaping.as_mut().poll(&mut context) {
            assert!(result?.success());
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "released child was not reaped"
        );
        std::thread::yield_now();
    }
    Ok(())
}

#[test]
fn invalid_or_unimplemented_run_input_cannot_launch_a_shell()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    for command in [
        " ",
        "printf bad\0",
        "printf '%s' '$(FULL_CURRENT_PATH)' > forbidden.txt",
    ] {
        assert!(launch_literal(command, directory.path()).is_err());
        assert!(!directory.path().join("forbidden.txt").exists());
    }
    assert!(launch_literal("printf bad", &directory.path().join("missing")).is_err());
    Ok(())
}

#[test]
fn explicit_run_uses_posix_shell_in_the_supplied_directory_without_cmd_parsing()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let mut child = launch_literal(
        "printf '%s' 'space and '\"'\"'quote' > 'output file.txt'",
        directory.path(),
    )?;
    assert!(child.wait()?.success());
    assert_eq!(
        std::fs::read(directory.path().join("output file.txt"))?,
        b"space and 'quote"
    );
    let mut child = launch_literal(
        "printf '%s' \"$(printf nested)\" > nested.txt",
        directory.path(),
    )?;
    assert!(child.wait()?.success());
    assert_eq!(
        std::fs::read(directory.path().join("nested.txt"))?,
        b"nested"
    );
    Ok(())
}
