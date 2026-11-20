# Build on macOS

[中文](BUILD_zh.md) · [Overview](README.md) · [Design decisions](DESIGN.md)

## 1. Prepare the environment

### System and tools

| Item | Requirement |
|---|---|
| Example system | macOS 26. The build and packaging examples use deployment target `26.0`. |
| Hardware | An Apple Silicon Mac; `uname -m` must report `arm64`. |
| Developer tools | Xcode Command Line Tools, or full Xcode with its command-line tools configured. The macOS SDK, Clang, and Swift are required. |
| Rust | **1.98.1**, pinned in `rust-toolchain.toml`. Use rustup to manage it. |
| Python | Python 3 for `scripts/package-macos.py`. |
| Network | Required for the initial Rust toolchain and Cargo dependency downloads. |

The packaging script accepts ARM64 Mach-O executables only.

Install the Apple command-line tools:

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

### Keep build files in one workspace

Choose an absolute workspace path. Run this block in each terminal used for installation, builds or packaging, before running any Rust command:

```sh
export WORKSPACE="$HOME/workspace/notepad-matrix-build"
export CARGO_HOME="$WORKSPACE/cargo"
export RUSTUP_HOME="$WORKSPACE/rustup"
export CARGO_TARGET_DIR="$WORKSPACE/target"
export TMPDIR="$WORKSPACE/tmp"
export PATH="$CARGO_HOME/bin:$PATH"
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" "$CARGO_TARGET_DIR" "$TMPDIR" \
  "$WORKSPACE/downloads" "$WORKSPACE/src"
```

| Directory | Contents |
|---|---|
| `$WORKSPACE/cargo` | Cargo configuration, command proxies, registry downloads and Git dependencies. |
| `$WORKSPACE/rustup` | Rust toolchains, installed targets and rustup configuration. |
| `$WORKSPACE/target` | Build artifacts, dependency builds and build-script output. |
| `$WORKSPACE/tmp` | Temporary files for tools that use `TMPDIR`. |
| `$WORKSPACE/downloads` | The rustup installation script. |
| `$WORKSPACE/src` | Source checkout. |
| `$WORKSPACE/dist` | Application bundles and DMGs created by the commands below. |

These variables apply to the current terminal and its child processes. They do not move or delete existing `~/.cargo` or `~/.rustup` directories. Xcode, SDKs and caches managed separately by Apple tools remain outside this layout. See [rustup installation paths](https://rust-lang.github.io/rustup/installation/index.html#choosing-where-to-install) and [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html).

On first use of this workspace, install rustup without changing shell startup files, then install the project toolchain and target:

```sh
curl --proto '=https' --tlsv1.2 -fsSLo "$WORKSPACE/downloads/rustup-init.sh" https://sh.rustup.rs
sh "$WORKSPACE/downloads/rustup-init.sh" -y --no-modify-path --default-toolchain none
rustup toolchain install 1.98.1 --profile minimal --target aarch64-apple-darwin
```

### Source directory

Get the source from the [official project repository](https://github.com/skyfiredao/notepad-matrix):

```sh
git clone https://github.com/skyfiredao/notepad-matrix.git "$WORKSPACE/src/notepad-matrix"
cd "$WORKSPACE/src/notepad-matrix"
```

Enter the project directory containing `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/`, and `assets/`. Run all remaining commands from that directory.

```sh
rustc --version
cargo --version
rustup show active-toolchain
rustup show home
command -v cargo
```

Builds use the repository's `Cargo.lock` and bundled resources.

Automatic build versions require complete Git history. CI fetches it automatically. Local builds use Git HEAD; `NOTEPAD_MATRIX_BUILD_COMMIT` can supply the build commit. Missing or shallow history uses the manifest patch with a build-time warning.

## 2. Build the application

The deployment target `26.0` is also written to `LSMinimumSystemVersion`. Use Xcode or Command Line Tools and an SDK that support macOS 26.

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo fetch --locked
cargo build --locked --release --target aarch64-apple-darwin
```

Output with the workspace environment above:

```text
$WORKSPACE/target/aarch64-apple-darwin/release/notepad-matrix
```

Check its architecture:

```sh
file "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix"
```

The output should identify an ARM64 Mach-O executable. The commands below use the same `CARGO_TARGET_DIR`.

Debug build:

```sh
cargo build --locked --target aarch64-apple-darwin
```

Debug output goes into `$CARGO_TARGET_DIR/aarch64-apple-darwin/debug/`. Release builds use `opt-level=3`; see [release configuration](DESIGN.md#7-parse-resources-on-demand).

## 3. Run locally

Start the editor or open a file:

```sh
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix"
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" ./notes.txt
```

Start an independent instance and skip the automatic session:

```sh
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" -multiInst -nosession ./notes.txt
```

See [command-line usage](README.md#command-line-usage).

## 4. Package the application

Use the same toolchain environment and deployment target to run the repository script:

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
python3 scripts/package-macos.py \
  --binary "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" \
  --output "$WORKSPACE/dist/macos-arm64"
```

Use a new or empty output directory.

Outputs:

```text
$WORKSPACE/dist/macos-arm64/notepad-matrix.app
$WORKSPACE/dist/macos-arm64/notepad-matrix.dmg
```

The script:

1. Checks that the input is an ARM64 Mach-O executable.
2. Reads dependencies with `cargo metadata --locked --offline`, so dependencies must already be downloaded.
3. Uses Swift and system type identifiers to create document type declarations.
4. Checks dependency records in `THIRD_PARTY_NOTICES.md`, reads the executable's `--version`, copies resources and licenses, and writes its numeric version to `Info.plist`.
5. Strips symbols, signs with `--sign-identity` (ad-hoc `-` by default), verifies the signature, and creates a compressed DMG with an Applications link.

Verify the signature and launch the application:

```sh
codesign --verify --deep --strict --verbose=2 "$WORKSPACE/dist/macos-arm64/notepad-matrix.app"
open "$WORKSPACE/dist/macos-arm64/notepad-matrix.app"
```

### Optional App Sandbox development package

Use the same executable with `--sandbox` and a separate empty output directory:

```sh
python3 scripts/package-macos.py \
  --binary "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" \
  --sandbox --sign-identity - \
  --output "$WORKSPACE/dist/macos-arm64-sandbox"
codesign --display --entitlements :- "$WORKSPACE/dist/macos-arm64-sandbox/notepad-matrix.app"
```

Without `--output`, the script writes ordinary packages to `dist/` and sandbox packages to `dist-sandbox/` in the project directory. The examples above set `--output` to keep packages in the workspace. `--sign-identity 'Apple Development: …'` selects an installed signing identity.

The sandbox defaults are `com.apple.security.app-sandbox`, `com.apple.security.files.user-selected.read-write`, and `com.apple.security.print`. `--entitlements /absolute/path/custom.plist` is used with `--sandbox` to merge custom entries; the three base entries must remain `true`.

## 5. Allow a trusted application to run

### Check the download source first

Download from the [official project repository](https://github.com/skyfiredao/notepad-matrix) or a verifiable trusted publisher.

If the publisher provides a SHA-256 checksum, check the downloaded file before installation or local signing. For example:

```sh
shasum -a 256 "$HOME/Downloads/notepad-matrix.dmg"
```

Use the downloaded filename and compare the output with the checksum on the release page.

### Locally built, ad-hoc-signed packages

The packaging script defaults to `codesign --force --sign -`. Verify the signature, then launch through Finder or `open`. For unidentified-developer or missing-notarization warnings, use the per-application approval steps below.

### Downloaded ad-hoc-signed or unsigned versions

After checking the source, checksum, and release notes, copy the `.app` from the disk image to an application directory and try to open it once. If macOS blocks it because of an unidentified developer or missing notarization:

1. Open **System Settings → Privacy & Security**.
2. In **Security**, find the blocked Notepad Matrix application and check its name.
3. Click **Open Anyway** and authenticate when prompted.
4. Click **Open** to create an exception for that application.

Later launches can use a double-click. Updates or replacements may trigger another check. On managed Macs that prevent exceptions, contact the administrator.

If the release notes explicitly identify an unsigned build that needs local ad-hoc signing, first copy the trusted `.app` to a directory you can write, such as `~/Applications/`, then run:

```sh
codesign --force --sign - "$HOME/Applications/notepad-matrix.app"
codesign --verify --deep --strict --verbose=2 "$HOME/Applications/notepad-matrix.app"
open "$HOME/Applications/notepad-matrix.app"
```

For malware or damaged-application warnings, obtain a fresh copy from a trusted source or contact the publisher.

System procedure: [Apple: Safely open apps on your Mac](https://support.apple.com/en-us/102445). Wording and button locations may differ across macOS versions.

## Troubleshooting

| Problem | Check and action |
|---|---|
| Wrong Rust version | Check the project directory and `rustup show active-toolchain`. Install the version in `rust-toolchain.toml`. |
| Rust files appear outside the workspace | Re-run the workspace environment block in the same terminal before installation, fetching, building or packaging. Check that `rustup show home` reports `$RUSTUP_HOME` and `command -v cargo` reports `$CARGO_HOME/bin/cargo`. |
| Missing SDK, Clang, or Swift | Check `xcode-select -p` and the `xcrun` commands above. Finish installing and initializing the Apple developer tools. |
| Cargo download failure | Check the network and Cargo registry configuration. Complete `cargo fetch --locked` before using `--offline`. |
| Running outside macOS fails | Build and run the graphical application on a Mac. |
| Packaging rejects the architecture | Check `file` output and build for `aarch64-apple-darwin`. |
| Packaging reports a missing environment variable | Set `MACOSX_DEPLOYMENT_TARGET` in the same terminal that runs the script. |
| Output directory is not empty | Choose an empty `--output` directory. |
| Signing or launch is blocked | Follow [launch approval](#5-allow-a-trusted-application-to-run). |

## Resource generation

Cargo's `build.rs` generates language API data in the build output directory and losslessly compresses the Mermaid script, checking the restored bytes after compression.

The language, character, and Unicode catalog scripts in `scripts/` maintain resources. Preserve source and license notices. Update `THIRD_PARTY_NOTICES.md` when dependency versions, license terms or bundled resources change. See [resource trade-offs](DESIGN.md#7-parse-resources-on-demand).
