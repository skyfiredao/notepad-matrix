# macOS 构建说明

[English](BUILD.md) · [项目介绍](README_zh.md) · [设计取舍](DESIGN_zh.md)

在 Mac 的终端运行以下命令。

## 1. 准备环境

### 系统和工具

| 项目 | 要求 |
|---|---|
| 示例系统 | macOS 26。下文构建和打包使用部署目标 `26.0`。 |
| 机器 | Apple Silicon Mac，`uname -m` 输出 `arm64`。 |
| 开发工具 | Xcode Command Line Tools，或已配置命令行工具的完整 Xcode。需要 macOS SDK、Clang 和 Swift。 |
| Rust | `rust-toolchain.toml` 固定的 **1.98.1**，推荐用 rustup 管理。 |
| Python | Python 3，用于 `scripts/package-macos.py`；不需要第三方 Python 包。 |
| 网络 | 首次下载 Rust 工具链和 Cargo 依赖时需要。 |

当前打包脚本只接受 ARM64 Mach-O 可执行文件，不生成 Intel 或 Universal 应用。

如未安装 Apple 命令行工具：

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

项目启用了 GPUI 的 `runtime_shaders`，Metal shader 在运行时编译。按当前 feature 配置构建，不需要另写离线 `metal`／`metallib` 编译步骤。

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

`rustc` 应使用项目固定的 1.98.1。常规构建使用仓库中的 `Cargo.lock`，不需要克隆 Notepad++、Scintilla 或其他原版源码，也不需要安装其运行库。

## 2. 构建应用

先选择部署目标。下面使用 macOS 26 作为构建示例，并将部署目标设为 `26.0`；该值也会写入打包后的 `LSMinimumSystemVersion`。请使用支持 macOS 26 的 Xcode／Command Line Tools 和 SDK。仅降低这个数字不能证明旧系统兼容性。

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo fetch --locked
cargo build --locked --release --target aarch64-apple-darwin
```

默认 feature 已包含 `native-ui`。禁用默认 feature 的核心库构建不包含可运行的图形应用。

默认产物：

```text
target/aarch64-apple-darwin/release/notepad-matrix
```

检查产物架构：

```sh
file target/aarch64-apple-darwin/release/notepad-matrix
```

应显示 ARM64 Mach-O 可执行文件。若设置了 `CARGO_TARGET_DIR`，后续所有二进制路径也应改为对应输出目录。

调试构建不加 `--release`：

```sh
cargo build --locked --target aarch64-apple-darwin
```

调试产物位于 `target/aarch64-apple-darwin/debug/`。发布配置使用 `opt-level=3`、ThinLTO、一个 codegen unit 和符号剥离，因此链接耗时与调试构建不同。

## 3. 本机运行

从终端启动，不指定文件时打开编辑器：

```sh
./target/aarch64-apple-darwin/release/notepad-matrix
./target/aarch64-apple-darwin/release/notepad-matrix ./notes.txt
```

需要独立实例，并跳过本次自动会话时：

```sh
./target/aarch64-apple-darwin/release/notepad-matrix -multiInst -nosession ./notes.txt
```

`-nosession` 不隔离配置，也不隔离其他用户数据。配置仍位于 `~/.notepad-matrix/`。完整参数说明见 [README_zh.md](README_zh.md#命令行用法)。

首次构建和运行不需要辅助功能权限。普通编辑、文件打开和打印不依赖 UI 自动化工具；只有执行相应系统用途时才按系统提示处理相关权限。

## 4. 应用打包

使用同一工具链环境和部署目标，调用仓库脚本：

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
python3 scripts/package-macos.py \
  --binary target/aarch64-apple-darwin/release/notepad-matrix \
  --output dist/macos-arm64
```

输出目录必须不存在或为空。脚本拒绝覆盖已有产物；再次打包时选择新的空目录，不需要清理 Cargo 缓存。

生成：

```text
dist/macos-arm64/notepad-matrix.app
dist/macos-arm64/notepad-matrix.dmg
```

脚本执行这些步骤：

1. 检查输入为 ARM64 Mach-O。
2. 通过 `cargo metadata --locked --offline` 读取依赖，要求依赖已经下载。
3. 使用 Swift 和系统类型标识生成文档类型声明。
4. 复制可执行文件、图标、主题和依赖许可证，生成 `Info.plist`。
5. 剥离符号，执行 ad-hoc 签名，生成含 Applications 链接的压缩 DMG。

检查签名并启动应用：

```sh
codesign --verify --deep --strict --verbose=2 dist/macos-arm64/notepad-matrix.app
open dist/macos-arm64/notepad-matrix.app
```

ad-hoc 签名不是 Developer ID 签名，脚本也不执行公证。不能把本机可启动等同于可直接在其他 Mac 上通过 Gatekeeper 验证。对外分发需另行配置开发者签名和公证，不通过关闭系统安全机制解决。

## 5. 允许运行可信应用

### 先核对下载来源

只从[项目官方仓库](https://github.com/skyfiredao/notepad-matrix)提供的下载入口，或发布者身份和文件完整性均可核验的可信渠道获取应用。不要使用不明网盘、广告下载器或被重新打包的镜像。未签名、临时签名和未公证的软件都有额外风险。

如果发布方提供 SHA-256，先核对下载文件，再安装或补签。例如：

```sh
shasum -a 256 "$HOME/Downloads/notepad-matrix.dmg"
```

文件名按实际下载修改，将输出与可信发布页面公布的值比较。哈希相同只说明文件与所公布的内容一致，不代替对发布者的信任，也不证明文件没有恶意内容。

### 本机构建的临时签名包

打包脚本已经执行 `codesign --force --sign -`，正常情况下不需要再次签名。先运行上节的 `codesign --verify`，然后用 Finder 或 `open` 启动。如果系统仍提示“无法验证开发者”或“Apple 无法检查是不是包含恶意软件”，确认这是自己构建的应用后，按下述单应用授权步骤操作。

临时签名提供本地签名完整性检查，但不证明开发者身份，不等于 Apple 公证，也不会自动获得 Gatekeeper 放行。

### 下载的临时签名或无签名版本

确认来源、校验值和发布说明后，将 `.app` 从磁盘映像复制到应用目录，再尝试打开一次。因开发者身份或缺少公证被阻止时：

1. 打开“系统设置”→“隐私与安全性”。
2. 在“安全性”区域找到刚被阻止的 Notepad Matrix，核对应用名称。
3. 点按“仍要打开”（Open Anyway），按系统提示认证。
4. 再次核对警告并点按“打开”。这是针对该应用的例外，不是关闭所有应用的安全检查。

后续通常可直接双击启动。更新或替换应用后，系统可能重新检查。受组织管理的 Mac 可能不允许添加例外，此时联系管理员，不绕过管理策略。

如果发布说明明确提供的是无签名构建，且需要补本地临时签名，可先把可信的 `.app` 复制到自己可写的目录，例如 `~/Applications/`，再执行：

```sh
codesign --force --sign - "$HOME/Applications/notepad-matrix.app"
codesign --verify --deep --strict --verbose=2 "$HOME/Applications/notepad-matrix.app"
open "$HOME/Applications/notepad-matrix.app"
```

本地补签会修改应用，必须先完成下载校验；它不认证原发布者，也不替代上面的单应用授权。不要给来源不明、签名失效或被篡改的应用补签来掩盖问题。

如果提示“将损坏你的电脑”、检测到恶意软件，或应用已损坏，停止运行并重新从可信渠道获取，必要时联系发布者。不要将此类警告一律当作缺少签名；本文不要求关闭 Gatekeeper 或批量移除下载隔离标记。

系统操作依据：[Apple：在 Mac 上安全地打开 App](https://support.apple.com/zh-cn/102445)。不同 macOS 版本的文字和按钮位置可能不同。

## 故障排查

| 问题 | 检查与处理 |
|---|---|
| Rust 版本不符 | 确认位于项目目录，检查 `rustup show active-toolchain`，安装 `rust-toolchain.toml` 指定版本。不要为绕过错误直接改锁文件或切换 nightly。 |
| 找不到 SDK、Clang 或 Swift | 检查 `xcode-select -p` 与上述 `xcrun` 命令，完成 Apple 开发工具安装和初始化。 |
| Cargo 下载失败 | 检查网络和 Cargo registry 配置，先完成 `cargo fetch --locked`。缓存未齐时不要使用 `--offline` 构建。 |
| 非 macOS 环境运行失败 | 在 Mac 上构建和运行图形应用。 |
| 打包提示架构错误 | 检查 `file` 输出，使用 `aarch64-apple-darwin` 构建，勿传入 Intel 二进制。 |
| 打包提示缺少环境变量 | 在调用脚本的同一终端设置 `MACOSX_DEPLOYMENT_TARGET`。 |
| 打包目录非空 | 换一个空的 `--output` 目录，脚本不会覆盖已有包。 |
| 签名或启动受阻 | 先核对来源和完整性，再按上节对单个可信应用授权；临时签名不等于开发者认证或公证。恶意软件／损坏警告不强行放行。 |

## 资源生成

日常构建直接使用仓库内的 `assets/`，不需要先运行资源导入脚本。Cargo 的 `build.rs` 会在构建输出目录生成语言 API 数据并无损压缩 Mermaid 脚本，压缩后会核对还原字节。

`scripts/` 中的语言、字符和 Unicode catalog 脚本用于维护资源，不是首次构建依赖。资源修改时保留相关来源和许可文件。完整职责与取舍见 [DESIGN_zh.md](DESIGN_zh.md#7-按需解析资源不以删除功能换体积)。
