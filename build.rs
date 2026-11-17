use std::error::Error;
use std::io::{Read, Write};
use std::path::Path;

#[path = "build_support/intelligence.rs"]
mod intelligence;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("Missing Cargo manifest directory")?;
    let out = std::env::var_os("OUT_DIR").ok_or("Missing Cargo output directory")?;
    let manifest = Path::new(&manifest);
    let out = Path::new(&out);
    intelligence::generate(manifest, out)?;
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
