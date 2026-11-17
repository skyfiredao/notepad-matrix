# Design decisions

[中文](DESIGN_zh.md) · [Overview](README.md) · [Build guide](BUILD.md)

## Goals and boundaries

Notepad Matrix reimplements Notepad++ editing workflows on macOS. Upstream behavior and resources serve as references; Windows APIs, message loops, and binary runtime libraries are not part of the application.

Core logic is separate from the interface to reduce platform coupling. The project does not implement the Windows DLL plugin ABI, a plugin marketplace, the upstream updater, or a default Windows ANSI code page (ACP).

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

Most modules exposed by `lib.rs` do not depend on GPUI. The `ui` module compiles on macOS with `native-ui` enabled. The executable requires that feature and reports an error when started on another operating system.

## 1. Store bytes, project Unicode for display

**Decision:** `ByteBuffer` stores document contents. Editing positions use source byte offsets. `ByteProjection` produces Unicode text for layout and maps differences between source and display coordinates.

A `String`-only model requires valid UTF-8 after every load and edit. An editor also needs to handle embedded NUL, invalid UTF-8, and raw bytes inserted by Converter. Displaying a replacement character must not overwrite those source bytes.

`ByteBuffer` uses shared nodes and leaves so snapshots can share unchanged data. The display projection uses a Rope and tracks length differences caused by invalid bytes.

**Cost:** Source byte positions, display positions, and the UTF-16 coordinates used by macOS input APIs need explicit conversion. Hit testing, selections, composition, search, and recovery must respect those boundaries. Preserving bytes does not determine the intended encoding of damaged text.

Sources: [`byte_buffer.rs`](src/byte_buffer.rs), [`byte_projection.rs`](src/byte_projection.rs), [`editor.rs`](src/editor.rs), [`input.rs`](src/input.rs).

## 2. Separate document identity from view state

**Decision:** `Workspace` identifies documents with `BufferId` and the primary and secondary views with `ViewId`. Tab membership, the active document, and viewport state do not use temporary table row numbers as document identities.

One document can appear in both views. Each view manages scrolling, folding, and selections through its editor and state interfaces. Window lists, panels, and asynchronous operations check document identity again before acting, so sorting or closing a row cannot redirect an operation to another document.

**Cost:** View changes, saving, closing, and recovery must handle more than the currently visible editor.

Sources: [`workspace.rs`](src/workspace.rs), [`split_view.rs`](src/ui/split_view.rs), [`document_session_surface.rs`](src/ui/document_session_surface.rs).

## 3. Convert encodings at explicit boundaries

**Decision:** UTF-8 is the default file format. Loading respects BOM detection, encoding detection settings, and explicit code-page selection. An explicit code page is decoded into the editing representation; without one, the editor can retain raw UTF-8 bytes.

Reloading disk contents with a selected encoding differs from converting the current document to another encoding. These operations change data and format state, not just a status label. Available macOS character sets use native conversion; OEM 720 and OEM 858 use Rust mappings. Windows ACP is not an implicit fallback.

The save path retains file identity and an original-content baseline to check for external changes. Ordinary saves use a temporary file in the same directory followed by replacement. Raw-byte operations do not pass through the lossy display projection.

**Cost:** Explicit code-page output may use replacement or best-fit mappings. Saving to a legacy encoding cannot preserve every Unicode character.

Sources: [`files.rs`](src/files.rs), [`codepages.rs`](src/codepages.rs), [`encoding_surface.rs`](src/ui/encoding_surface.rs).

## 4. Separate document backups from session metadata

**Decision:** Automatic-session JSON stores document, path, encoding, selection, view, and backup-path metadata. Document contents go into separate `backup/filename@timestamp` files.

A periodic snapshot is not a formal save and does not mark the document as saved. For UTF-8 without a BOM or explicit code page, backups store source bytes directly. Recovery restores selections using source byte coordinates. Save-time `.bak` backups serve a separate purpose.

Configuration updates use locking and temporary-file persistence. Configuration and backup paths reject unsupported file types, including symbolic links where regular files are required.

**Cost:** Recovery requires matching metadata and content files. Copying JSON alone does not include unsaved document contents. Periodic recovery is not a substitute for a separate file backup.

Sources: [`config.rs`](src/config.rs), [`recovery.rs`](src/recovery.rs), [`raw_backup.rs`](src/raw_backup.rs), [`save_backup.rs`](src/save_backup.rs).

## 5. Draw the interface with GPUI, use AppKit for system operations

**Decision:** GPUI draws the editor, menus, panels, and most dialogs. AppKit handles file selection, printing, the system clipboard, windows, and system application integration. Mermaid uses WebKit.

Reusing upstream Windows windows or runtime libraries would not provide native input and window behavior. Replacing every system operation with custom controls would lose system file-selection and printing facilities.

Native window resizing can synchronously trigger GPUI callbacks. Fullscreen, Post-It, and distraction-free modes schedule native style and frame changes outside the current window update, preserving window and application presentation state for exit. Docking, floating panels, and navigation share the actual layout state rather than a separate screenshot model.

**Cost:** Rust editing state and AppKit main-thread state need an explicit boundary. macOS window frames and font metrics differ from Windows.

Sources: [`ui.rs`](src/ui.rs), [`view_surface.rs`](src/ui/view_surface.rs), [`panel_surface.rs`](src/ui/panel_surface.rs), [`document_map_surface.rs`](src/ui/document_map_surface.rs).

## 6. Compile built-in modules instead of loading external plugins

**Decision:** Converter, MIME Tools, and HTML file export compile into the application. They retain the familiar Plugins menu grouping without DLL loading, plugin installation, or an update framework.

These modules use Rust editing transactions, raw-byte interfaces, and the panel system. This avoids the Windows plugin ABI and an external binary-loading boundary.

**Cost:** Third-party Notepad++ plugins cannot be installed directly. Adding a module requires a code change and rebuild.

Macro actions live in `macros.json`. `shortcuts.xml` binds macro shortcuts by name and stores Run commands and shortcut overrides. This is not a full implementation of upstream XML Actions. Run is an explicit external-command execution feature, separate from built-in modules.

Sources: [`converter.rs`](src/converter.rs), [`mime.rs`](src/mime.rs), [`html_export.rs`](src/html_export.rs), [`macros.rs`](src/macros.rs), [`shortcuts.rs`](src/shortcuts.rs), [`run_commands.rs`](src/run_commands.rs).

## 7. Parse resources on demand without removing features

**Decision:** The build generates embedded API data split by language, and the application parses it on demand. Read-only theme and style data is shared and copied when modified. The Mermaid script is embedded with lossless compression and decompressed for a separate WebKit preview when needed.

Startup does not parse every language definition or require an upstream resource directory at runtime. Release builds use speed optimization, ThinLTO, one codegen unit, and symbol stripping rather than `opt-level=z`.

**Cost:** Complete resources still occupy binary space. First use of a language or Mermaid incurs parsing or decompression. WebKit also has a separate process memory cost. Size, memory, and speed changes need measurement rather than fixed benefit claims.

Sources: [`build.rs`](build.rs), [`build_support/intelligence.rs`](build_support/intelligence.rs), [`intelligence.rs`](src/intelligence.rs), [`style_config.rs`](src/style_config.rs), [`mermaid.rs`](src/ui/mermaid.rs).

## 8. Keep Matrix animation separate from document contents

About displays a 128×128 logo using high-quality, premultiplied-alpha PNG variants at 128, 256, and 768 pixels, selected for the displayed size and window scale. The title and version sit beside the logo, with the project link and notice below. The window remains 420×385; the application icon remains `assets/notepad-matrix.icns`.

**Decision:** `matrix_surface` uses an `active` boolean to display an animation across the current main window without changing `Workspace`. `Matrix.style` selects `matrix` (default and fallback) or `terminal`, sampled when opened. `matrix_rain` draws character rain; `matrix_terminal` cycles through Cargo, CMake, Docker, Maven, package installation, and asset-download scenes using shared output/progress steps. History is capped at 128 logical lines, each at most 1,024 characters. Long commands soft-wrap to the available width, up to 180 columns, with scrolling based on visual rows and cached shaping for visible lines. Both renderers use GPUI, stop requesting frames when hidden or inactive, and perform no terminal, process, network, or document operations. The green circled toolbar **i** and **? → About** use the same guarded entry point.

Each terminal opening uses a fresh randomized seed to select the first scene and vary progress, then follows the six-scene cycle. Maven simulates dependency retrieval, javac, test summaries, and JAR packaging. Cargo, C/C++, Docker, and Java output include verbose command lines. These are simplified scripts rather than genact's full generated datasets; Java and Maven are not runtime dependencies.

`Matrix.lockScreen` defaults to `false` and is exposed as **Preferences → General → Matrix screen lock**. With protection off, closing About clears `active` and restores the document interface. Protected mode uses shared process-wide lock state, native hiding of auxiliary windows, and entry-point guards; closing About leaves the animation active. Clicking the animation calls `matrix_auth` and the macOS LocalAuthentication framework. The system selects Touch ID or another configured method. An authentication generation counter rejects stale replies. Only a successful current request unlocks the application and clears `active`; failure or cancellation keeps it locked.

When protected About first renders, one task updates the close-button countdown against a fixed seven-second monotonic deadline. Closing About or ending the animation cancels it and advances its generation. Each update checks the generation and active lock; expiry closes only About and never authenticates or unlocks. About uses a shared 420×385 size and tighter side margins. Terminal output follows [genact's simulated activity](https://github.com/svenstaro/genact), without running genact.

The About dialog follows the upstream compact layout. All Preferences pages reduce space between groups and balance group placement without changing default font sizes. User-facing behavior is described in the [README](README.md#matrix-easter-egg-and-screen-lock).

Sources: [`matrix_surface.rs`](src/ui/matrix_surface.rs), [`matrix_rain.rs`](src/ui/matrix_rain.rs), [`matrix_terminal.rs`](src/ui/matrix_terminal.rs), [`matrix_auth.rs`](src/ui/matrix_auth.rs).

## Distribution

The packaging script produces an ARM64 application and disk image, includes shared legal files (`LICENSE` and `THIRD_PARTY_NOTICES.md`), and applies an ad-hoc signature. Identical terms are shared; component copyright and additional notices remain distinct. The About license link opens the public release repository at `https://github.com/skyfiredao/notepad-matrix#license`. Source code and build scripts remain in the repository, not the application bundle. The script does not produce Intel or Universal packages, apply Developer ID signatures, or notarize builds. See [BUILD.md](BUILD.md) for the procedure.
