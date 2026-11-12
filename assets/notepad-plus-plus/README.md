# Notepad++ toolbar artwork

The original Standard BMP resources are copied **without modification** from
Notepad++ v8.9.9, commit `d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e`:

`PowerEditor/src/icons/standard/toolbar/*.bmp`

This is the traditional colored set visible in the user's reference image, **not**
the newer regular ICO set. The command mapping is upstream
`PowerEditor/src/Notepad_plus.rc:300–307,319–323`. Original size is 16×16.

Runtime PNGs preserve the original RGB artwork and dimensions. Indexed BMPs map
the classic RGB(192,192,192) toolbar background to alpha zero; 32-bit BMPs preserve
their original alpha. This replaces Win32's bitmap transparency handling, not the
artwork. The external Spec tool `standard_toolbar_assets.py` reproducibly generates
and verifies the PNGs against the fixed source. There is no redraw or resampling.

The zoom, wrap, invisible-character, indent-guide, document-map, function-list,
folder-workspace, monitoring and macro PNGs
use the corresponding BMPs in that same upstream directory. Four-bit and
eight-bit palettes use the same classic-background alpha adaptation. These
additional runtime images do not require bundling another copy of their BMP inputs.

`saved.png` and `unsaved.png` preserve the original 16×16 PNG entries from
`PowerEditor/src/icons/standard/tabbar/{saved,unsaved}.ico` at the same commit.

`tab-pin.png` and `tab-pinned.png` preserve the 16×16 RGBA DIB entries from
`PowerEditor/src/icons/standard/tabbar/pinTabButton{,_pinned}.ico` without
resampling or recoloring; only the image container is converted to PNG.
Their `-dark.png` counterparts are the unchanged 16×16 PNG entries from the
corresponding `PowerEditor/src/icons/dark/tabbar/` ICOs. State selection follows
`WinControls/TabBar/TabBar.cpp`, not font-dependent placeholder characters.

`tab-close{,-hover,-on-tab,-pressed}.png` preserve the original 11×11 RGBA
entries from `icons/standard/tabbar/closeTabButton{,_hoverIn,_hoverOnTab,_push}.ico`.
Their `-dark.png` counterparts preserve the original 16×16 entries in
`icons/dark/tabbar/`. Only the container changes. Light-mode pin artwork remains
the original 16×16 entry and is displayed at 11×11, matching the source's
`g_TabPinBtnSize` request; dark-mode buttons remain 16×16.

The Standard set is shared across themes, as in the upstream TB_STANDARD path
(`ToolBar.cpp:531–551`). Disabled buttons currently use reduced opacity; exact
Win32-generated disabled-state and DPI/pixel alignment remain unverified.

Source: <https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e/PowerEditor/src/icons>

Copyright and license: Notepad++ contributors, including Don HO; see the unmodified
upstream `LICENSE` in this directory (GNU GPL version 3 and upstream terms).
This provenance notice does not replace those terms or authorize a release.

`bookmark14.rgba` is the unchanged `bookmark14` array from upstream
`PowerEditor/src/rgba_icons.h`. `bookmark14.png` adds only a PNG container to those
original 14×14 RGBA bytes; no redraw, recoloring or scaling. Upstream usage:
`ScintillaEditView.cpp:425–430`.

`npp.icns` uses the original 256×256 PNG entry from
`PowerEditor/src/icons/npp.ico` at the same fixed commit. The PNG bytes are unchanged;
only the macOS ICNS container (`ic08`) is added. Source ICO SHA-256:
`7f00c181277bddfe06f51e7712c713e24e29a167b20033c43829136cdadc655d`.
It supplies the application, Finder and Dock icon without redesigning the artwork.

`app16.png` preserves the 16×16, 32-bit RGBA image from the same original ICO
for the custom main-window title. Only its DIB container is converted to PNG;
there is no resampling or recoloring.

The Rust UDL implementation also derives behavior from the fixed checkout's
`lexilla/lexers/LexUser.cxx`, copyright (C) 2003 Don HO, GPL version 2 or later.
The bundled GPL version 3 text is a permitted later version. No original lexer
library or DLL is linked.

The URI scanner in `src/links.rs` is adapted from Notepad++, copyright (C) 2021
Don HO <don.h@free.fr>, GPL version 3 or later, distributed without any warranty.
The complete license is retained in `LICENSE` in this directory.
