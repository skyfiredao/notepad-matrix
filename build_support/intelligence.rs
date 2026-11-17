use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

pub fn generate(manifest: &Path, out: &Path) -> Result<(), Box<dyn Error>> {
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("build_support/intelligence.rs").display()
    );
    let resources = manifest.join("assets/intelligence");
    let destination = out.join("intelligence");
    fs::create_dir_all(&destination)?;
    let api_path = resources.join("apis.json");
    println!("cargo:rerun-if-changed={}", api_path.display());
    let api_json: Value = serde_json::from_slice(&fs::read(&api_path)?)?;
    let apis = api_json
        .as_object()
        .ok_or_else(|| invalid("API catalog must be an object"))?;
    let mut generated = String::from("const BUNDLED_APIS: &[(&str, &str)] = &[\n");
    for (index, (key, value)) in apis.iter().enumerate() {
        validate_node(value).map_err(|error| invalid(format!("API {key}: {error}")))?;
        let children = value["children"]
            .as_array()
            .ok_or_else(|| invalid(format!("missing API children: {key}")))?;
        for word in children.iter().filter(|node| node["tag"] == "KeyWord") {
            if word["attributes"]["name"].as_str().unwrap_or("").is_empty() {
                return Err(invalid(format!("missing API keyword name: {key}")).into());
            }
        }
        let filename = format!("api-{index}.json");
        fs::write(destination.join(&filename), serde_json::to_vec(value)?)?;
        writeln!(
            generated,
            "    ({key:?}, include_str!(concat!(env!(\"OUT_DIR\"), \"/intelligence/{filename}\"))),"
        )?;
    }
    generated.push_str("];\n");
    for (source, constant) in [
        ("parsers.json", "BUNDLED_PARSERS"),
        ("overrides.json", "BUNDLED_ASSOCIATIONS"),
    ] {
        let path = resources.join(source);
        println!("cargo:rerun-if-changed={}", path.display());
        let value: Value = serde_json::from_slice(&fs::read(&path)?)?;
        if source == "parsers.json" {
            let parsers = value
                .as_object()
                .ok_or_else(|| invalid("parser catalog must be an object"))?;
            for (key, parser) in parsers {
                validate_node(&parser["definition"])
                    .map_err(|error| invalid(format!("parser {key}: {error}")))?;
            }
        } else {
            validate_node(&value)?;
        }
        fs::write(destination.join(source), serde_json::to_vec(&value)?)?;
        writeln!(
            generated,
            "const {constant}: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/intelligence/{source}\"));"
        )?;
    }
    fs::write(out.join("intelligence_generated.rs"), generated)?;
    Ok(())
}

fn validate_node(value: &Value) -> Result<(), io::Error> {
    let tag = value["tag"]
        .as_str()
        .ok_or_else(|| invalid("missing definition tag"))?;
    let attributes = value["attributes"]
        .as_object()
        .ok_or_else(|| invalid(format!("missing attributes: {tag}")))?;
    for (key, value) in attributes {
        if !value.is_string() {
            return Err(invalid(format!("non-string attribute: {tag}/{key}")));
        }
    }
    let children = value["children"]
        .as_array()
        .ok_or_else(|| invalid(format!("missing children: {tag}")))?;
    for child in children {
        validate_node(child)?;
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
