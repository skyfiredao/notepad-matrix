# Notepad++ toolbar artwork

The Standard PNG resources are converted from the original BMP resources in
Notepad++ v8.9.9, commit `d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e`:

`PowerEditor/src/icons/standard/toolbar/*.bmp`

This is the traditional colored set visible in the user's reference image, **not**
the newer regular ICO set. The command mapping is upstream
`PowerEditor/src/Notepad_plus.rc:300–307,319–323`. Original size is 16×16.

Runtime PNGs preserve the original RGB artwork and dimensions. Indexed BMPs map
the classic RGB(192,192,192) toolbar background to alpha zero; 32-bit BMPs preserve
their original alpha. This replaces Win32's bitmap transparency handling, not the
artwork. There is no redraw or resampling. The unused BMP copies are not retained;
their originals remain available at the fixed upstream commit.

The zoom, wrap, invisible-character, indent-guide, document-map, function-list,
folder-workspace, monitoring and macro PNGs
use the corresponding BMPs in that same upstream directory. Four-bit and
eight-bit palettes use the same classic-background alpha adaptation. These
runtime images do not require bundling another copy of their BMP inputs.

`icons/{light,dark}/toolbar/{regular,filled}/` retains upstream ICO resources for
the selectable toolbar sets, including disabled states. `icons/standard/tabbar/`
retains the alternative saved, modified and read-only document-state ICOs.

`saved.png` and `unsaved.png` preserve the original 16×16 PNG entries from
`PowerEditor/src/icons/standard/tabbar/{saved,unsaved}.ico` at the same commit.

Document-state icons follow `ScintillaComponent/DocTabView.cpp:190–205`:
Monitoring, system read-only, application read-only, modified, then saved.
`readonly{,-dark}.png`, `monitoring-tab{,-dark}.png`, and `unsaved-dark.png`
preserve the original 16×16 PNG entries selected by `Notepad_plus.rc:278–291`.
`readonly-system{,-dark}.png` losslessly converts the original 16×16 DIB32/DIB8
pixels and ICO AND mask to PNG. `saved-dark.png` converts the source transparent
13×13 DIB1 `icons/dark/tabbar/empty.ico`; transparency remains unchanged.
Only containers change, without resampling or recoloring; the same license applies.

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

Copyright and license: Notepad++ contributors, including Don HO; see the retained
upstream notice in [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) and the
shared GNU GPL version 3 text in [LICENSE](../../LICENSE).
This provenance notice does not replace those terms or authorize a release.

`bookmark14.png` adds only a PNG container to the original 14×14 RGBA bytes in
the `bookmark14` array from upstream `PowerEditor/src/rgba_icons.h`; no redraw,
recoloring or scaling. The unused raw RGBA copy is not retained. Upstream usage:
`ScintillaEditView.cpp:425–430`.

Application branding is supplied by `../notepad-matrix.icns`.

The Rust UDL implementation also derives behavior from the fixed checkout's
`lexilla/lexers/LexUser.cxx`, copyright (C) 2003 Don HO, GPL version 2 or later.
The bundled GPL version 3 text is a permitted later version. No original lexer
library or DLL is linked.

The URI scanner in `src/links.rs` is adapted from Notepad++, copyright (C) 2021
Don HO <don.h@free.fr>, GPL version 3 or later, distributed without any warranty.
The copyright notice and complete license are retained in the shared legal files above.
