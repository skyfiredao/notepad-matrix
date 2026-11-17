# Build on macOS

[中文](BUILD_zh.md) · [Overview](README.md) · [Design decisions](DESIGN.md)

Run the following commands in a terminal on your Mac.

## 1. Prepare the environment

### System and tools

| Item | Requirement |
|---|---|
| Example system | macOS 26. The build and packaging examples use deployment target `26.0`. |
| Hardware | An Apple Silicon Mac; `uname -m` must report `arm64`. |
| Developer tools | Xcode Command Line Tools, or full Xcode with its command-line tools configured. The macOS SDK, Clang, and Swift are required. |
| Rust | **1.98.1**, pinned in `rust-toolchain.toml`. Use rustup to manage it. |
| Python | Python 3 for `scripts/package-macos.py`. No third-party Python packages are required. |
| Network | Required for the initial Rust toolchain and Cargo dependency downloads. |

The packaging script accepts ARM64 Mach-O executables only. It does not create Intel or Universal applications.

If the Apple command-line tools are not installed:

```sh
xcode-select --install
```

After installation, check the tools:

```sh
uname -m
xcode-select -p
xcrun --sdk macosx --show-sdk-path
xcrun --find clang
xcrun --find swift
python3 --version
```

Install rustup from [rustup.rs](https://rustup.rs/), then install the project toolchain and target:

```sh
rustup toolchain install 1.98.1 --profile minimal
rustup target add aarch64-apple-darwin --toolchain 1.98.1
```

The project enables GPUI's `runtime_shaders` feature, which compiles Metal shaders at runtime. This configuration does not require separate offline `metal` or `metallib` build commands.

### Source directory

Get the source from the [official project repository](https://github.com/skyfiredao/notepad-matrix):

```sh
git clone https://github.com/skyfiredao/notepad-matrix.git
```

Enter the project directory containing `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/`, and `assets/`. Run all remaining commands from that directory.

```sh
rustc --version
cargo --version
rustup show active-toolchain
```

`rustc` should use the pinned version, 1.98.1. Normal builds use the repository's `Cargo.lock` and bundled resources. They do not require a separate Notepad++ or Scintilla checkout or their runtime libraries.

## 2. Build the application

This example uses macOS 26 and sets the deployment target to `26.0`. The packaging script also writes this value to `LSMinimumSystemVersion`. Use Xcode or Command Line Tools and an SDK that support macOS 26. Lowering the value alone does not establish compatibility with older systems.

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo fetch --locked
cargo build --locked --release --target aarch64-apple-darwin
```

The default features include `native-ui`. A core-library build with default features disabled does not contain a runnable graphical application.

Default output:

```text
target/aarch64-apple-darwin/release/notepad-matrix
```

Check its architecture:

```sh
file target/aarch64-apple-darwin/release/notepad-matrix
```

The output should identify an ARM64 Mach-O executable. If you set `CARGO_TARGET_DIR`, update all later binary paths to match that output directory.

For a debug build, omit `--release`:

```sh
cargo build --locked --target aarch64-apple-darwin
```

Debug output goes into `target/aarch64-apple-darwin/debug/`. Release builds use `opt-level=3`, ThinLTO, one codegen unit, and symbol stripping, so linking time differs from debug builds.

## 3. Run locally

Start the editor from the terminal, with or without a file:

```sh
./target/aarch64-apple-darwin/release/notepad-matrix
./target/aarch64-apple-darwin/release/notepad-matrix ./notes.txt
```

To start an independent instance without restoring or saving the automatic session for that launch:

```sh
./target/aarch64-apple-darwin/release/notepad-matrix -multiInst -nosession ./notes.txt
```

`-nosession` does not isolate configuration or other user data. Configuration still lives in `~/.notepad-matrix/`. See [command-line usage](README.md#command-line-usage) for the full option list.

Building and starting the editor do not require Accessibility permission. Normal editing, file opening, and printing do not depend on UI automation tools. Respond to system permission prompts when using the related system operation.

## 4. Package the application

Use the same toolchain environment and deployment target to run the repository script:

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
python3 scripts/package-macos.py \
  --binary target/aarch64-apple-darwin/release/notepad-matrix \
  --output dist/macos-arm64
```

The output directory must be absent or empty. The script refuses to overwrite existing artifacts. Use a new empty directory for another package; clearing the Cargo cache is not required.

Outputs:

```text
dist/macos-arm64/notepad-matrix.app
dist/macos-arm64/notepad-matrix.dmg
```

The script:

1. Checks that the input is an ARM64 Mach-O executable.
2. Reads dependencies with `cargo metadata --locked --offline`, so dependencies must already be downloaded.
3. Uses Swift and system type identifiers to create document type declarations.
4. Copies the executable, icon, themes, and dependency licenses, and writes `Info.plist`.
5. Strips symbols, applies an ad-hoc signature, and creates a compressed DMG with an Applications link.

Verify the signature and launch the application:

```sh
codesign --verify --deep --strict --verbose=2 dist/macos-arm64/notepad-matrix.app
open dist/macos-arm64/notepad-matrix.app
```

An ad-hoc signature is not a Developer ID signature, and the script does not notarize builds. Running locally does not establish that Gatekeeper will accept the package on another Mac. Public distribution requires a separate developer-signing and notarization process, not disabling system security.

## 5. Allow a trusted application to run

### Check the download source first

Download through links provided by the [official project repository](https://github.com/skyfiredao/notepad-matrix), or from a trusted publisher whose identity and file integrity you can verify. Avoid unknown file hosts, advertising downloaders, and repackaged mirrors. Unsigned, ad-hoc-signed, and unnotarized software carries additional risk.

If the publisher provides a SHA-256 checksum, check the downloaded file before installation or local signing. For example:

```sh
shasum -a 256 "$HOME/Downloads/notepad-matrix.dmg"
```

Use the actual downloaded filename and compare the output with the value on the trusted release page. A matching hash confirms that the file matches the published content. It does not establish trust in the publisher or prove that the file is free of malware.

### Locally built, ad-hoc-signed packages

The packaging script already runs `codesign --force --sign -`, so signing again is normally unnecessary. Run the `codesign --verify` command above, then launch through Finder or `open`. If macOS still says that the developer cannot be verified or that Apple cannot check the app for malicious software, confirm that this is your own build and follow the per-application approval steps below.

Ad-hoc signing supports local signature integrity checks. It does not identify the developer, provide Apple notarization, or automatically grant Gatekeeper approval.

### Downloaded ad-hoc-signed or unsigned versions

After checking the source, checksum, and release notes, copy the `.app` from the disk image to an application directory and try to open it once. If macOS blocks it because of an unidentified developer or missing notarization:

1. Open **System Settings → Privacy & Security**.
2. In **Security**, find the blocked Notepad Matrix application and check its name.
3. Click **Open Anyway** and authenticate when prompted.
4. Check the warning again and click **Open**. This creates an exception for that application; it does not disable checks for all applications.

Later launches can normally use a double-click. macOS may check the app again after an update or replacement. Organization-managed Macs may prevent exceptions; contact the administrator rather than bypassing management policy.

If the release notes explicitly identify an unsigned build that needs local ad-hoc signing, first copy the trusted `.app` to a directory you can write, such as `~/Applications/`, then run:

```sh
codesign --force --sign - "$HOME/Applications/notepad-matrix.app"
codesign --verify --deep --strict --verbose=2 "$HOME/Applications/notepad-matrix.app"
open "$HOME/Applications/notepad-matrix.app"
```

Local signing modifies the application, so verify the download first. It neither authenticates the original publisher nor replaces per-application approval. Do not re-sign an unknown, invalidly signed, or tampered application to hide a problem.

If macOS reports that the app will damage your computer, detects malware, or reports a damaged application, stop and obtain a fresh copy from a trusted source. Contact the publisher if needed. Do not treat these warnings as routine missing-signature messages, disable Gatekeeper, or remove quarantine attributes in bulk.

System procedure: [Apple: Safely open apps on your Mac](https://support.apple.com/en-us/102445). Wording and button locations may differ across macOS versions.

## Troubleshooting

| Problem | Check and action |
|---|---|
| Wrong Rust version | Check the project directory and `rustup show active-toolchain`. Install the version in `rust-toolchain.toml`; do not change the lockfile or switch to nightly to hide an error. |
| Missing SDK, Clang, or Swift | Check `xcode-select -p` and the `xcrun` commands above. Finish installing and initializing the Apple developer tools. |
| Cargo download failure | Check the network and Cargo registry configuration. Complete `cargo fetch --locked` before using `--offline`. |
| Running outside macOS fails | Build and run the graphical application on a Mac. |
| Packaging rejects the architecture | Check `file` output and build for `aarch64-apple-darwin`. Do not pass an Intel executable. |
| Packaging reports a missing environment variable | Set `MACOSX_DEPLOYMENT_TARGET` in the same terminal that runs the script. |
| Output directory is not empty | Choose an empty `--output` directory. The script does not overwrite existing packages. |
| Signing or launch is blocked | Check the source and integrity, then approve only the trusted application as described above. Ad-hoc signing is not developer authentication or notarization. Do not force a malware-blocked or damaged app to run. |

## Resource generation

Normal builds use the checked-in `assets/` directly. Resource-import scripts are not prerequisites. Cargo's `build.rs` generates language API data in the build output directory and losslessly compresses the Mermaid script, checking the restored bytes after compression.

The language, character, and Unicode catalog scripts in `scripts/` maintain resources. Keep source and license notices when changing those resources. See [resource trade-offs](DESIGN.md#7-parse-resources-on-demand-without-removing-features) for the design.
