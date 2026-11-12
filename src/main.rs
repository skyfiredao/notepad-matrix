use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    let result = notepad_matrix::ui::run();
    #[cfg(not(target_os = "macos"))]
    let result: Result<(), Box<dyn std::error::Error>> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Notepad Matrix native UI requires macOS",
    )
    .into());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Notepad Matrix: {error}");
            ExitCode::FAILURE
        }
    }
}
