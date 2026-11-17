# Fixed language catalog

`languages.json` is runtime-consumed data extracted from Notepad++ commit
`d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e`:
<https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e>.

The upstream copyright notice is retained in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md), with shared GPL version 3 terms
in [`LICENSE`](../LICENSE). The upstream notice identifies Don HO, copyright 2021.
Source-file SHA-256 digests, including the license, are embedded in the JSON.
The catalog is transformed source data, not a separately relicensed database.

## Inventory and identity

There are exactly **96 Language models** in `langs.model.xml` and **93 LexerType
definitions** in `stylers.model.xml`. All models, keyword classes, keyword text,
language attributes, lexer attributes, style attributes, and style keyword text
are retained. XML entity decoding and newline normalization follow XML parsing.
Source metadata is retained even when a language has no default extension.

The language name table has 98 entries. Its additional entries are `udf`
(`L_USER`, id 15), which is the separate user-defined-language mechanism, and
`ext` (`L_EXTERNAL`, id 97), the external-lexer boundary. Neither is a fixed
Language model. IDs are verified against the actual `LangType` declaration,
not assigned from XML order. `searchResult` is included but marked internal.

The three models without their own LexerType definition are:

| Model | Source behavior |
| --- | --- |
| `normal` | Null lexer; no independent lexer styles |
| `json5` | JSON styles and keyword sets, with JSON comments enabled |
| `jsp` | HTML styling with embedded JavaScript, PHP, and ASP components |

`javascript` and `javascript.js` remain separate source identities. The former
describes embedded-JavaScript styles; selecting it as a document language calls
the modern JavaScript setup. `fortran` and `fortran77`, `tex` and `latex`, `sql`
and `mssql`, and `ini` and `props` are likewise not collapsed into aliases.

`Language::Builtin(u16)` stores the source ID. Existing `normal` and `json`
keys remain unchanged; other fixed-language keys use `builtin:<source-id>`.
`LanguageDefinition::key()` returns the source model name for lexer dispatch,
not the persisted Language key.

## Source data versus effective setup

`keywords` retains the XML class-name map without reordering. `numbered_keywords`
contains the effective `SCI_SETKEYWORDS` slots after the source setters' masks,
remapping, keyword sharing, and style additions. The getter `keywords(index)`
reads those numbered sets. Empty or unset slots return an empty string.
Substyle classes remain in the raw keyword map and style `keywordClass`
attributes; they are not falsely inserted into numbered `SCI_SETKEYWORDS` slots.

Examples include C-family types in slot 1 and C++ Doxygen words in slot 2;
CSS slots 0, 1, 4, and 6; XML DOCTYPE words in slot 5; HTML's embedded VB words
in slot 2 and PHP words in slot 4; and Perl's style-supplied `carp croak` words.
Concatenated lists preserve the source setter's separator space. Directly
shared Doxygen lists are retained without introducing a separator.

`lexer` preserves the name-table lexer. `effective_lexer`, also returned by
`lexer()`, reflects document selection: PHP uses the shared `hypertext` setup,
despite its name-table `phpscript` entry. `styles` retains the model's source
style records, resolving the JSON5/JSP aliases above. `effective_style_sources`
and `components` describe shared or embedded style setup without erasing any
source style definitions.

`properties` contains source application property assignments, including shared
`setLexer` fold settings, conditional Go/JSON5 overrides, and SQL's source GUI
default for backslash escaping. Repeated assignments retain the last value,
including JavaScript's backquoted-string setting of `2`. HTML-family setup
includes embedded ASP's `asp.default.language=2`. Absent Lexilla properties are
not invented: an absent property means the lexer supplies its own default.

## Resolution and errors

Default extensions are checked in reverse model order, as in
`NppParameters::getLangFromExt`. In particular, `.tex` resolves to `tex`, while
`.sty` resolves to `latex`. Matching is ASCII case-insensitive. Source dotfile
extensions such as `.bashrc`, `.editorconfig`, and `.gitconfig` are recognized.
Frozen user-extension lists in LexerType attributes are empty.

The filename refinements from `Buffer.cpp` run only after normal/unknown
extension resolution: Makefile, GNUmakefile, CMakeLists.txt, SConstruct,
SConscript, wscript, Rakefile, Vagrantfile, crontab, PKGBUILD, and APKBUILD.
`resolve(path, Some(id))` preserves explicit selection, including explicit
normal text. Unknown explicit IDs return a caller-visible error.

`LanguageCatalog::from_json` validates the schema, source version, identity,
styles, references, and complete frozen payload. It accepts a semantically
identical JSON import, not a modified or partial replacement catalog. Corruption
and unsupported versions return `CatalogError`. No runtime XML parser,
network fetch, or external catalog dependency is used.

## Lexer result contract

`TokenSpan.range` is a half-open UTF-8 byte range; `style` is the source numeric
style ID. Fold line indices are zero-based. `FoldLevel` separates nesting level,
header, and whitespace flags. `FoldRange` identifies an inclusive start/end line
pair. `LexResult` groups spans, line levels, and fold ranges. These types define
the integration seam; the catalog itself does not implement lexical behavior.
Catalog/profile availability is not a claim that a language's lexer is complete.

## Fixed light/dark theme data

`languages.themes.json` retains the source light `PowerEditor/src/stylers.model.xml`
and dark `PowerEditor/installer/themes/DarkModeDefault.xml` at the same frozen
commit. Each has 93 LexerTypes, 1,774 lexer style records, and 53 global style
records. The original language catalog and its 96/93/422/1774 inventory remain
unchanged. Theme source paths and SHA-256 digests, including the shared upstream
license, are included in this separate schema-version-1 JSON. The copyright and
license in the shared legal files also cover these transformed theme data.

Every lexer/global style attribute and keyword text is retained. Global records
are a vector because multiple source global styles have style ID 0; they are
not collapsed by numeric ID. Global Default Style 32 is black on white with
Courier New 10 in the light source and `DCDCCC` on `3F3F3F` with Consolas 10 in
the dark source. These values come from the frozen files, not a generated
palette.

`LanguageCatalog::default_style(dark)` returns the selected source's global
style 32. `style_for_theme(profile_id, style_id, dark)` searches effective lexer
style sources in reverse application order, so the final source assignment
wins. This includes JSON5 using JSON, JSP/HTML/PHP/ASP using the merged HTML,
embedded JavaScript, PHP, and ASP sources, and the deprecated selectable
embedded-JavaScript identity using modern JavaScript styles. A request for
global default 32 resolves through the selected theme. Unknown profiles or
unassigned style IDs return `None`; dark lookup never falls back to light.
Returned attributes remain raw, allowing the renderer to apply inherited font
or appearance settings without erasing source distinctions.

`LanguageCatalog::themes().theme(dark)` exposes all raw theme lexer and global
records. `ThemeCatalog::from_json` rejects malformed, modified, incomplete, or
unsupported-version imports with `CatalogError`. It does not import arbitrary
user themes. No `LanguageProfile` fields or constructors are added for theme
data. The generator's `--check` path verifies each light/dark source record's
attributes and text independently, as well as the existing language audit.

## Reproduce and verify

From `notepad-matrix` with a local checkout containing the frozen commit:

```sh
python3 scripts/language_catalog.py --source /path/to/notepad-plus-plus
python3 scripts/language_catalog.py --source /path/to/notepad-plus-plus --check
```

Generation reads committed blobs with `git show`, not mutable working-tree
files. Verification separately compares every source model and LexerType
attribute and keyword/style text against the generated JSON, checks the
LangType ordering, and checks the shared GPL terms and upstream copyright notice.

## Preinstalled User Defined Languages

`udls.json` contains both Markdown definitions from the same frozen revision,
`PowerEditor/bin/userDefineLangs/markdown._preinstalled.udl.xml` and
`markdown._preinstalled_DM.udl.xml`. It preserves all keyword lists, comments,
delimiter alternatives, nesting masks, styles, extensions and theme flags.

These definitions are from Edditoria's Markdown-plus-plus under the MIT license;
the upstream notice and shared MIT terms are in `THIRD_PARTY_NOTICES.md` and at
https://github.com/Edditoria/markdown-plus-plus/blob/master/LICENSE.txt.

Fresh registries include the two profiles. Existing registries, edits and
deletions remain authoritative. Extension association prefers the current
light/dark mode and falls back to the last matching definition, following
`Parameters.cpp::getUserDefinedLangNameFromExt`.

```sh
python3 scripts/udl_catalog.py /path/to/notepad-plus-plus
python3 scripts/udl_catalog.py /path/to/notepad-plus-plus --check
```

## Character panel data

`src/character_codes.rs` is generated from the same frozen revision's
`PowerEditor/src/WinControls/AnsiCharPanel/asciiListView.cpp`, preserving the
control labels, HTML entity names and special decimal mappings. Run
`python3 scripts/character_catalog.py /path/to/notepad-plus-plus --check`
to verify it. The source GPL notice is preserved in `THIRD_PARTY_NOTICES.md`.
The original table implementation is Copyright (C)2021 Don HO
<don.h@free.fr>, under GPL-3.0-or-later.

The approved platform adaptation is explicit encoding selection with UTF-8
as the character panel default, never an implicit Windows ACP. This does not
change file encoding defaults. HTML entity columns follow the source's
Windows-1252 applicability rule; the selected byte is decoded by encoding_rs.
The decoder dependency is encoding_rs 0.8.44 by Henri Sivonen,
https://github.com/hsivonen/encoding_rs, licensed
`(Apache-2.0 OR MIT) AND BSD-3-Clause`.
