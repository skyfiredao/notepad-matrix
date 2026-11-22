# Design decisions

[中文](DESIGN_zh.md) · [Overview](README.md) · [Build guide](BUILD.md)

## Goals and boundaries

Notepad Matrix reimplements Notepad++ editing workflows on macOS, using upstream behavior and resources as references.

Core logic is separate from the interface.

## Module structure

```text
main.rs
  └─ ui.rs / ui/                   GPUI windows, menus, input, and feature surfaces
       ├─ workspace.rs             Document identity, tabs, two views, and state
       ├─ input.rs / editor.rs      Input actions, selections, transactions, undo
       │    ├─ byte_buffer.rs      Source byte storage
       │    └─ byte_projection.rs  Source-to-display coordinate mapping
       ├─ files.rs / codepages.rs   File identity, formats, encodings, and saving
       ├─ recovery.rs              Session metadata and periodic recovery
       ├─ syntax / lexers / udl    Built-in and user-defined languages
       └─ intelligence / search / macros / hashes / converter and other modules
```

`lib.rs` exposes the core modules. The executable uses macOS and the `native-ui` feature.

## 1. Store bytes, project Unicode for display

`ByteBuffer` stores document contents; editing positions use source byte offsets. `ByteProjection` produces Unicode text for layout and maps source and display coordinates.

Documents can contain embedded NUL, invalid UTF-8, and raw bytes inserted by Converter. Display replacement characters leave source bytes intact.

`ByteBuffer` uses shared nodes and leaves so snapshots can share unchanged data. The display projection uses a Rope and tracks length differences caused by invalid bytes.

Hit testing, selections, composition, search, and recovery convert between source byte offsets, display positions, and macOS UTF-16 coordinates.

Sources: [`byte_buffer.rs`](src/byte_buffer.rs), [`byte_projection.rs`](src/byte_projection.rs), [`editor.rs`](src/editor.rs), [`input.rs`](src/input.rs).

## 2. Separate document identity from view state

`Workspace` identifies documents with `BufferId` and the primary and secondary views with `ViewId`.

One document can appear in both views. Each view manages scrolling, folding, and selections. Window lists, panels, and asynchronous operations recheck document identity before acting.

Sources: [`workspace.rs`](src/workspace.rs), [`split_view.rs`](src/ui/split_view.rs), [`document_session_surface.rs`](src/ui/document_session_surface.rs).

## 3. Convert encodings at explicit boundaries

UTF-8 is the default file format. Loading uses BOM detection, encoding detection settings, and explicit code-page selection. Explicit code pages are decoded into the editing representation; otherwise, the editor can retain raw UTF-8 bytes.

Reloading reads disk contents with the selected encoding; conversion acts on the current document. Available macOS character sets use native conversion; OEM 720 and OEM 858 use Rust mappings. Legacy-encoding output uses replacement or best-fit mappings.

The save path retains file identity and an original-content baseline to check for external changes. Ordinary saves use a temporary file in the same directory followed by replacement. Raw-byte operations act directly on source bytes.

Sources: [`files.rs`](src/files.rs), [`codepages.rs`](src/codepages.rs), [`encoding_surface.rs`](src/ui/encoding_surface.rs).

## 4. Separate document backups from session metadata

Automatic-session JSON stores document, path, encoding, selection, view, and backup-path metadata. Document contents go into separate `backup/filename@timestamp` files. Recovery requires matching metadata and content files.

Periodic snapshots retain the document's modification state. For UTF-8 without a BOM or explicit code page, backups store source bytes directly. Recovery restores selections using source byte coordinates. Save-time backups use `.bak` files.

Configuration updates use locking and temporary-file persistence. Configuration and backup paths validate file types.

Sources: [`config.rs`](src/config.rs), [`recovery.rs`](src/recovery.rs), [`raw_backup.rs`](src/raw_backup.rs), [`save_backup.rs`](src/save_backup.rs).

## 5. Draw the interface with GPUI, use AppKit for system operations

GPUI draws the editor, menus, panels, and most dialogs. AppKit handles file selection, printing, the system clipboard, windows, and system application integration. Mermaid uses WebKit.

Native window resizing can synchronously trigger GPUI callbacks. Fullscreen, Post-It, and distraction-free modes schedule native style and frame changes outside the current window update and restore presentation state on exit. Docking, floating panels, and navigation share layout state.

Trash uses `NSFileManager.trashItemAtURL` under a parent-directory access guard; callers handle confirmation and document closing. External application opening uses `NSWorkspace`.

### Terminal

| Module | Responsibility |
|---|---|
| `ssh_terminal.rs` | Configuration persistence, keys, host trust and SSH network sessions. |
| `terminal_config_surface.rs` | Configuration drafts, password input, color panel and transparency slider. |
| `ssh_terminal_surface.rs` | Connection state, input queues, `vt100` parsing, terminal painting and selection. |

The local entry and F5 call `NSWorkspace.openApplication` to open Terminal.app. Remote entries create or reuse an embedded SSH session by profile ID. File and workspace entries also use Terminal.app.

Each SSH session runs `russh` on a separate thread with a single-threaded tokio runtime. GPUI sends input and resize requests through a command queue and receives status and output through an event queue. `vt100` parses output; a GPUI canvas paints the grid. The target sshd provides the PTY and shell. Networking uses the existing sandbox outbound permission.

Configuration is stored under `ssh_terminal`. Remote records have stable `u64` IDs and store address, account, authentication and appearance fields, with mode fixed to SSH. The panel keeps separate loaded snapshots, pending records and edit drafts. `Settings::save(store, expected)` compares snapshots under the write lock and returns an error on conflict. Legacy local records remain for read/write compatibility; the interface lists remote records only. Loading clears legacy `UserDefinedCommands`, Run-only shortcuts and uppercase `Terminal` settings, retaining other configuration.

Passwords use `NSSecureTextField`, bind to a target profile snapshot and are released after authentication. Profiles share the user-generated, unencrypted Ed25519 private key in the app's private container directory, with directory mode 0700 and file mode 0600. Host keys are pinned by canonical address and port, saved after first confirmation and rejected on change. Existing localhost trust is retained.

`State.active` holds the current `ConnectionState`; `State.parked` stores background states by ID, and `tabs` records opening order. Switching tabs swaps active and parked states, retaining each connection, parser, history, selection and input queue. One polling task services all connections. Address and account changes take effect on reconnect; appearance changes update state directly. Closing or deleting profiles cancels their connections and prompts. Closing the active connection selects a neighboring tab; no remaining connections hides the panel.

`background_color` stores 24-bit RGB and `transparency` stores a percentage from 0–100, defaulting to black and 0%. The shared `NSColorPanel` binds to the edited profile ID and updates its draft only. Saving synchronizes the color; ending editing or changing the color target stops updates. The slider uses Preferences colors and supports keyboard and accessibility value actions. Font changes recalculate cell metrics and SSH PTY dimensions.

A transparent active terminal switches the window to `WindowBackgroundAppearance::Transparent`. The root background is transparent; document and interface regions paint separate opaque surfaces. Terminal backgrounds are painted once per cell, including ANSI backgrounds, with unused edges filled separately. Text, selections, cursors, tabs, buttons and the scrollbar stay opaque. Closing the terminal, setting transparency to zero or entering the Matrix interface restores an opaque window.

Tabs share document-tab font, color and frame drawing, with a 4px top indicator in the configured color. Tabs, status and controls share one row. The target address uses a tooltip; buttons use focus handles for Enter/Space activation. Scrollbars reuse the main editor's 16px width, track, thumb and arrow drawing. The profile list is fixed at 148px, including a 26px header, five 24px rows and borders. Mouse paste reads the shared `ssh_terminal.paste_button`, either `middle` or `right`, defaulting to Right button.

Sources: [`ssh_terminal.rs`](src/ssh_terminal.rs), [`terminal_config_surface.rs`](src/ui/terminal_config_surface.rs), [`ssh_terminal_surface.rs`](src/ui/ssh_terminal_surface.rs), [`ui.rs`](src/ui.rs), [`view_surface.rs`](src/ui/view_surface.rs), [`panel_surface.rs`](src/ui/panel_surface.rs), [`document_map_surface.rs`](src/ui/document_map_surface.rs), [`native_app_open.rs`](src/native_app_open.rs), [`folder_workspace.rs`](src/folder_workspace.rs), [`child_process.rs`](src/child_process.rs).

## 6. Compile built-in modules

Converter, MIME Tools, and HTML file export compile into the application and use the Modules menu. They use Rust editing transactions, raw-byte interfaces, and the panel system.

Sources: [`converter.rs`](src/converter.rs), [`mime.rs`](src/mime.rs), [`html_export.rs`](src/html_export.rs), [`macros.rs`](src/macros.rs), [`shortcuts.rs`](src/shortcuts.rs).

## 7. Parse resources on demand

The build generates embedded API data split by language for on-demand parsing. Read-only theme and style data is shared and copied when modified. The Mermaid script is losslessly compressed and decompressed for a separate WebKit preview when needed. WebKit uses separate process memory.

Release builds use speed optimization, ThinLTO, one codegen unit, and symbol stripping.

Build versions use `<major>.<minor>.<count>+<SHA7>`. Major/minor come from `Cargo.toml`; the count includes only subsequent commits affecting this project after the current series was introduced. A new series resets the count. The version is embedded at build time and shared by `--version`, About and debug information. Packaging reads the supplied executable's version, not the current source checkout. Both bundle version fields use its numeric part, as required by macOS.

Sources: [`build.rs`](build.rs), [`build_support/intelligence.rs`](build_support/intelligence.rs), [`intelligence.rs`](src/intelligence.rs), [`style_config.rs`](src/style_config.rs), [`mermaid.rs`](src/ui/mermaid.rs).

## 8. Keep Matrix animation separate from document contents

`matrix_surface` displays a GPUI animation over the main window and retains the document state in `Workspace`. `Matrix.style` is sampled when opened and selects `matrix` (default and fallback) or `terminal`. Rendering stops requesting frames when hidden or inactive. The simulated terminal follows [genact](https://github.com/svenstaro/genact).

See [`matrix_terminal.rs`](src/ui/matrix_terminal.rs) for scene scripts, rendering limits, and text layout.

`Matrix.lockScreen` defaults to `false`. Protected mode uses process-wide lock state, hides auxiliary windows, and guards entry points. `matrix_auth` uses macOS LocalAuthentication; a generation counter rejects stale replies. Only a successful current request unlocks the application.

The About countdown uses a seven-second monotonic deadline. Closing About or ending the animation cancels the task and advances its generation. Each update checks the generation and lock state; expiry closes only About.

See [usage](README.md#matrix-easter-egg-and-screen-lock).

Sources: [`matrix_surface.rs`](src/ui/matrix_surface.rs), [`matrix_rain.rs`](src/ui/matrix_rain.rs), [`matrix_terminal.rs`](src/ui/matrix_terminal.rs), [`matrix_auth.rs`](src/ui/matrix_auth.rs).

## 9. Keep sandbox access separate from document formats

`file_access` restores security-scoped URL bookmarks from private `file-access.json`. Document handles retain their access scopes. Atomic saves, sibling backups and directory operations obtain parent-directory access separately. Missing or expired access uses a native selection panel; background monitoring restores existing access. Sessions and projects retain their JSON formats. Cancelling access preserves recovery files.

User-selected and dropped folders acquire directory access before traversal. Sandbox configuration and recovery data use the container home, separate from unsandboxed paths.

Sandbox instance launches pass a private, one-use request file through `NSWorkspace`. Each request owns its transfer scopes until completion. The receiving instance confirms document opening; moving a tab leaves the source open on cancellation, failure or timeout.

Sources: [`file_access.rs`](src/file_access.rs), [`launch.rs`](src/launch.rs), [`native_app_launch.rs`](src/ui/native_app_launch.rs), [`native_app_open.rs`](src/native_app_open.rs).
