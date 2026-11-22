# Build on macOS

[中文](BUILD_zh.md) · [Overview](README.md) · [Design decisions](DESIGN.md)

## 1. Prepare the environment

### System and tools

| Item | Requirement |
|---|---|
| Example system | macOS 26. The build examples use deployment target `26.0`. |
| Hardware | An Apple Silicon Mac; `uname -m` must report `arm64`. |
| Developer tools | Xcode Command Line Tools, or full Xcode with its command-line tools configured. The macOS SDK, Clang, and Swift are required. |
| Rust | **1.98.1**, pinned in `rust-toolchain.toml`. Use rustup to manage it. |
| Network | Required for the initial Rust toolchain and Cargo dependency downloads. |

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
```

### Keep build files in one workspace

Choose an absolute workspace path. Run this block in each terminal used for toolchain installation, dependency downloads or builds, before running any Rust command:

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

Set deployment target `26.0` and use Xcode or Command Line Tools with an SDK that supports macOS 26.

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

## Resource generation

Cargo's `build.rs` generates language API data in the build output directory and losslessly compresses the Mermaid script, checking the restored bytes after compression.

The language, character, and Unicode catalog scripts in `scripts/` maintain resources. Preserve source and license notices. Update `THIRD_PARTY_NOTICES.md` when dependency versions, license terms or bundled resources change. See [resource trade-offs](DESIGN.md#7-parse-resources-on-demand).
