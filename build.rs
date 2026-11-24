use std::error::Error;
use std::io::{Read, Write};
use std::path::Path;

#[path = "build_support/intelligence.rs"]
mod intelligence;

#[path = "build_support/version.rs"]
mod version;

fn main() -> Result<(), Box<dyn Error>> {
    version::generate();
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing Cargo manifest directory")?;
    let out = std::env::var_os("OUT_DIR").ok_or("Missing Cargo output directory")?;
    let manifest = Path::new(&manifest);
    let out = Path::new(&out);
    intelligence::generate(manifest, out)?;
    let icon_source = manifest.join("assets/notepad-matrix.icns");
    println!("cargo:rerun-if-changed={}", icon_source.display());
    let icon_bytes = std::fs::read(icon_source)?;
    let header = icon_bytes.get(..16).ok_or("Missing ICNS header")?;
    let container_len = u32::from_be_bytes(header[4..8].try_into()?) as usize;
    if &header[..4] != b"icns" || container_len != icon_bytes.len() {
        return Err("Invalid ICNS container".into());
    }
    let entry_len = u32::from_be_bytes(header[12..16].try_into()?) as usize;
    let icon = icon_bytes.get(16..8 + entry_len).ok_or("Invalid first ICNS entry")?;
    if !icon.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("First ICNS entry must contain a PNG".into());
    }
    std::fs::write(out.join("application-icon.png"), icon)?;
    let source = manifest.join("assets/mermaid/mermaid-12.3.0.min.js");
    println!("cargo:rerun-if-changed={}", source.display());
    let bytes = std::fs::read(&source)?;
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&bytes)?;
    let packed = encoder.finish()?;
    let mut restored = Vec::with_capacity(bytes.len());
    flate2::bufread::DeflateDecoder::new(packed.as_slice()).read_to_end(&mut restored)?;
    if restored != bytes {
        return Err("Mermaid resource compression changed the source bytes".into());
    }
    std::fs::write(out.join("mermaid-script.deflate"), packed)?;
    std::fs::write(out.join("mermaid-source-len.rs"), format!("{}usize", bytes.len()))?;
    Ok(())
}
