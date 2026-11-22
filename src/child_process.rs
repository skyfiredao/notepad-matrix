use std::io;
use std::process::{Child, ExitStatus};

pub(crate) async fn reap<F, Tick>(mut child: Child, mut wait_tick: F) -> io::Result<ExitStatus>
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
