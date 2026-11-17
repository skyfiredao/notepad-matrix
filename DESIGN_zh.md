# 设计与取舍

[English](DESIGN.md) · [项目介绍](README_zh.md) · [构建说明](BUILD_zh.md)

## 目标与边界

Notepad Matrix 在 macOS 上重新实现 Notepad++ 的编辑流程和界面用途。原版行为和资源用于对照，Windows API、消息循环和二进制运行库不进入应用。

核心逻辑与界面分开，降低平台耦合。项目不实现 Windows DLL 插件 ABI、插件市场、原版更新器或默认 Windows ACP。

## 模块结构

```text
main.rs
  └─ ui.rs / ui/                 GPUI 窗口、菜单、输入与功能界面
       ├─ workspace.rs           文档身份、标签、两个视图及文档状态
       ├─ input.rs / editor.rs   输入动作、选区、编辑事务、撤销历史
       │    ├─ byte_buffer.rs    原始字节存储
       │    └─ byte_projection.rs  字节坐标与显示文字坐标映射
       ├─ files.rs / codepages.rs  文件身份、格式、编码与保存
       ├─ recovery.rs            会话元数据与周期恢复
       ├─ syntax / lexers / udl  内建与自定义语言
       └─ intelligence / search / macros / hashes / converter 等功能模块
```

`lib.rs` 的大部分模块不依赖 GPUI。`ui` 仅在 macOS 且启用 `native-ui` 时编译；可执行文件要求该 feature，非 macOS 启动会明确报错。

## 1. 字节是正文，Unicode 是显示投影

**选择：** 正文以 `ByteBuffer` 保存，编辑位置使用源字节偏移。`ByteProjection` 将内容映射到用于布局的 Unicode 文字，并记录源坐标与显示坐标的差异。

只使用 `String` 会要求每次加载和编辑后的正文都是合法 UTF-8。文本编辑器还需要处理嵌入 NUL、无效 UTF-8 和 Converter 插入的原始字节。显示替换字符不应反向改写这些字节。

`ByteBuffer` 使用共享节点和叶片保存字节，快照可共享未修改的数据。显示投影使用 Rope，并单独处理无效字节产生的长度差异。

**代价：** 字节、显示文字和 macOS 输入接口的 UTF-16 坐标不能混用。命中、选区、组合输入、搜索和恢复都需要明确转换边界。原始字节不被替换，不代表对所有损坏文本都能推断正确编码。

源码：[`byte_buffer.rs`](src/byte_buffer.rs)、[`byte_projection.rs`](src/byte_projection.rs)、[`editor.rs`](src/editor.rs)、[`input.rs`](src/input.rs)。

## 2. 文档身份与视图状态分开

**选择：** `Workspace` 用 `BufferId` 标识文档，用 `ViewId` 标识主、从两个视图。标签归属、活动文档和视口状态不以临时列表行号充当身份。

同一文档可以出现在两个视图中。滚动、折叠、选区等状态通过相应视图的编辑和状态接口管理。窗口列表、面板和异步操作在执行时重新确认文档身份，避免排序或关闭后误操作相同行号上的另一份文档。

**代价：** 切换视图、保存、关闭和恢复必须处理多个入口，不能只更新当前可见的编辑区。

源码：[`workspace.rs`](src/workspace.rs)、[`split_view.rs`](src/ui/split_view.rs)、[`document_session_surface.rs`](src/ui/document_session_surface.rs)。

## 3. 编码转换只发生在明确边界

**选择：** 默认文件格式为 UTF-8。文件加载保留 BOM 识别、编码检测设置和显式代码页选择；指定代码页的内容先解码为编辑表示，无显式代码页时允许保留原始 UTF-8 字节。

“选择编码后重新读取磁盘”和“将当前文档转换为另一编码”是不同操作，不能只更改状态栏文字。macOS 可用的字符集使用原生转换能力，OEM 720 和 OEM 858 使用 Rust 映射。Windows 默认 ACP 不作为隐式回退。

保存保留磁盘身份和原始内容基线，以检查外部变更。普通文件保存使用同目录临时文件与替换流程；原始字节入口不先经过有损显示投影。

**代价：** 显式代码页可能发生替代或最佳拟合，不能把“可保存”解释为任意 Unicode 到任意旧编码都无损。

源码：[`files.rs`](src/files.rs)、[`codepages.rs`](src/codepages.rs)、[`encoding_surface.rs`](src/ui/encoding_surface.rs)。

## 4. 正文备份与会话元数据分开

**选择：** 自动会话的 JSON 保存文档、路径、编码、选区、视图及备份路径等元数据，正文写入独立的 `backup/文件名@时间戳` 文件。

周期快照不是正式保存，不把文档标记为已保存。UTF-8 无 BOM 且无显式代码页的备份直接保存源字节；恢复后继续用源字节坐标恢复选区。保存前 `.bak` 备份与周期恢复是不同用途。

配置更新使用锁和临时文件持久化。配置及备份路径检查拒绝不符合要求的文件类型，避免把符号链接当作普通配置或备份文件。

**代价：** 恢复依赖元数据和正文文件相互匹配，不能只复制一份 JSON 就认为包含全部未保存内容。周期备份也不能替代用户自己的文件备份。

源码：[`config.rs`](src/config.rs)、[`recovery.rs`](src/recovery.rs)、[`raw_backup.rs`](src/raw_backup.rs)、[`save_backup.rs`](src/save_backup.rs)。

## 5. GPUI 绘制界面，AppKit 承接系统用途

**选择：** GPUI 负责编辑区、菜单、面板和多数对话框。AppKit 负责文件选择、打印、系统剪贴板、窗口和系统应用交互；Mermaid 使用 WebKit。

直接复用原版窗口或 Windows 运行库无法满足本机输入和窗口行为。另一方面，将所有系统用途都画成自定义控件，会丢失系统文件选择和打印等能力。

原生窗口改尺寸可能同步触发 GPUI 回调。全屏、便签和沉浸模式的原生 style/frame 操作排到当前窗口更新之外，并保留退出时的窗口与应用显示状态。面板停靠、浮动和导航共享实际布局状态，不另做只用于截图的模型。

**代价：** 需要维护 Rust 编辑状态与 AppKit 主线程状态的边界。macOS 原生窗口外壳和字体度量不会与 Windows 逐像素一致。

源码：[`ui.rs`](src/ui.rs)、[`view_surface.rs`](src/ui/view_surface.rs)、[`panel_surface.rs`](src/ui/panel_surface.rs)、[`document_map_surface.rs`](src/ui/document_map_surface.rs)。

## 6. 固定模块代替外部插件加载

**选择：** Converter、MIME Tools 和 HTML 文件导出是随程序编译的模块，保留熟悉的 Plugins 菜单分组，不加载 DLL，也不提供插件安装或更新框架。

这样可以复用 Rust 的编辑事务、原字节接口和面板系统，避免引入 Windows 插件 ABI 和任意外部二进制执行边界。

**代价：** 第三方 Notepad++ 插件不能直接安装，增加模块需要修改并重新构建程序。

宏动作使用 `macros.json`；`shortcuts.xml` 按名称提供宏快捷键，同时保存 Run 命令和快捷键覆盖。不把该协议描述为完整的原版 XML Action 兼容层。Run 是显式的外部命令执行入口，不与内嵌模块混为一谈。

源码：[`converter.rs`](src/converter.rs)、[`mime.rs`](src/mime.rs)、[`html_export.rs`](src/html_export.rs)、[`macros.rs`](src/macros.rs)、[`shortcuts.rs`](src/shortcuts.rs)、[`run_commands.rs`](src/run_commands.rs)。

## 7. 按需解析资源，不以删除功能换体积

**选择：** 构建时生成按语言拆分的内嵌 API 数据，运行时按需解析。只读主题和样式数据共享，修改时再复制。Mermaid 脚本无损压缩后嵌入，使用时解压到独立的 WebKit 预览。

不在启动时解析所有语言数据，也不要求用户运行时安装原版资源目录。发布配置使用速度优化、ThinLTO、单 codegen unit 和符号剥离，而不是以 `opt-level=z` 为默认目标。

**代价：** 完整资源仍占二进制空间，首次使用某语言或 Mermaid 需要解析／解压。Mermaid 的 WebKit 进程也有独立内存成本。没有测量时，不给出固定的体积、内存或速度收益承诺。

源码：[`build.rs`](build.rs)、[`build_support/intelligence.rs`](build_support/intelligence.rs)、[`intelligence.rs`](src/intelligence.rs)、[`style_config.rs`](src/style_config.rs)、[`mermaid.rs`](src/ui/mermaid.rs)。

## 8. Matrix 动画与正文分开

关于显示 128×128 的图片，使用高质量、预乘透明度缩放的 128、256、768 像素 PNG，按显示尺寸和窗口倍率选择。标题和版本位于图片右侧，项目链接和说明位于下方。窗口保持 420×385；应用图标仍使用 `assets/notepad-matrix.icns`。

**选择：** `matrix_surface` 通过 `active` 显示铺满当前主窗口的动画，不改动 `Workspace`。`Matrix.style` 在打开时采样，选择 `matrix`（默认及回退值）或 `terminal`。`matrix_rain` 绘制字符雨；`matrix_terminal` 使用共用输出／进度步骤，轮播 Cargo、CMake、Docker、Maven、软件包安装和素材下载场景。历史最多 128 条逻辑行，每条最多 1,024 个字符。长命令按可用宽度折行，最多 180 列；按视觉行滚动，仅对可见逻辑行缓存排版。两者使用 GPUI，不可见或应用不活跃时不续帧，不运行终端、进程或网络操作，不写正文。工具栏绿色圆圈 **i** 与 **? → 关于**复用同一受保护入口。

每次打开伪终端时重新生成随机种子，用于选择起始场景和变化进度，之后按六种场景顺序轮播。Maven 模拟依赖下载、javac、测试摘要和 JAR 打包；Cargo、C/C++、Docker 和 Java 输出包含详细长命令。这些仍为简化脚本，未完整移植 genact 的生成数据池；运行不依赖 Java 或 Maven。

`Matrix.lockScreen` 默认 `false`，界面入口为**首选项 → 常用 → Matrix 锁屏保护**。关闭保护时，关闭关于会清除 `active`，恢复文档界面。保护模式使用进程内共享锁态、原生辅助窗口隐藏和入口守卫；关闭关于仍保留动画。点击动画通过 `matrix_auth` 调用 macOS LocalAuthentication 系统框架，由系统选择 Touch ID 等已配置方式。认证代次拒绝过期回调；仅当前请求成功后解锁并清除 `active`，失败或取消保持锁定。

受保护的关于窗口首次渲染时启动一个任务，按固定七秒单调时钟期限逐秒更新关闭按钮上的倒计时。关闭关于或结束动画会取消任务并推进代次；每次更新核对代次及锁态，到时仅关闭关于，不认证或解锁。关于窗口统一使用 420×385 尺寸并收紧左右边距。伪终端参考 [genact 的模拟活动](https://github.com/svenstaro/genact)，不运行 genact。

关于对话框按原版紧凑风格排列。首选项全部页压缩分组之间的留白，平衡分组布局，不改变默认字号设置。使用方式见 [README](README_zh.md#matrix-彩蛋与锁屏保护)。

源码：[`matrix_surface.rs`](src/ui/matrix_surface.rs)、[`matrix_rain.rs`](src/ui/matrix_rain.rs)、[`matrix_terminal.rs`](src/ui/matrix_terminal.rs)、[`matrix_auth.rs`](src/ui/matrix_auth.rs)。

## 分发边界

当前打包脚本生成 ARM64 应用与磁盘映像，加入共用法律文件（`LICENSE` 和 `THIRD_PARTY_NOTICES.md`），并执行 ad-hoc 签名。相同条款合并，各组件的版权和必要附加声明分别保留。关于的许可链接打开公开发布仓库 `https://github.com/skyfiredao/notepad-matrix#license`。源码和构建脚本留在仓库，不加入应用包。脚本不提供 Intel／Universal 产物、Developer ID 签名或公证流程。具体操作见 [BUILD_zh.md](BUILD_zh.md)。
