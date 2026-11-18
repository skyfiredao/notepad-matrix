# macOS 构建说明

[English](BUILD.md) · [项目介绍](README_zh.md) · [设计取舍](DESIGN_zh.md)

## 1. 准备环境

### 系统和工具

| 项目 | 要求 |
|---|---|
| 示例系统 | macOS 26。下文构建和打包使用部署目标 `26.0`。 |
| 机器 | Apple Silicon Mac，`uname -m` 输出 `arm64`。 |
| 开发工具 | Xcode Command Line Tools，或已配置命令行工具的完整 Xcode。需要 macOS SDK、Clang 和 Swift。 |
| Rust | `rust-toolchain.toml` 固定的 **1.98.1**，推荐用 rustup 管理。 |
| Python | Python 3，用于 `scripts/package-macos.py`。 |
| 网络 | 首次下载 Rust 工具链和 Cargo 依赖时需要。 |

打包脚本只接受 ARM64 Mach-O 可执行文件。

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
python3 --version
```

通过 [rustup.rs](https://rustup.rs/) 安装 rustup，然后安装项目工具链和目标：

```sh
rustup toolchain install 1.98.1 --profile minimal
rustup target add aarch64-apple-darwin --toolchain 1.98.1
```

### 源码目录

从[官方项目仓库](https://github.com/skyfiredao/notepad-matrix)获取源码：

```sh
git clone https://github.com/skyfiredao/notepad-matrix.git
```

进入同时包含 `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、`src/` 和 `assets/` 的项目目录。下文命令均从该目录执行。

```sh
rustc --version
cargo --version
rustup show active-toolchain
```

构建使用仓库中的 `Cargo.lock` 和内嵌资源。

## 2. 构建应用

部署目标 `26.0` 也会写入 `LSMinimumSystemVersion`。使用支持 macOS 26 的 Xcode／Command Line Tools 和 SDK。

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo fetch --locked
cargo build --locked --release --target aarch64-apple-darwin
```

默认产物：

```text
target/aarch64-apple-darwin/release/notepad-matrix
```

检查产物架构：

```sh
file target/aarch64-apple-darwin/release/notepad-matrix
```

应显示 ARM64 Mach-O 可执行文件。若设置了 `CARGO_TARGET_DIR`，后续所有二进制路径也应改为对应输出目录。

调试构建：

```sh
cargo build --locked --target aarch64-apple-darwin
```

调试产物位于 `target/aarch64-apple-darwin/debug/`。发布构建使用 `opt-level=3`，见[发布配置](DESIGN_zh.md#7-按需解析资源)。

## 3. 本机运行

启动编辑器或打开文件：

```sh
./target/aarch64-apple-darwin/release/notepad-matrix
./target/aarch64-apple-darwin/release/notepad-matrix ./notes.txt
```

需要独立实例，并跳过本次自动会话时：

```sh
./target/aarch64-apple-darwin/release/notepad-matrix -multiInst -nosession ./notes.txt
```

见[命令行用法](README_zh.md#命令行用法)。

## 4. 应用打包

使用同一工具链环境和部署目标，调用仓库脚本：

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
python3 scripts/package-macos.py \
  --binary target/aarch64-apple-darwin/release/notepad-matrix \
  --output dist/macos-arm64
```

使用新建或空的输出目录。

生成：

```text
dist/macos-arm64/notepad-matrix.app
dist/macos-arm64/notepad-matrix.dmg
```

脚本执行这些步骤：

1. 检查输入为 ARM64 Mach-O。
2. 通过 `cargo metadata --locked --offline` 读取依赖，要求依赖已经下载。
3. 使用 Swift 和系统类型标识生成文档类型声明。
4. 核对 `THIRD_PARTY_NOTICES.md` 中的依赖记录，复制可执行文件、图标、主题、`LICENSE` 和 `THIRD_PARTY_NOTICES.md`，生成 `Info.plist`。
5. 剥离符号，使用 `--sign-identity` 签名（默认 ad-hoc `-`），检查签名，生成含 Applications 链接的压缩 DMG。

检查签名并启动应用：

```sh
codesign --verify --deep --strict --verbose=2 dist/macos-arm64/notepad-matrix.app
open dist/macos-arm64/notepad-matrix.app
```

### 可选的 App Sandbox 开发包

同一可执行文件可用 `--sandbox` 打成沙盒包，输出到另一个空目录：

```sh
python3 scripts/package-macos.py \
  --binary target/aarch64-apple-darwin/release/notepad-matrix \
  --sandbox --sign-identity - \
  --output dist/macos-arm64-sandbox
codesign --display --entitlements :- dist/macos-arm64-sandbox/notepad-matrix.app
```

普通包默认输出到 `dist/`，沙盒包默认输出到 `dist-sandbox/`。`--sign-identity 'Apple Development: …'` 选择已安装的签名身份。

沙盒默认启用 `com.apple.security.app-sandbox`、`com.apple.security.files.user-selected.read-write` 和 `com.apple.security.print`。`--entitlements /absolute/path/custom.plist` 配合 `--sandbox` 合并自定义条目；三个基础项保持 `true`。

## 5. 允许运行可信应用

### 先核对下载来源

从[项目官方仓库](https://github.com/skyfiredao/notepad-matrix)或可核验的可信发布者下载。

如果发布方提供 SHA-256，先核对下载文件，再安装或补签。例如：

```sh
shasum -a 256 "$HOME/Downloads/notepad-matrix.dmg"
```

使用实际下载文件名，将输出与发布页面的校验值比较。

### 本机构建的临时签名包

打包脚本默认执行 `codesign --force --sign -`。检查签名后，用 Finder 或 `open` 启动。遇到开发者身份或缺少公证的警告时，按下述单应用授权步骤操作。

### 下载的临时签名或无签名版本

确认来源、校验值和发布说明后，将 `.app` 从磁盘映像复制到应用目录，再尝试打开一次。因开发者身份或缺少公证被阻止时：

1. 打开“系统设置”→“隐私与安全性”。
2. 在“安全性”区域找到刚被阻止的 Notepad Matrix，核对应用名称。
3. 点按“仍要打开”（Open Anyway），按系统提示认证。
4. 点按“打开”，为该应用添加例外。

后续可双击启动。更新或替换应用后，系统可能重新检查。受管理的 Mac 若禁止添加例外，联系管理员。

如果发布说明明确提供的是无签名构建，且需要补本地临时签名，可先把可信的 `.app` 复制到自己可写的目录，例如 `~/Applications/`，再执行：

```sh
codesign --force --sign - "$HOME/Applications/notepad-matrix.app"
codesign --verify --deep --strict --verbose=2 "$HOME/Applications/notepad-matrix.app"
open "$HOME/Applications/notepad-matrix.app"
```

遇到恶意软件或应用损坏警告，从可信渠道重新获取或联系发布者。

系统操作依据：[Apple：在 Mac 上安全地打开 App](https://support.apple.com/zh-cn/102445)。不同 macOS 版本的文字和按钮位置可能不同。

## 故障排查

| 问题 | 检查与处理 |
|---|---|
| Rust 版本不符 | 确认位于项目目录，检查 `rustup show active-toolchain`，安装 `rust-toolchain.toml` 指定版本。 |
| 找不到 SDK、Clang 或 Swift | 检查 `xcode-select -p` 与上述 `xcrun` 命令，完成 Apple 开发工具安装和初始化。 |
| Cargo 下载失败 | 检查网络和 Cargo registry 配置，先完成 `cargo fetch --locked` 再使用 `--offline`。 |
| 非 macOS 环境运行失败 | 在 Mac 上构建和运行图形应用。 |
| 打包提示架构错误 | 检查 `file` 输出，使用 `aarch64-apple-darwin` 构建。 |
| 打包提示缺少环境变量 | 在调用脚本的同一终端设置 `MACOSX_DEPLOYMENT_TARGET`。 |
| 打包目录非空 | 换一个空的 `--output` 目录。 |
| 签名或启动受阻 | 按[运行授权](#5-允许运行可信应用)处理。 |

## 资源生成

Cargo 的 `build.rs` 在构建输出目录生成语言 API 数据并无损压缩 Mermaid 脚本，压缩后核对还原字节。

`scripts/` 中的语言、字符和 Unicode catalog 脚本用于维护资源。保留来源和许可声明。依赖版本、许可条款或内嵌资源变化时，更新 `THIRD_PARTY_NOTICES.md`。见[资源取舍](DESIGN_zh.md#7-按需解析资源)。
