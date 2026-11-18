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

F5 expands commands into a private, self-removing `.command` file. Sandboxed launches hand the file to Terminal through `NSWorkspace.openURLs`; unsandboxed launches use a child process. The asynchronous result reports application handoff. See [`native_app_open.rs`](src/native_app_open.rs) for launch options and completion handling.

Sources: [`ui.rs`](src/ui.rs), [`view_surface.rs`](src/ui/view_surface.rs), [`panel_surface.rs`](src/ui/panel_surface.rs), [`document_map_surface.rs`](src/ui/document_map_surface.rs), [`native_app_open.rs`](src/native_app_open.rs), [`folder_workspace.rs`](src/folder_workspace.rs), [`run_commands.rs`](src/run_commands.rs).

## 6. Compile built-in modules

Converter, MIME Tools, and HTML file export compile into the application and use the Plugins menu. They use Rust editing transactions, raw-byte interfaces, and the panel system.

Run executes external commands separately from built-in modules. See [configuration files](README.md#configuration).

Sources: [`converter.rs`](src/converter.rs), [`mime.rs`](src/mime.rs), [`html_export.rs`](src/html_export.rs), [`macros.rs`](src/macros.rs), [`shortcuts.rs`](src/shortcuts.rs), [`run_commands.rs`](src/run_commands.rs).

## 7. Parse resources on demand

The build generates embedded API data split by language for on-demand parsing. Read-only theme and style data is shared and copied when modified. The Mermaid script is losslessly compressed and decompressed for a separate WebKit preview when needed. WebKit uses separate process memory.

Release builds use speed optimization, ThinLTO, one codegen unit, and symbol stripping.

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
