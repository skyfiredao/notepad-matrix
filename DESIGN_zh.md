# 设计与取舍

[English](DESIGN.md) · [项目介绍](README_zh.md) · [构建说明](BUILD_zh.md)

## 目标与边界

Notepad Matrix 在 macOS 上重新实现 Notepad++ 的编辑流程，以原版行为和资源为依据。

核心逻辑与界面分开。

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

`lib.rs` 公开核心模块，可执行程序使用 macOS 和 `native-ui` feature。

## 1. 字节是正文，Unicode 是显示投影

`ByteBuffer` 保存正文，编辑位置使用源字节偏移。`ByteProjection` 生成用于布局的 Unicode 文字，映射源坐标与显示坐标。

文档可包含嵌入 NUL、无效 UTF-8 和 Converter 插入的原始字节。显示替换字符时保留源字节。

`ByteBuffer` 使用共享节点和叶片保存字节，快照可共享未修改的数据。显示投影使用 Rope，并单独处理无效字节产生的长度差异。

命中、选区、组合输入、搜索和恢复需转换源字节偏移、显示位置与 macOS UTF-16 坐标。

源码：[`byte_buffer.rs`](src/byte_buffer.rs)、[`byte_projection.rs`](src/byte_projection.rs)、[`editor.rs`](src/editor.rs)、[`input.rs`](src/input.rs)。

## 2. 文档身份与视图状态分开

`Workspace` 用 `BufferId` 标识文档，用 `ViewId` 标识主、从两个视图。

同一文档可以出现在两个视图中。各视图分别管理滚动、折叠和选区。窗口列表、面板和异步操作在执行前重新确认文档身份。

源码：[`workspace.rs`](src/workspace.rs)、[`split_view.rs`](src/ui/split_view.rs)、[`document_session_surface.rs`](src/ui/document_session_surface.rs)。

## 3. 编码转换只发生在明确边界

默认文件格式为 UTF-8。加载使用 BOM 识别、编码检测设置和显式代码页选择；指定代码页时解码为编辑表示，否则可保留原始 UTF-8 字节。

重新读取按所选编码读取磁盘，编码转换作用于当前文档。macOS 可用字符集使用原生转换，OEM 720 和 OEM 858 使用 Rust 映射。旧编码输出使用替代或最佳拟合映射。

保存保留磁盘身份和原始内容基线，以检查外部变更。普通文件保存使用同目录临时文件与替换流程；原始字节操作直接作用于源字节。

源码：[`files.rs`](src/files.rs)、[`codepages.rs`](src/codepages.rs)、[`encoding_surface.rs`](src/ui/encoding_surface.rs)。

## 4. 正文备份与会话元数据分开

自动会话 JSON 保存文档、路径、编码、选区、视图及备份路径元数据，正文写入独立的 `backup/文件名@时间戳` 文件。恢复依赖匹配的元数据和正文文件。

周期快照保留文档的修改状态。UTF-8 无 BOM 且无显式代码页的备份直接保存源字节；恢复时用源字节坐标恢复选区。保存前备份使用 `.bak` 文件。

配置更新使用锁和临时文件持久化。配置及备份路径执行文件类型检查。

源码：[`config.rs`](src/config.rs)、[`recovery.rs`](src/recovery.rs)、[`raw_backup.rs`](src/raw_backup.rs)、[`save_backup.rs`](src/save_backup.rs)。

## 5. GPUI 绘制界面，AppKit 承接系统用途

GPUI 负责编辑区、菜单、面板和多数对话框。AppKit 负责文件选择、打印、系统剪贴板、窗口和系统应用交互；Mermaid 使用 WebKit。

原生窗口改尺寸可能同步触发 GPUI 回调。全屏、便签和沉浸模式的原生 style/frame 操作排到当前窗口更新之外，退出时恢复显示状态。面板停靠、浮动和导航共享布局状态。

废纸篓在父目录授权守卫内使用 `NSFileManager.trashItemAtURL`；调用者处理确认和关闭文档。外部应用打开使用 `NSWorkspace`。

F5 将命令展开为私有自删除 `.command` 文件。沙盒中通过 `NSWorkspace.openURLs` 交给 Terminal，非沙盒使用子进程。异步结果表示应用交接。启动选项和完成回调见 [`native_app_open.rs`](src/native_app_open.rs)。

源码：[`ui.rs`](src/ui.rs)、[`view_surface.rs`](src/ui/view_surface.rs)、[`panel_surface.rs`](src/ui/panel_surface.rs)、[`document_map_surface.rs`](src/ui/document_map_surface.rs)、[`native_app_open.rs`](src/native_app_open.rs)、[`folder_workspace.rs`](src/folder_workspace.rs)、[`run_commands.rs`](src/run_commands.rs)。

## 6. 内嵌功能模块

Converter、MIME Tools 和 HTML 文件导出随程序编译，使用 Plugins 菜单。模块使用 Rust 编辑事务、原字节接口和面板系统。

Run 执行外部命令，与内嵌模块分开。见[配置文件](README_zh.md#配置)。

源码：[`converter.rs`](src/converter.rs)、[`mime.rs`](src/mime.rs)、[`html_export.rs`](src/html_export.rs)、[`macros.rs`](src/macros.rs)、[`shortcuts.rs`](src/shortcuts.rs)、[`run_commands.rs`](src/run_commands.rs)。

## 7. 按需解析资源

构建时生成按语言拆分的内嵌 API 数据，运行时按需解析。只读主题和样式数据共享，修改时再复制。Mermaid 脚本无损压缩，使用时解压到独立的 WebKit 预览。WebKit 使用独立进程内存。

发布配置使用速度优化、ThinLTO、单 codegen unit 和符号剥离。

构建版本为 `<主版本>.<次版本>.<提交数>+<SHA 前 7 位>`。主、次版本取自 `Cargo.toml`，提交数只统计引入当前版本系列后影响本项目的提交；新系列归零。版本在编译时嵌入，由 `--version`、关于和调试信息共用。打包读取所选二进制的版本，不读取当前源码版本。两个 bundle 版本字段按 macOS 要求只保存数字部分。

源码：[`build.rs`](build.rs)、[`build_support/intelligence.rs`](build_support/intelligence.rs)、[`intelligence.rs`](src/intelligence.rs)、[`style_config.rs`](src/style_config.rs)、[`mermaid.rs`](src/ui/mermaid.rs)。

## 8. Matrix 动画与正文分开

`matrix_surface` 在主窗口上覆盖 GPUI 动画，保留 `Workspace` 中的文档状态。`Matrix.style` 在打开时采样，选择 `matrix`（默认及回退值）或 `terminal`。不可见或应用不活跃时停止续帧。伪终端参考 [genact](https://github.com/svenstaro/genact)。

场景脚本、渲染限制和文字排版见 [`matrix_terminal.rs`](src/ui/matrix_terminal.rs)。

`Matrix.lockScreen` 默认 `false`。保护模式使用进程级锁态，隐藏辅助窗口并守卫操作入口。`matrix_auth` 调用 macOS LocalAuthentication，以认证代次拒绝过期回调；仅当前请求成功后解锁。

关于倒计时使用七秒单调时钟期限。关闭关于或结束动画会取消任务并推进代次；每次更新核对代次及锁态，到时仅关闭关于。

见[使用方式](README_zh.md#matrix-彩蛋与锁屏保护)。

源码：[`matrix_surface.rs`](src/ui/matrix_surface.rs)、[`matrix_rain.rs`](src/ui/matrix_rain.rs)、[`matrix_terminal.rs`](src/ui/matrix_terminal.rs)、[`matrix_auth.rs`](src/ui/matrix_auth.rs)。

## 9. 沙盒授权与文档格式分开

`file_access` 从私有 `file-access.json` 恢复安全作用域 URL 书签，文档句柄持有访问作用域。原子保存、同目录备份和目录操作分别取得父目录授权。缺少授权或授权失效时使用原生选择窗口；后台监控恢复已有授权。会话和项目沿用各自的 JSON 格式，取消授权时保留恢复文件。

用户选择或拖入文件夹时，先获取目录授权再遍历。沙盒配置和恢复数据使用容器主目录，与非沙盒路径分开。

沙盒新实例通过 `NSWorkspace` 传递私有一次性请求文件。每个请求持有传递作用域直到完成，接收实例确认文档打开；移动标签时，取消、失败或超时均保留来源标签。

源码：[`file_access.rs`](src/file_access.rs)、[`launch.rs`](src/launch.rs)、[`native_app_launch.rs`](src/ui/native_app_launch.rs)、[`native_app_open.rs`](src/native_app_open.rs)。
