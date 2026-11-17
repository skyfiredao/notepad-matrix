# Notepad Matrix

[中文](README_zh.md) · [Build guide](BUILD.md) · [Design decisions](DESIGN.md)

A free text editor for macOS, built with Rust and GPUI. Notepad Matrix reimplements familiar Notepad++ workflows without running the Windows application or its editor libraries. It is an independent project, not an official Notepad++ release.

Project: [github.com/skyfiredao/notepad-matrix](https://github.com/skyfiredao/notepad-matrix)

## Quick start

On an Apple Silicon Mac running macOS 26, install the prerequisites in [BUILD.md](BUILD.md), then run these commands from the directory containing `Cargo.toml`:

```sh
export MACOSX_DEPLOYMENT_TARGET=26.0
cargo build --locked --release --target aarch64-apple-darwin
./target/aarch64-apple-darwin/release/notepad-matrix ./notes.txt
```

To create a Finder-launchable application and disk image, follow the [packaging instructions](BUILD.md#4-package-the-application).

For downloaded or ad-hoc-signed builds, see [macOS launch approval](BUILD.md#5-allow-a-trusted-application-to-run). Download only through the official project or a verifiable trusted publisher, and check published hashes before allowing an application to run.

## Why

The project started from a need for a free, capable editor on macOS with familiar menus and editing habits. The goal is to preserve useful workflows, not the original platform dependencies.

## Features

- Tabbed documents, two editor views, multiple selections, rectangular selections, undo and redo.
- Find and replace, regular expressions, search in files, and result navigation.
- Syntax highlighting, folding, user-defined languages, completion, call tips, and a function list.
- UTF-8 by default, UTF-16 BOM formats, and explicit Windows, OEM, and other character sets.
- Session recovery with periodic document backups stored separately from JSON metadata.
- Document list and document map panels, with docking and floating windows.
- Macros, configurable shortcuts, saved Run commands, and document variables.
- MD5, SHA-1, SHA-256, SHA-512, Converter, MIME Tools, and HTML file export.
- Read-only Markdown preview, including Mermaid diagrams.
- macOS file dialogs, printing, Finder integration, and native input handling.

The Plugins menu contains built-in modules. It is not a DLL loader or plugin marketplace. Windows ACP defaults, the Windows updater, and Intel/Universal packaging are outside the current implementation.

## Command-line usage

```sh
./target/aarch64-apple-darwin/release/notepad-matrix -multiInst -nosession ./notes.txt
./target/aarch64-apple-darwin/release/notepad-matrix -lrust -n20 -c5 ./src/main.rs
./target/aarch64-apple-darwin/release/notepad-matrix -- ./-example.txt
```

| Option | Purpose |
|---|---|
| `-multiInst` | Start an independent instance. |
| `-nosession` | Do not restore or save the automatic session for this launch. |
| `-openSession session.json` | Open one saved session file. |
| `-l<language>` | Select a built-in language, such as `rust`. |
| `-udl=<name>` | Select an installed user-defined language. Quote names containing spaces. |
| `-n<line>` / `-c<column>` | Set the initial position, starting at 1. |
| `--` | Treat all following arguments as file paths. |

Unsupported options return an error. Command-line compatibility does not cover every Notepad++ option.

## Configuration

Settings and user data live in `~/.notepad-matrix/`. Use the application's settings and shortcut dialogs for routine changes.

| File or directory | Purpose |
|---|---|
| `settings.json` | Preferences and built-in module switches. |
| `macros.json` | Recorded macro actions. |
| `shortcuts.xml` | Shortcut overrides, including macro bindings matched by name, and saved Run commands. |
| `converter.ini` | Converter formatting options. |
| `backup/` | Automatic session metadata and independent document backup files. |

Close the application before editing configuration files manually. A recovery backup is not a formal save and does not replace a separate backup of important files. Run commands can start external programs; review commands before running them.

## Development

[BUILD.md](BUILD.md) covers the macOS toolchain, local execution, packaging, and build failures. [DESIGN.md](DESIGN.md) describes the byte-oriented editor model, platform boundaries, recovery format, and resource trade-offs.

For changes and bug reports, use the [project repository](https://github.com/skyfiredao/notepad-matrix). Include the macOS version, chip architecture, build revision, and steps to reproduce. Do not attach private documents or configuration secrets.

## License

Notepad Matrix is licensed under [GNU GPL v3.0](LICENSE). Bundled resources and dependencies retain their own license notices. The packaging script includes dependency notices and license files in the application bundle.
