#[cfg(all(target_os = "macos", feature = "native-input-tests"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected output PNG path")?;
    match std::env::args_os().nth(2) {
        Some(page) => notepad_matrix::ui::reference_frame::capture_search(
            std::path::Path::new(&path),
            page.to_str()
                .ok_or("expected find, replace, files or results")?,
        ),
        None => notepad_matrix::ui::reference_frame::capture(std::path::Path::new(&path)),
    }
}

#[cfg(not(all(target_os = "macos", feature = "native-input-tests")))]
fn main() {
    eprintln!("reference_frame requires macOS and --features native-input-tests");
    std::process::exit(2);
}
