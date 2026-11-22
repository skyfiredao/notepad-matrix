<p align="center">
  <img src="assets/notebook_matrix_logo_doc.png" alt="Notepad Matrix logo" width="192" height="192">
</p>

<h1 align="center">Notepad Matrix</h1>

<p align="center">
  <a href="README_zh.md">中文</a> ·
  <a href="BUILD.md">Build guide</a> ·
  <a href="DESIGN.md">Design decisions</a>
</p>

A free, open-source text editor for macOS, built with Rust and GPUI.

Project: [github.com/skyfiredao/notepad-matrix](https://github.com/skyfiredao/notepad-matrix)

## Quick start

On an Apple Silicon Mac running macOS 26, install the prerequisites in [BUILD.md](BUILD.md), then run these commands from the directory containing `Cargo.toml`:

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo build --locked --release --target aarch64-apple-darwin
./target/aarch64-apple-darwin/release/notepad-matrix ./notes.txt
```

## Project background

Notepad Matrix brings classic Windows Notepad++ editing habits to macOS. It is an independent project, not an official Notepad++ release.

## Features

### Files and tabs

- Create, open, save, save as, save a copy, save all, and reload files from disk.
- Rename saved files or untitled tabs, and move disk files to the Trash.
- View recent files, reopen the last closed file, open all recent files, or clear the list.
- Pin tabs, assign tab colors, and sort by name, path, type, content length, or modification time in either order.
- Use multi-line or vertical tabs and switch documents in most-recently-used order.
- Close documents to the left or right, all except the current or pinned tabs, or all unchanged documents.
- Move documents between two editor views, or clone a document to share its contents while browsing each view separately; synchronize horizontal or vertical scrolling.
- Open documents in an independent instance, or move the current document to a new instance.

### Selection and editing

- Cut, copy, paste, delete, select all, undo, and redo, with insert and overwrite modes and input-method composition.
- Use multiple or rectangular selections; add the next or all matches, skip a match, undo the last added selection, or change the main selection.
- Insert repeated text or incrementing numbers with the column editor, setting the starting value, increment, repeat count, base, and leading fill.
- Duplicate lines or selections, move lines up or down, transpose adjacent lines, join or split lines, and insert blank lines above or below.
- Convert to upper, lower, proper, or sentence case, invert case, or randomize case.
- Remove leading or trailing whitespace, empty or whitespace-only lines, and duplicate or consecutive duplicate lines.
- Convert tabs to spaces or spaces to tabs, indent or unindent, and set tab width and indentation by language.
- Sort lines lexically, without case sensitivity, as integers or decimals, by length, or with system locale rules; reverse or shuffle line order.
- Add, toggle, or remove line and block comments for the current language; insert short, long, or custom date and time formats.
- Set in-app read-only status for the current or all documents; separately toggle macOS file write permissions for saved files.
- Preserve selection bytes with Binary Copy/Cut/Paste, and copy the current file's full path, name, or directory.
- Browse byte-to-character mappings for a selected character set in the character panel, and insert characters or numeric representations.

### Find, replace, and navigation

- Search with plain text, extended escape sequences, or regular expressions; set case, whole-word matching, wrapping, direction, and selection scope. Regex mode can let a dot match a newline.
- Find the next or previous match, count matches, and list all results in the current or all open documents; backward regex search is off by default.
- Replace one match, replace all, or replace across all open documents.
- Search and replace in a directory or selected project panels, with filename filters, subdirectory search, and hidden-directory options.
- Track and cancel file searches; group results by search, file, and line, collapse groups, wrap result text, and jump back to source positions.
- Use incremental search, search selected text or the word under the caret, and search character ranges; retain find, replace, filter, and directory history.
- Go to a line number or byte offset, jump to a matching brace, or select the matching brace range.
- Add, clear, invert, and navigate bookmarks; copy, cut, replace, or delete bookmarked lines, or delete unmarked lines.
- Highlight a selection or all matches with five marking styles, navigate each style, clear marks, or copy marked text.
- Display document change history, navigate previous or next changes, and clear the history.

### Languages and encodings

- Detect the language from the file path or select a built-in language; use syntax highlighting and fold the current, all, or specified nesting levels.
- Create, rename, save as, remove, import, and export user-defined languages; configure keywords, comments, numbers, operators, delimiters, folding, and styles.
- Complete words, functions, and paths, display function call tips, and switch between signatures; configure automatic indentation and paired-character insertion.
- Parse the document into a language-specific function list, filter names, and jump to definitions.
- Use UTF-8 by default, UTF-8 BOM, UTF-16 LE BOM, or UTF-16 BE BOM, and explicitly select Windows, OEM, and other character sets.
- Select a source encoding to reload a file or convert its save encoding. Reloading with another encoding asks for confirmation when changes or undo history would be lost.
- Convert between Windows (CRLF), Unix (LF), and classic Mac (CR) line endings.

### Workspaces and panels

- View tabs from both editor views in the document list, or use the document map to browse a miniature overview and locate text.
- Browse a folder tree, open files, and refresh it as directories change; drop files to open them, or drop folders to add them to the workspace or open their files, according to preferences.
- Organize projects, groups, and files in three project panels; edit names and paths, reorder entries, find entries, and save, load, or export workspaces.
- Dock, resize, or float panels, and save their layout and floating-window positions.
- Monitor disk changes, reload content, and follow the file tail for logs that keep growing.

### Sessions and backups

- Save and load sessions, remember open documents and editor-view state, and restore the last session at startup.
- Periodically back up unsaved contents with a configurable interval; store document files separately from session JSON metadata.
- Disable save-before backups or use simple or timestamped verbose backups, with an optional custom save-backup directory.
- Automatically delete expired backups, enabled by default with a 90-day retention period; change the period or turn cleanup off. Expiry uses the last modification time.
- Keep backups needed for recovery, referenced by sessions, or with an unconfirmed identity. Remove unneeded recovery backups after a successful save or explicit discard.
- Ask before quitting with unsaved documents that have no usable recovery backup; save, discard, or cancel. A failed save does not cause an immediate exit.
- Configure the large-file threshold and wrapping, completion, smart highlighting, brace matching, and link behavior; restricted large files do not receive periodic content snapshots or save-before `.bak` files.

### Macros and shortcuts

- Record text-editing actions, stop, play back, and save named macros; repeat a set number of times or until the end of the file.
- Change shortcuts for menu and editor commands, macros, and built-in modules, with conflict display.

### Terminal

- **Terminal → Local Terminal** or F5 always opens Terminal.app directly, without reading SSH settings.
- Add remote SSH servers in **Terminal → Terminal Configuration**, save, then open their connections from the **Terminal** menu.
- Remote terminals support password or public-key login, with configurable font size, background color, transparency and mouse-paste button.

### Preview and tools

- View a read-only Markdown preview that follows the current document, with headings, lists, quotations, code blocks, tables, links, images, and Mermaid diagrams.
- Select and copy preview text, open links, or jump to document anchors; scroll wide tables horizontally on their own.
- Calculate MD5, SHA-1, SHA-256, and SHA-512 hashes for entered text, selections, or selected files; copy selection hashes directly to the clipboard.
- Use Converter's ASCII, decimal, hexadecimal, binary, and octal panel, or convert text to and from hexadecimal.
- Base64-encode selections with or without padding and decode them with MIME Tools; decoded output must be UTF-8 text.
- Export a complete document as a syntax-styled HTML file, including built-in and user-defined languages.

### Display, printing, and macOS integration

- Wrap text, zoom, show indentation guides, and display whitespace, line endings, control characters, and nonprinting characters.
- Use full-screen, always-on-top, Post-it, or distraction-free modes; hide selected lines and view a document summary.
- Use light, dark, or system appearance; adjust fonts, sizes, foregrounds, backgrounds, and language styles in the style configurator, and import themes.
- Switch interface languages, with corresponding right-to-left layouts for menus and preferences.
- Read document length, line count, caret line and column, selection character and line counts, encoding, and line endings in the status bar, with menus for language, encoding, and line endings.
- Customize the editor context menu by editing `contextMenu.xml`; changes take effect after restart.
- Use native file and print dialogs; print the selection or whole document with pagination, syntax colors, line numbers, margins, headers, and footers.
- Reveal files in Finder or open the current file with its default application or a selected browser.
- Open a file path from the document or reveal it in Finder, and search selected text with a chosen search engine or custom search URL.

The Modules menu contains built-in modules. See [platform boundaries](DESIGN.md#goals-and-boundaries).

## Matrix Easter egg and screen lock

Use the **F1** shortcut, **? → About**, or the toolbar's green circled **i** to open the Easter egg. Select **Matrix** (default) or **Simulated terminal** in **Preferences → General → Style**. The terminal does not run real commands.

**In-app locking is off by default.** Enable it in **Preferences → General → Matrix screen lock**. With it off, closing About returns to the document. With it on, opening the Easter egg locks the application; About closes after a seven-second countdown, but the application stays locked. Click the animation and complete macOS system authentication to unlock.

The application lock does not replace the macOS screen lock or encrypt files.

See [design details](DESIGN.md#8-keep-matrix-animation-separate-from-document-contents).

## Command-line usage

```sh
./target/aarch64-apple-darwin/release/notepad-matrix -multiInst -nosession ./notes.txt
./target/aarch64-apple-darwin/release/notepad-matrix -lrust -n20 -c5 ./src/main.rs
./target/aarch64-apple-darwin/release/notepad-matrix -- ./-example.txt
```

| Option | Purpose |
|---|---|
| `--version` | Print the build version and exit without opening a window. |
| `-multiInst` | Start an independent instance. |
| `-nosession` | Do not restore or save the automatic session for this launch. |
| `-openSession session.json` | Open one saved session file. |
| `-l<language>` | Select a built-in language, such as `rust`. |
| `-udl=<name>` | Select an installed user-defined language. Quote names containing spaces. |
| `-n<line>` / `-c<column>` | Set the initial position, starting at 1. |
| `--` | Treat all following arguments as file paths. |

## Configuration

Default configuration directories:

| Build | Directory |
|---|---|
| Sandboxed | `.notepad-matrix/` under the application's container `Data` directory. |
| Unsandboxed | `~/.notepad-matrix/`. |

Select a custom configuration directory in Preferences for the next launch. Existing configuration files stay in their current directory.

| File or directory | Purpose |
|---|---|
| `settings.json` | Preferences and built-in module switches. |
| `macros.json` | Recorded macro actions. |
| `shortcuts.xml` | Shortcut overrides and macro bindings matched by name. |
| `file-access.json` | Sandbox file and directory access bookmarks. |
| `config-directory.json` | Configuration directory selection, stored in the default directory. |
| `converter.ini` | Converter formatting options. |
| `backup/` | Automatic session metadata and independent document backup files. |

Close the application before editing configuration files manually. See [recovery behavior](DESIGN.md#4-separate-document-backups-from-session-metadata).

## Development

Report issues in the [project repository](https://github.com/skyfiredao/notepad-matrix) with the macOS version, chip architecture, build revision, and steps to reproduce.

## License

Notepad Matrix is licensed under [GNU GPL v3](LICENSE). [Third-party notices](THIRD_PARTY_NOTICES.md) group shared license terms and retain component copyright notices.
