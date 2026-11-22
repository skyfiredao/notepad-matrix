# macOS 构建说明

[English](BUILD.md) · [项目介绍](README_zh.md) · [设计取舍](DESIGN_zh.md)

## 1. 准备环境

### 系统和工具

| 项目 | 要求 |
|---|---|
| 示例系统 | macOS 26。下文构建使用部署目标 `26.0`。 |
| 机器 | Apple Silicon Mac，`uname -m` 输出 `arm64`。 |
| 开发工具 | Xcode Command Line Tools，或已配置命令行工具的完整 Xcode。需要 macOS SDK、Clang 和 Swift。 |
| Rust | `rust-toolchain.toml` 固定的 **1.98.1**，推荐用 rustup 管理。 |
| 网络 | 首次下载 Rust 工具链和 Cargo 依赖时需要。 |

安装 Apple 命令行工具：

```sh
xcode-select --install
```

安装完成后检查：

```sh
uname -m
xcode-select -p
xcrun --sdk macosx --show-sdk-path
xcrun --find clang
xcrun --find swift
```

### 将构建文件集中到 workspace

指定 workspace 的绝对路径。每次打开用于安装工具链、下载依赖或构建的终端，都先执行以下环境设置，再运行 Rust 命令：

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

| 目录 | 内容 |
|---|---|
| `$WORKSPACE/cargo` | Cargo 配置、命令代理、registry 下载和 Git 依赖。 |
| `$WORKSPACE/rustup` | Rust 工具链、已安装目标和 rustup 配置。 |
| `$WORKSPACE/target` | 编译产物、依赖编译结果和构建脚本输出。 |
| `$WORKSPACE/tmp` | 遵循 `TMPDIR` 的工具产生的临时文件。 |
| `$WORKSPACE/downloads` | rustup 安装脚本。 |
| `$WORKSPACE/src` | 源码仓库。 |

这些变量仅作用于当前终端及其子进程，不会移动或删除已有的 `~/.cargo`、`~/.rustup`。Xcode、SDK 和 Apple 工具独立管理的缓存不属于该目录布局。参考 [rustup 安装路径](https://rust-lang.github.io/rustup/installation/index.html#choosing-where-to-install)和 [Cargo 环境变量](https://doc.rust-lang.org/cargo/reference/environment-variables.html)。

首次使用这个 workspace 时，安装 rustup，不修改 shell 启动配置，然后安装项目工具链和目标：

```sh
curl --proto '=https' --tlsv1.2 -fsSLo "$WORKSPACE/downloads/rustup-init.sh" https://sh.rustup.rs
sh "$WORKSPACE/downloads/rustup-init.sh" -y --no-modify-path --default-toolchain none
rustup toolchain install 1.98.1 --profile minimal --target aarch64-apple-darwin
```

### 源码目录

从[官方项目仓库](https://github.com/skyfiredao/notepad-matrix)获取源码：

```sh
git clone https://github.com/skyfiredao/notepad-matrix.git "$WORKSPACE/src/notepad-matrix"
cd "$WORKSPACE/src/notepad-matrix"
```

进入同时包含 `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、`src/` 和 `assets/` 的项目目录。下文命令均从该目录执行。

```sh
rustc --version
cargo --version
rustup show active-toolchain
rustup show home
command -v cargo
```

构建使用仓库中的 `Cargo.lock` 和内嵌资源。

自动版本需要完整 Git 历史，CI 会自动拉取。本机构建使用 Git HEAD，也可由 `NOTEPAD_MATRIX_BUILD_COMMIT` 提供构建提交。历史缺失或不完整时使用清单中的第三位，并在构建时告警。

## 2. 构建应用

设置部署目标 `26.0`，使用支持 macOS 26 的 Xcode／Command Line Tools 和 SDK。

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo fetch --locked
cargo build --locked --release --target aarch64-apple-darwin
```

使用上述 workspace 环境时，产物位于：

```text
$WORKSPACE/target/aarch64-apple-darwin/release/notepad-matrix
```

检查产物架构：

```sh
file "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix"
```

应显示 ARM64 Mach-O 可执行文件。后续命令沿用同一个 `CARGO_TARGET_DIR`。

调试构建：

```sh
cargo build --locked --target aarch64-apple-darwin
```

调试产物位于 `$CARGO_TARGET_DIR/aarch64-apple-darwin/debug/`。发布构建使用 `opt-level=3`，见[发布配置](DESIGN_zh.md#7-按需解析资源)。

## 3. 本机运行

启动编辑器或打开文件：

```sh
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix"
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" ./notes.txt
```

需要独立实例，并跳过本次自动会话时：

```sh
"$CARGO_TARGET_DIR/aarch64-apple-darwin/release/notepad-matrix" -multiInst -nosession ./notes.txt
```

见[命令行用法](README_zh.md#命令行用法)。

## 资源生成

Cargo 的 `build.rs` 在构建输出目录生成语言 API 数据并无损压缩 Mermaid 脚本，压缩后核对还原字节。

`scripts/` 中的语言、字符和 Unicode catalog 脚本用于维护资源。保留来源和许可声明。依赖版本、许可条款或内嵌资源变化时，更新 `THIRD_PARTY_NOTICES.md`。见[资源取舍](DESIGN_zh.md#7-按需解析资源)。
