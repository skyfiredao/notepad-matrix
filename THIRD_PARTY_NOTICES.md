# Licenses and third-party notices

Notepad Matrix is distributed under GNU GPL version 3, without warranty. You may redistribute and modify it under those terms. The complete GPL text is in [LICENSE](LICENSE).

Shared license terms appear once below. Copyright and additional notices are grouped separately and apply to the components that reference them. Alternative licenses remain available as declared; different legal wording is retained rather than treated as identical.

Corresponding source and build scripts: https://github.com/skyfiredao/notepad-matrix. Use the tag or revision matching the binary release. Third-party source is linked below. Modified source files and resource provenance are retained in the repository, not copied into the application bundle.

## Bundled resources and adaptations

| Component | Source | Shared terms | Copyright and additional notices |
|---|---|---|---|
| Boost.Regex Rust adaptations | [Source](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e) | [bsl-1.0](#bsl-1.0) | [notice-20](#notice-20) |
| Language and theme data | [Source](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e) | [gpl-3.0](#gpl-3.0) | [notice-16](#notice-16) |
| Markdown parser | [Source](https://github.com/pulldown-cmark/pulldown-cmark/tree/v0.13.6) | [mit](#mit) | [notice-150](#notice-150) |
| Markdown user-defined languages | [Source](https://github.com/Edditoria/markdown-plus-plus) | [mit](#mit) | [notice-24](#notice-24) |
| Mermaid 12.3.0 | [Source](https://github.com/mermaid-js/mermaid/tree/mermaid%4012.3.0) | [mit](#mit) | [notice-29](#notice-29) |
| Mermaid bundled dependencies | [Source](https://github.com/mermaid-js/mermaid/tree/mermaid%4012.3.0) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-195](#notice-195) |
| Notepad++ data and Rust adaptations | [Source](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e) | [gpl-3.0](#gpl-3.0) | [notice-16](#notice-16), [notice-186](#notice-186) |
| OEM encoding mapping data | [Source](https://github.com/unicode-org/icu-data/tree/6eac2f4d349d9f4a0825103bb41845d8e5ab7add) | [unicode-3.0](#unicode-3.0) | [notice-17](#notice-17) |
| Scintilla and Lexilla Rust adaptations | [Source](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e) | [scintilla](#scintilla) | [notice-162](#notice-162), [notice-184](#notice-184) |
| Unicode conversion Rust adaptations | [Source](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e) | — | [notice-18](#notice-18) |

## Rust release dependencies

This inventory follows the locked macOS ARM64 normal dependency graph with `native-ui` enabled. Build-only and development dependencies are not shipped. The package script rejects an inventory that does not match the selected dependency names, versions and declared licenses.

| Crate | Version | Declared license | Source | Shared terms | Copyright and additional notices |
|---|---|---|---|---|---|
| `accesskit` | `0.24.1` | `MIT OR Apache-2.0` | [Source](https://github.com/AccessKit/accesskit) | [mit](#mit) | — |
| `accesskit_consumer` | `0.38.0` | `MIT OR Apache-2.0` | [Source](https://github.com/AccessKit/accesskit) | [mit](#mit) | — |
| `accesskit_macos` | `0.26.3` | `MIT OR Apache-2.0` | [Source](https://github.com/AccessKit/accesskit) | [mit](#mit) | — |
| `addr2line` | `0.27.1` | `Apache-2.0 OR MIT` | [Source](https://github.com/gimli-rs/addr2line) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-65](#notice-65) |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | [Source](https://github.com/oyvindln/adler2) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-19](#notice-19) |
| `aho-corasick` | `1.1.5` | `Unlicense OR MIT` | [Source](https://github.com/BurntSushi/aho-corasick) | [mit](#mit) | [notice-188](#notice-188), [notice-42](#notice-42) |
| `aligned` | `0.4.3` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-embedded-community/aligned) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-70](#notice-70) |
| `aligned-vec` | `0.6.4` | `MIT` | [Source](https://github.com/sarah-ek/aligned-vec/) | [mit](#mit) | [notice-126](#notice-126) |
| `allocator-api2` | `0.2.21` | `MIT OR Apache-2.0` | [Source](https://github.com/zakarumych/allocator-api2) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `anyhow` | `1.0.104` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/anyhow) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `arg_enum_proc_macro` | `0.3.4` | `MIT` | [Source](https://github.com/lu-zero/arg_enum_proc_macro) | [mit](#mit) | [notice-77](#notice-77) |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | [Source](https://github.com/droundy/arrayref) | — | [notice-44](#notice-44) |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | [Source](https://github.com/bluss/arrayvec) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-142](#notice-142) |
| `as-slice` | `0.2.1` | `MIT OR Apache-2.0` | [Source](https://github.com/japaric/as-slice) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-76](#notice-76) |
| `async-channel` | `2.5.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/async-channel) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `async-task` | `4.7.1` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/async-task) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `av-scenechange` | `0.14.1` | `MIT` | [Source](https://github.com/rust-av/av-scenechange) | [mit](#mit) | [notice-96](#notice-96) |
| `av1-grain` | `0.2.5` | `BSD-2-Clause` | [Source](https://github.com/rust-av/av1-grain) | — | [notice-12](#notice-12) |
| `avif-serialize` | `0.8.9` | `BSD-3-Clause` | [Source](https://github.com/kornelski/avif-serialize) | — | [notice-14](#notice-14) |
| `backtrace` | `0.3.77` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/backtrace-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `base64` | `0.22.1` | `MIT OR Apache-2.0` | [Source](https://github.com/marshallpierce/rust-base64) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-41](#notice-41) |
| `bit-set` | `0.8.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/contain-rs/bit-set) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-128](#notice-128) |
| `bit-vec` | `0.8.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/contain-rs/bit-vec) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-128](#notice-128) |
| `bit_field` | `0.10.3` | `Apache-2.0/MIT` | [Source](https://github.com/phil-opp/rust-bit-field) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-59](#notice-59) |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | [Source](https://github.com/bitflags/bitflags) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | [Source](https://github.com/bitflags/bitflags) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `bitstream-io` | `4.10.0` | `MIT/Apache-2.0` | [Source](https://github.com/tuffy/bitstream-io) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-69](#notice-69) |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/utils) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-85](#notice-85) |
| `block2` | `0.5.1` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-179](#notice-179) |
| `block2` | `0.6.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-178](#notice-178) |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | [Source](https://github.com/fitzgen/bumpalo) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-97](#notice-97) |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/Lokathor/bytemuck) | [apache-2.0](#apache-2.0), [zlib](#zlib) | [notice-90](#notice-90), [notice-91](#notice-91) |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/Lokathor/bytemuck) | [apache-2.0](#apache-2.0), [zlib](#zlib) | [notice-90](#notice-90), [notice-91](#notice-91) |
| `byteorder` | `1.5.0` | `Unlicense OR MIT` | [Source](https://github.com/BurntSushi/byteorder) | [mit](#mit) | [notice-188](#notice-188), [notice-42](#notice-42) |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | [Source](https://github.com/image-rs/byteorder-lite) | [mit](#mit) | [notice-42](#notice-42) |
| `bytes` | `1.12.1` | `MIT` | [Source](https://github.com/tokio-rs/bytes) | [mit](#mit) | [notice-75](#notice-75) |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/cfg-if) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `chrono` | `0.4.45` | `MIT OR Apache-2.0` | [Source](https://github.com/chronotope/chrono) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-187](#notice-187) |
| `color_quant` | `1.1.0` | `MIT` | [Source](https://github.com/image-rs/color_quant.git) | [mit](#mit) | [notice-60](#notice-60) |
| `concurrent-queue` | `2.5.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/concurrent-queue) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `convert_case` | `0.10.0` | `MIT` | [Source](https://github.com/rutrum/convert-case) | [mit](#mit) | [notice-133](#notice-133) |
| `core-foundation` | `0.10.1` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/core-foundation-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `core-foundation-sys` | `0.8.7` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/core-foundation-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `core-graphics` | `0.24.0` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/core-foundation-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `core-graphics-types` | `0.2.0` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/core-foundation-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `core-text` | `21.0.0` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/core-foundation-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `core_maths` | `0.1.1` | `MIT` | [Source](https://github.com/robertbastian/core_maths) | [mit](#mit) | [notice-132](#notice-132) |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/utils) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-118](#notice-118) |
| `crc32fast` | `1.5.4` | `MIT OR Apache-2.0` | [Source](https://github.com/srijs/rust-crc32fast) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-81](#notice-81) |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | [Source](https://github.com/crossbeam-rs/crossbeam) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-104](#notice-104) |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | [Source](https://github.com/crossbeam-rs/crossbeam) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-104](#notice-104) |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | [Source](https://github.com/crossbeam-rs/crossbeam) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-104](#notice-104) |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/traits) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-121](#notice-121) |
| `ctor` | `1.0.13` | `Apache-2.0 OR MIT` | [Source](https://github.com/mmastrac/linktime) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170) |
| `data-url` | `0.3.2` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-url) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-28](#notice-28) |
| `derive_more` | `2.1.1` | `MIT` | [Source](https://github.com/JelteF/derive_more) | [mit](#mit) | [notice-58](#notice-58) |
| `derive_more-impl` | `2.1.1` | `MIT` | [Source](https://github.com/JelteF/derive_more) | [mit](#mit) | [notice-58](#notice-58) |
| `digest` | `0.10.7` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/traits) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-68](#notice-68) |
| `dirs` | `5.0.1` | `MIT OR Apache-2.0` | [Source](https://github.com/soc/dirs-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-86](#notice-86) |
| `dirs-sys` | `0.4.1` | `MIT OR Apache-2.0` | [Source](https://github.com/dirs-dev/dirs-sys-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-86](#notice-86) |
| `dispatch2` | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-183](#notice-183) |
| `displaydoc` | `0.2.7` | `MIT OR Apache-2.0` | [Source](https://github.com/yaahc/displaydoc) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `dyn-clone` | `1.0.20` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/dyn-clone) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `either` | `1.19.0` | `MIT OR Apache-2.0` | [Source](https://github.com/rayon-rs/either) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-40](#notice-40) |
| `encoding_rs` | `0.8.44` | `(Apache-2.0 OR MIT) AND BSD-3-Clause` | [Source](https://github.com/hsivonen/encoding_rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-168](#notice-168), [notice-172](#notice-172) |
| `enumn` | `0.1.14` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/enumn) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `equator` | `0.4.2` | `MIT` | [Source](https://github.com/sarah-ek/equator/) | [mit](#mit) | [notice-130](#notice-130) |
| `equator-macro` | `0.4.2` | `MIT` | [Source](https://github.com/sarah-ek/equator/) | [mit](#mit) | [notice-130](#notice-130) |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | [Source](https://github.com/indexmap-rs/equivalent) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-63](#notice-63) |
| `errno` | `0.3.14` | `MIT OR Apache-2.0` | [Source](https://github.com/lambda-fairy/rust-errno) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-31](#notice-31) |
| `etagere` | `0.2.15` | `MIT/Apache-2.0` | [Source](https://github.com/nical/etagere) | [mit](#mit) | [notice-110](#notice-110), [notice-159](#notice-159) |
| `euclid` | `0.22.14` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/euclid) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-25](#notice-25) |
| `event-listener` | `5.4.2` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/event-listener) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `event-listener-strategy` | `0.5.4` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/event-listener-strategy) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `exr` | `1.74.2` | `BSD-3-Clause` | [Source](https://github.com/johannesvollmer/exrs) | — | [notice-2](#notice-2) |
| `fancy-regex` | `0.20.1` | `MIT` | [Source](https://github.com/fancy-regex/fancy-regex) | [mit](#mit) | [notice-151](#notice-151), [notice-3](#notice-3) |
| `fastrand` | `2.5.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/fastrand) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `fax` | `0.2.7` | `MIT` | [Source](https://github.com/pdf-rs/fax) | [mit](#mit) | [notice-171](#notice-171) |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/fdeflate) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `flate2` | `1.1.10` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/flate2-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-38](#notice-38) |
| `float-cmp` | `0.9.0` | `MIT` | [Source](https://github.com/mikedilger/float-cmp) | [mit](#mit) | [notice-37](#notice-37) |
| `float-ord` | `0.3.2` | `MIT / Apache-2.0` | [Source](https://github.com/notriddle/rust-float-ord) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-50](#notice-50) |
| `float_next_after` | `1.0.0` | `MIT` | [Source](https://gitlab.com/bronsonbdevost/next_afterf) | [mit](#mit) | [notice-113](#notice-113) |
| `flume` | `0.12.0` | `Apache-2.0/MIT` | [Source](https://github.com/zesterer/flume) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `foldhash` | `0.2.0` | `Zlib` | [Source](https://github.com/orlp/foldhash) | [zlib](#zlib) | [notice-131](#notice-131) |
| `fontdb` | `0.23.0` | `MIT` | [Source](https://github.com/RazrFalcon/fontdb) | [mit](#mit) | [notice-114](#notice-114), [notice-194](#notice-194) |
| `foreign-types` | `0.5.0` | `MIT/Apache-2.0` | [Source](https://github.com/sfackler/foreign-types) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-73](#notice-73) |
| `foreign-types-macros` | `0.2.4` | `MIT/Apache-2.0` | [Source](https://github.com/sfackler/foreign-types) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-73](#notice-73) |
| `foreign-types-shared` | `0.3.1` | `MIT/Apache-2.0` | [Source](https://github.com/sfackler/foreign-types) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-73](#notice-73) |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-url) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-27](#notice-27) |
| `futures` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-channel` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-core` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-executor` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-io` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-macro` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-sink` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-task` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `futures-util` | `0.3.34` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/futures-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-54](#notice-54), [notice-55](#notice-55) |
| `generic-array` | `0.14.7` | `MIT` | [Source](https://github.com/fizyk20/generic-array.git) | [mit](#mit) | [notice-43](#notice-43) |
| `getrandom` | `0.3.4` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-random/getrandom) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-87](#notice-87) |
| `getrandom` | `0.4.3` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-random/getrandom) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-88](#notice-88) |
| `gif` | `0.14.2` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/image-gif) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-51](#notice-51) |
| `gimli` | `0.34.0` | `MIT OR Apache-2.0` | [Source](https://github.com/gimli-rs/gimli) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-48](#notice-48) |
| `gpui-pre` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-apple` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-collections` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-derive-refineable` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-http-client` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-macos` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-macros` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-platform` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-refineable` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-scheduler` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-shared-string` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-sum-tree` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-util` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-ztracing` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `gpui-pre-ztracing-macro` | `0.3.11` | `Apache-2.0` | [Source](https://github.com/zed-industries/zed) | [apache-2.0](#apache-2.0) | [notice-161](#notice-161) |
| `half` | `2.7.1` | `MIT OR Apache-2.0` | [Source](https://github.com/VoidStarKat/half-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `hash32` | `0.3.1` | `MIT OR Apache-2.0` | [Source](https://github.com/japaric/hash32) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-76](#notice-76) |
| `hashbrown` | `0.16.1` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/hashbrown) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-56](#notice-56) |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/hashbrown) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-56](#notice-56) |
| `heapless` | `0.9.3` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-embedded/heapless) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-70](#notice-70) |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | [Source](https://github.com/withoutboats/heck) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-48](#notice-48) |
| `http` | `1.5.0` | `MIT OR Apache-2.0` | [Source](https://github.com/hyperium/http) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-153](#notice-153), [notice-74](#notice-74) |
| `http-body` | `1.1.0` | `MIT` | [Source](https://github.com/hyperium/http-body) | [mit](#mit) | [notice-108](#notice-108) |
| `iana-time-zone` | `0.1.65` | `MIT OR Apache-2.0` | [Source](https://github.com/strawlab/iana-time-zone) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-109](#notice-109), [notice-158](#notice-158) |
| `icu_collections` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_locale_core` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_locale_fallback` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_locale_fallback_data` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_normalizer` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_normalizer_data` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_properties` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_properties_data` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_provider` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_segmenter` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `icu_segmenter_data` | `2.4.0` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-url/) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-28](#notice-28) |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | [Source](https://github.com/hsivonen/idna_adapter) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-141](#notice-141) |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/image) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `image-webp` | `0.2.4` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/image-webp) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `imagesize` | `0.14.0` | `MIT` | [Source](https://github.com/Roughsketch/imagesize) | [mit](#mit) | [notice-71](#notice-71) |
| `imgref` | `1.12.3` | `CC0-1.0 OR Apache-2.0` | [Source](https://github.com/kornelski/imgref) | [apache-2.0](#apache-2.0) | [notice-173](#notice-173) |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | [Source](https://github.com/indexmap-rs/indexmap) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-62](#notice-62) |
| `inventory` | `0.3.25` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/inventory) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `itertools` | `0.14.0` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-itertools/itertools) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-40](#notice-40) |
| `itoa` | `1.0.18` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/itoa) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `kurbo` | `0.13.1` | `Apache-2.0 OR MIT` | [Source](https://github.com/linebender/kurbo) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-79](#notice-79) |
| `lazy_static` | `1.5.1` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang-nursery/lazy-static.rs) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `lebe` | `0.5.3` | `BSD-3-Clause` | [Source](https://github.com/johannesvollmer/lebe) | — | [notice-123](#notice-123) |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/libc) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-140](#notice-140) |
| `libm` | `0.2.16` | `MIT` | [Source](https://github.com/rust-lang/compiler-builtins) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-196](#notice-196) |
| `link-section` | `0.19.3` | `Apache-2.0 OR MIT` | [Source](https://github.com/mmastrac/linktime) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170) |
| `linktime-proc-macro` | `0.2.3` | `Apache-2.0 OR MIT` | [Source](https://github.com/mmastrac/linktime) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170) |
| `litemap` | `0.8.3` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `lock_api` | `0.4.14` | `MIT OR Apache-2.0` | [Source](https://github.com/Amanieu/parking_lot) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-61](#notice-61) |
| `log` | `0.4.35` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/log) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `loop9` | `0.1.5` | `MIT` | [Source](https://gitlab.com/kornelski/loop9.git) | [mit](#mit) | [notice-198](#notice-198) |
| `lyon` | `1.0.19` | `MIT OR Apache-2.0` | [Source](https://github.com/nical/lyon) | [mit](#mit) | [notice-26](#notice-26) |
| `lyon_algorithms` | `1.0.22` | `MIT OR Apache-2.0` | [Source](https://github.com/nical/lyon) | [mit](#mit) | [notice-26](#notice-26) |
| `lyon_geom` | `1.0.20` | `MIT OR Apache-2.0` | [Source](https://github.com/nical/lyon) | [mit](#mit) | [notice-26](#notice-26) |
| `lyon_path` | `1.0.19` | `MIT OR Apache-2.0` | [Source](https://github.com/nical/lyon) | [mit](#mit) | [notice-26](#notice-26) |
| `lyon_tessellation` | `1.0.23` | `MIT OR Apache-2.0` | [Source](https://github.com/nical/lyon) | [mit](#mit) | [notice-26](#notice-26) |
| `mach2` | `0.6.0` | `BSD-2-Clause OR MIT OR Apache-2.0` | [Source](https://github.com/JohnTitor/mach2) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-98](#notice-98), [notice-99](#notice-99) |
| `maybe-rayon` | `0.1.1` | `MIT` | [Source](https://github.com/shssoichiro/maybe-rayon) | [mit](#mit) | [notice-119](#notice-119) |
| `md-5` | `0.10.6` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/hashes) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-21](#notice-21) |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | [Source](https://github.com/BurntSushi/memchr) | [mit](#mit) | [notice-188](#notice-188), [notice-42](#notice-42) |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | [Source](https://github.com/RazrFalcon/memmap2-rs) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-115](#notice-115), [notice-169](#notice-169) |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | [Source](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-146](#notice-146), [notice-147](#notice-147) |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | [Source](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-146](#notice-146), [notice-147](#notice-147) |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | [Source](https://github.com/awxkee/moxcms.git) | [apache-2.0](#apache-2.0) | [notice-138](#notice-138), [notice-166](#notice-166) |
| `multiversion_no_op` | `1.0.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/hsivonen/multiversion_no_op) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-168](#notice-168) |
| `new_debug_unreachable` | `1.0.6` | `MIT` | [Source](https://github.com/mbrubeck/rust-debug-unreachable) | [mit](#mit) | [notice-45](#notice-45) |
| `no_std_io2` | `0.9.4` | `Apache-2.0 OR MIT` | [Source](https://github.com/wcampbell0x2a/no-std-io2) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-116](#notice-116), [notice-170](#notice-170) |
| `nom` | `8.0.0` | `MIT` | [Source](https://github.com/rust-bakery/nom) | [mit](#mit) | [notice-36](#notice-36) |
| `noop_proc_macro` | `0.3.0` | `MIT` | [Source](https://github.com/lu-zero/noop_proc_macro) | [mit](#mit) | [notice-94](#notice-94) |
| `num-bigint` | `0.4.8` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-bigint) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num-complex` | `0.4.6` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-complex) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num-derive` | `0.4.2` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-derive) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num-integer` | `0.1.47` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-integer) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num-rational` | `0.4.2` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-rational) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-num/num-traits) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `num_cpus` | `1.17.0` | `MIT OR Apache-2.0` | [Source](https://github.com/seanmonstar/num_cpus) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-53](#notice-53) |
| `objc-sys` | `0.3.5` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-178](#notice-178) |
| `objc2` | `0.5.3` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-179](#notice-179) |
| `objc2` | `0.6.5` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-178](#notice-178) |
| `objc2-app-kit` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-core-data` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-core-foundation` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-core-image` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-core-text` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-core-video` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-encode` | `4.1.0` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-178](#notice-178) |
| `objc2-foundation` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-foundation` | `0.3.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-176](#notice-176) |
| `objc2-io-surface` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-metal` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-metal` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-quartz-core` | `0.2.2` | `MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-139](#notice-139), [notice-176](#notice-176) |
| `objc2-quartz-core` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-security` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-uniform-type-identifiers` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-user-notifications` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `objc2-web-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/madsmtm/objc2) | [mit](#mit) | [notice-1](#notice-1), [notice-182](#notice-182) |
| `object` | `0.40.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/gimli-rs/object) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-47](#notice-47) |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | [Source](https://github.com/matklad/once_cell) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `option-ext` | `0.2.0` | `MPL-2.0` | [Source](https://github.com/soc/option-ext.git) | [mpl-2.0](#mpl-2.0) | — |
| `parking` | `2.2.1` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/parking) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-9](#notice-9) |
| `parking_lot` | `0.12.5` | `MIT OR Apache-2.0` | [Source](https://github.com/Amanieu/parking_lot) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-61](#notice-61) |
| `parking_lot_core` | `0.9.12` | `MIT OR Apache-2.0` | [Source](https://github.com/Amanieu/parking_lot) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-61](#notice-61) |
| `paste` | `1.0.15` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/paste) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `pastey` | `0.1.1` | `MIT OR Apache-2.0` | [Source](https://github.com/as1100k/pastey) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `pathfinder_geometry` | `0.5.1` | `MIT/Apache-2.0` | [Source](https://github.com/servo/pathfinder) | [mit](#mit) | [notice-181](#notice-181) |
| `pathfinder_simd` | `0.5.6` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/pathfinder) | [mit](#mit) | [notice-22](#notice-22) |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-url/) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-28](#notice-28) |
| `pico-args` | `0.5.0` | `MIT` | [Source](https://github.com/RazrFalcon/pico-args) | [mit](#mit) | [notice-107](#notice-107) |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | [Source](https://github.com/taiki-e/pin-project) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | [Source](https://github.com/taiki-e/pin-project) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | [Source](https://github.com/taiki-e/pin-project-lite) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `png` | `0.17.16` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/image-png) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-51](#notice-51) |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/image-png) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-51](#notice-51) |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | [Source](https://github.com/zesterer/pollster) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-117](#notice-117) |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | [Source](https://github.com/linebender/kurbo) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-79](#notice-79) |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `ppv-lite86` | `0.2.21` | `MIT OR Apache-2.0` | [Source](https://github.com/cryptocorrosion/cryptocorrosion) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-105](#notice-105), [notice-156](#notice-156) |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | [Source](https://github.com/bkchr/proc-macro-crate) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/proc-macro2) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | [Source](https://github.com/aclysma/profiling) | [mit](#mit) | [notice-112](#notice-112) |
| `profiling-procmacros` | `1.0.18` | `MIT OR Apache-2.0` | [Source](https://github.com/aclysma/profiling) | [mit](#mit) | [notice-112](#notice-112) |
| `pulldown-cmark` | `0.13.6` | `MIT` | [Source](https://github.com/raphlinus/pulldown-cmark) | [mit](#mit) | [notice-149](#notice-149) |
| `pulp` | `0.22.3` | `MIT` | [Source](https://github.com/sarah-quinones/pulp/) | [mit](#mit) | [notice-122](#notice-122) |
| `pulp-wasm-simd-flag` | `0.1.1` | `MIT` | [Source](https://github.com/sarah-quinones/pulp/) | [mit](#mit) | [notice-122](#notice-122) |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | [Source](https://github.com/awxkee/pxfm) | [apache-2.0](#apache-2.0) | [notice-138](#notice-138), [notice-166](#notice-166) |
| `qoi` | `0.4.1` | `MIT/Apache-2.0` | [Source](https://github.com/aldanor/qoi-rust) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-124](#notice-124) |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | [Source](http://github.com/tailhook/quick-error) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-170](#notice-170), [notice-49](#notice-49) |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/quote) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `rand` | `0.9.5` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-random/rand) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-155](#notice-155) |
| `rand_chacha` | `0.9.0` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-random/rand) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-155](#notice-155) |
| `rand_core` | `0.9.5` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-random/rand) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-155](#notice-155) |
| `rav1e` | `0.8.1` | `BSD-2-Clause` | [Source](https://github.com/xiph/rav1e/) | — | [notice-11](#notice-11) |
| `ravif` | `0.13.0` | `BSD-3-Clause` | [Source](https://github.com/kornelski/cavif-rs) | — | [notice-15](#notice-15) |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | [Source](https://github.com/rust-windowing/raw-window-handle) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-101](#notice-101), [notice-111](#notice-111) |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | [Source](https://github.com/rayon-rs/rayon) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-22](#notice-22) |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | [Source](https://github.com/rayon-rs/rayon) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-22](#notice-22) |
| `reborrow` | `0.5.5` | `MIT` | [Source](https://github.com/sarah-ek/reborrow/) | [mit](#mit) | [notice-126](#notice-126) |
| `ref-cast` | `1.0.27` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/ref-cast) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `ref-cast-impl` | `1.0.27` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/ref-cast) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/regex) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/regex) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-34](#notice-34) |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/regex) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-193](#notice-193), [notice-34](#notice-34) |
| `resvg` | `0.46.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/linebender/resvg) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-154](#notice-154) |
| `rgb` | `0.8.53` | `MIT` | [Source](https://github.com/kornelski/rust-rgb) | [mit](#mit) | [notice-93](#notice-93) |
| `ropey` | `2.0.0-beta.1` | `MIT OR Apache-2.0` | [Source](https://github.com/cessen/ropey) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-127](#notice-127) |
| `roxmltree` | `0.21.2` | `MIT OR Apache-2.0` | [Source](https://github.com/RazrFalcon/roxmltree) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-84](#notice-84) |
| `rustc-demangle` | `0.1.28` | `MIT/Apache-2.0` | [Source](https://github.com/rust-lang/rustc-demangle) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | [Source](https://github.com/rust-lang/rustc-hash) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `rustix` | `1.1.6` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [Source](https://github.com/bytecodealliance/rustix) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `rustybuzz` | `0.20.1` | `MIT` | [Source](https://github.com/harfbuzz/rustybuzz) | [mit](#mit) | [notice-134](#notice-134), [notice-137](#notice-137) |
| `ryu` | `1.0.23` | `Apache-2.0 OR BSL-1.0` | [Source](https://github.com/dtolnay/ryu) | [apache-2.0](#apache-2.0), [bsl-1.0](#bsl-1.0) | — |
| `same-file` | `1.0.6` | `Unlicense/MIT` | [Source](https://github.com/BurntSushi/same-file) | [mit](#mit) | [notice-188](#notice-188), [notice-67](#notice-67) |
| `schemars` | `1.2.3` | `MIT` | [Source](https://github.com/GREsau/schemars) | [mit](#mit) | [notice-92](#notice-92) |
| `schemars_derive` | `1.2.3` | `MIT` | [Source](https://github.com/GREsau/schemars) | [mit](#mit) | [notice-92](#notice-92) |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | [Source](https://github.com/bluss/scopeguard) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-66](#notice-66) |
| `seahash` | `4.1.0` | `MIT` | [Source](https://gitlab.redox-os.org/redox-os/seahash) | [mit](#mit) | [notice-180](#notice-180) |
| `semver` | `1.0.28` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/semver) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | [Source](https://github.com/serde-rs/serde) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | [Source](https://github.com/serde-rs/serde) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | [Source](https://github.com/serde-rs/serde) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde_derive_internals` | `0.30.0` | `MIT OR Apache-2.0` | [Source](https://github.com/serde-rs/serde) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | [Source](https://github.com/serde-rs/json) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `serde_urlencoded` | `0.7.1` | `MIT/Apache-2.0` | [Source](https://github.com/nox/serde_urlencoded) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-57](#notice-57) |
| `sha1_smol` | `1.0.1` | `BSD-3-Clause` | [Source](https://github.com/mitsuhiko/sha1-smol) | [mit](#mit) | [notice-13](#notice-13), [notice-7](#notice-7) |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` | [Source](https://github.com/RustCrypto/hashes) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-21](#notice-21) |
| `simd-adler32` | `0.3.10` | `MIT` | [Source](https://github.com/mcountryman/simd-adler32) | [mit](#mit) | [notice-143](#notice-143) |
| `simd_helpers` | `0.1.0` | `MIT` | [Source](https://github.com/lu-zero/simd_helpers) | [mit](#mit) | [notice-177](#notice-177) |
| `simdutf8` | `0.1.5` | `MIT OR Apache-2.0` | [Source](https://github.com/rusticstuff/simdutf8) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `simplecss` | `0.2.2` | `Apache-2.0 OR MIT` | [Source](https://github.com/linebender/simplecss) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-4](#notice-4), [notice-80](#notice-80) |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | [Source](https://github.com/jedisct1/rust-siphash) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-145](#notice-145) |
| `slab` | `0.4.12` | `MIT` | [Source](https://github.com/tokio-rs/slab) | [mit](#mit) | [notice-89](#notice-89) |
| `slotmap` | `1.1.1` | `Zlib` | [Source](https://github.com/orlp/slotmap) | [zlib](#zlib) | [notice-120](#notice-120) |
| `smallvec` | `1.16.4` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-smallvec) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-82](#notice-82) |
| `smol_str` | `0.3.7` | `MIT OR Apache-2.0` | [Source](https://github.com/rust-lang/rust-analyzer/tree/master/lib/smol_str) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `spin` | `0.10.1` | `MIT` | [Source](https://github.com/mvdnes/spin-rs.git) | [mit](#mit) | [notice-32](#notice-32) |
| `spin` | `0.9.9` | `MIT` | [Source](https://github.com/mvdnes/spin-rs.git) | [mit](#mit) | [notice-32](#notice-32) |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | [Source](https://github.com/storyyeller/stable_deref_trait) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-72](#notice-72) |
| `str_indices` | `0.4.4` | `MIT OR Apache-2.0` | [Source](https://github.com/cessen/str_indices) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `strict-num` | `0.1.1` | `MIT` | [Source](https://github.com/RazrFalcon/strict-num) | [mit](#mit) | [notice-125](#notice-125) |
| `strum` | `0.28.0` | `MIT` | [Source](https://github.com/Peternator7/strum) | [mit](#mit) | [notice-102](#notice-102) |
| `strum_macros` | `0.28.0` | `MIT` | [Source](https://github.com/Peternator7/strum) | [mit](#mit) | [notice-102](#notice-102) |
| `subtle` | `2.6.1` | `BSD-3-Clause` | [Source](https://github.com/dalek-cryptography/subtle) | — | [notice-64](#notice-64) |
| `svg_fmt` | `0.4.5` | `MIT/Apache-2.0` | [Source](https://github.com/nical/rust_debug) | [mit](#mit) | [notice-100](#notice-100) |
| `svgtypes` | `0.16.2` | `Apache-2.0 OR MIT` | [Source](https://github.com/linebender/svgtypes) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-5](#notice-5), [notice-84](#notice-84) |
| `syn` | `2.0.119` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/syn) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `syn` | `3.0.9` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/syn) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `synstructure` | `0.14.0` | `MIT` | [Source](https://github.com/mystor/synstructure) | [mit](#mit) | [notice-152](#notice-152) |
| `taffy` | `0.15.1` | `MIT` | [Source](https://github.com/DioxusLabs/taffy) | [mit](#mit) | [notice-83](#notice-83) |
| `tempfile` | `3.27.0` | `MIT OR Apache-2.0` | [Source](https://github.com/Stebalien/tempfile) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-46](#notice-46) |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/thiserror) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | [Source](https://github.com/dtolnay/thiserror) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `tiff` | `0.11.3` | `MIT` | [Source](https://github.com/image-rs/image-tiff) | [mit](#mit) | [notice-78](#notice-78) |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | [Source](https://github.com/RazrFalcon/tiny-skia) | — | [notice-23](#notice-23) |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | [Source](https://github.com/RazrFalcon/tiny-skia/tree/master/path) | — | [notice-23](#notice-23) |
| `tinystr` | `0.8.5` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `tinyvec` | `1.13.3` | `Zlib OR Apache-2.0 OR MIT` | [Source](https://github.com/Lokathor/tinyvec) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-90](#notice-90) |
| `toml_datetime` | `1.1.2+spec-1.1.0` | `MIT OR Apache-2.0` | [Source](https://github.com/toml-rs/toml) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-136](#notice-136), [notice-167](#notice-167) |
| `toml_edit` | `0.25.18+spec-1.1.0` | `MIT OR Apache-2.0` | [Source](https://github.com/toml-rs/toml) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-136](#notice-136), [notice-167](#notice-167) |
| `toml_parser` | `1.1.5+spec-1.1.0` | `MIT OR Apache-2.0` | [Source](https://github.com/toml-rs/toml) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-136](#notice-136), [notice-167](#notice-167) |
| `tracing` | `0.1.44` | `MIT` | [Source](https://github.com/tokio-rs/tracing) | [mit](#mit) | [notice-106](#notice-106) |
| `tracing-attributes` | `0.1.31` | `MIT` | [Source](https://github.com/tokio-rs/tracing) | [mit](#mit) | [notice-106](#notice-106) |
| `tracing-core` | `0.1.36` | `MIT` | [Source](https://github.com/tokio-rs/tracing) | [mit](#mit) | [notice-106](#notice-106), [notice-32](#notice-32) |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | [Source](https://github.com/harfbuzz/ttf-parser) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-84](#notice-84) |
| `typenum` | `1.20.1` | `MIT OR Apache-2.0` | [Source](https://github.com/paholg/typenum) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-148](#notice-148), [notice-185](#notice-185), [notice-33](#notice-33) |
| `unicase` | `2.10.0` | `MIT OR Apache-2.0` | [Source](https://github.com/seanmonstar/unicase) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-39](#notice-39) |
| `unicode-bidi` | `0.3.18` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/unicode-bidi) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-189](#notice-189), [notice-48](#notice-48) |
| `unicode-bidi-mirroring` | `0.4.0` | `MIT/Apache-2.0` | [Source](https://github.com/RazrFalcon/unicode-bidi-mirroring) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-114](#notice-114) |
| `unicode-ccc` | `0.4.0` | `MIT/Apache-2.0` | [Source](https://github.com/RazrFalcon/unicode-ccc) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-114](#notice-114) |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | [Source](https://github.com/dtolnay/unicode-ident) | [apache-2.0](#apache-2.0), [mit](#mit), [unicode-3.0](#unicode-3.0) | [notice-190](#notice-190) |
| `unicode-linebreak` | `0.1.5` | `Apache-2.0` | [Source](https://github.com/axelf4/unicode-linebreak) | [apache-2.0](#apache-2.0) | — |
| `unicode-properties` | `0.1.5` | `(MIT OR Apache-2.0) AND Unicode-3.0` | [Source](https://github.com/unicode-rs/unicode-properties) | [apache-2.0](#apache-2.0), [mit](#mit), [unicode-3.0](#unicode-3.0) | [notice-191](#notice-191), [notice-48](#notice-48) |
| `unicode-script` | `0.5.9` | `MIT OR Apache-2.0` | [Source](https://github.com/unicode-rs/unicode-script) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-160](#notice-160), [notice-95](#notice-95) |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | [Source](https://github.com/unicode-rs/unicode-segmentation) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-48](#notice-48) |
| `unicode-vo` | `0.1.0` | `MIT/Apache-2.0` | [Source](https://github.com/RazrFalcon/unicode-vo) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-80](#notice-80) |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/rust-url) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-28](#notice-28) |
| `usvg` | `0.46.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/linebender/resvg) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-154](#notice-154) |
| `utf8_iter` | `1.1.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/hsivonen/utf8_iter) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-168](#notice-168) |
| `utf_types` | `1.0.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/hsivonen/utf_types) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-168](#notice-168) |
| `uuid` | `1.29.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/uuid-rs/uuid) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-35](#notice-35) |
| `v_frame` | `0.3.9` | `BSD-2-Clause` | [Source](https://github.com/rust-av/v_frame) | — | [notice-10](#notice-10) |
| `waker-fn` | `1.2.0` | `Apache-2.0 OR MIT` | [Source](https://github.com/smol-rs/waker-fn) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `walkdir` | `2.5.0` | `Unlicense/MIT` | [Source](https://github.com/BurntSushi/walkdir) | [mit](#mit) | [notice-188](#notice-188), [notice-42](#notice-42) |
| `wasm-bindgen` | `0.2.130` | `MIT OR Apache-2.0` | [Source](https://github.com/wasm-bindgen/wasm-bindgen) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `wasm-bindgen-macro` | `0.2.130` | `MIT OR Apache-2.0` | [Source](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `wasm-bindgen-macro-support` | `0.2.130` | `MIT OR Apache-2.0` | [Source](https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `wasm-bindgen-shared` | `0.2.130` | `MIT OR Apache-2.0` | [Source](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-30](#notice-30) |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | [Source](https://github.com/daxpedda/web-time) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-129](#notice-129), [notice-165](#notice-165) |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | [Source](https://github.com/image-rs/weezl) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-135](#notice-135) |
| `winnow` | `1.0.5` | `MIT` | [Source](https://github.com/winnow-rs/winnow) | [mit](#mit) | — |
| `writeable` | `0.6.4` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `xmlwriter` | `0.1.0` | `MIT` | [Source](https://github.com/RazrFalcon/xmlwriter) | [mit](#mit) | [notice-103](#notice-103) |
| `y4m` | `0.8.0` | `MIT` | [Source](https://github.com/image-rs/y4m.git) | [mit](#mit) | [notice-52](#notice-52) |
| `yoke` | `0.8.4` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `yoke-derive` | `0.8.4` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zed-font-kit` | `0.14.1-zed` | `MIT OR Apache-2.0` | [Source](https://github.com/servo/font-kit) | [apache-2.0](#apache-2.0), [mit](#mit) | — |
| `zerocopy` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | [Source](https://github.com/google/zerocopy) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-157](#notice-157), [notice-163](#notice-163), [notice-164](#notice-164) |
| `zerocopy-derive` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | [Source](https://github.com/google/zerocopy) | [apache-2.0](#apache-2.0), [mit](#mit) | [notice-157](#notice-157), [notice-163](#notice-163), [notice-164](#notice-164) |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zerofrom-derive` | `0.1.8` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zerotrie` | `0.2.5` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zerovec` | `0.11.9` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zerovec-derive` | `0.11.7` | `Unicode-3.0` | [Source](https://github.com/unicode-org/icu4x) | [unicode-3.0](#unicode-3.0) | [notice-192](#notice-192) |
| `zlib-rs` | `0.6.8` | `Zlib` | [Source](https://github.com/trifectatechfoundation/zlib-rs) | [zlib](#zlib) | [notice-6](#notice-6) |
| `zmij` | `1.0.23` | `MIT` | [Source](https://github.com/dtolnay/zmij) | [mit](#mit) | — |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | [Source](https://github.com/etemesi254/zune-image) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-144](#notice-144), [notice-174](#notice-174), [notice-197](#notice-197) |
| `zune-inflate` | `0.2.54` | `MIT OR Apache-2.0 OR Zlib` | [Source](https://crates.io/crates/zune-inflate/0.2.54) | [mit](#mit) | [notice-174](#notice-174), [notice-8](#notice-8) |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | [Source](https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg) | [apache-2.0](#apache-2.0), [mit](#mit), [zlib](#zlib) | [notice-144](#notice-144), [notice-175](#notice-175), [notice-197](#notice-197) |

## Shared license terms

<a id="gpl-3.0"></a>

### GNU GPL version 3

See the complete, unchanged text in [LICENSE](LICENSE). The project uses GPL version 3. Upstream components retain their stated version choices.

<a id="mit"></a>

### MIT

```text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

<a id="apache-2.0"></a>

### Apache License 2.0

```text
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
    of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!) The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

<a id="unicode-3.0"></a>

### Unicode License v3

```text
Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```

<a id="bsl-1.0"></a>

### Boost Software License 1.0

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

<a id="mpl-2.0"></a>

### Mozilla Public License 2.0

```text
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in 
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
```

<a id="zlib"></a>

### zlib

```text
This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

<a id="scintilla"></a>

### Scintilla and Lexilla

```text
Permission to use, copy, modify, and distribute this software and its
documentation for any purpose and without fee is hereby granted,
provided that the above copyright notice appear in all copies and that
both that copyright notice and this permission notice appear in
supporting documentation.

NEIL HODGSON DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS, IN NO EVENT SHALL NEIL HODGSON BE LIABLE FOR ANY
SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE
OR PERFORMANCE OF THIS SOFTWARE.
```

## Copyright and additional notices

<a id="notice-1"></a>

### notice-1

```text
# License The licensing of these crates is a bit complicated: - The crates `objc2`, `block2`, `objc2-foundation` and `objc2-encode` are [currently][#23] licensed under [the MIT license][MIT]. - All other crates are trio-licensed under the [Zlib], [Apache-2.0] or [MIT] license, at your option. Furthermore, the crates are (usually automatically) derived from Apple SDKs, and that may have implications for licensing, see below for details. [#23]: https://github.com/madsmtm/objc2/issues/23 [MIT]: https://opensource.org/license/MIT [Zlib]: https://zlib.net/zlib_license.html [Apache-2.0]: https://www.apache.org/licenses/ ## Apple SDKs These crates are derived from Apple SDKs shipped with Xcode. You can obtain a copy of the Xcode license at: https://www.apple.com/legal/sla/docs/xcode.pdf Or by typing `xcodebuild -license` in your terminal. From reading the license, it is unclear whether distributing derived works such as these crates are allowed? But in any case, to practically use these crates, you will have to link, and that only works when you have the correct Xcode SDK available to provide the required `.tbd` files, which is why we choose to still use the normal SPDX identifiers in the crates (Xcode is required to use the crates, and when using Xcode you have already agreed to the Xcode license).
```

<a id="notice-2"></a>

### notice-2

```text
# Licenses This Rust implementation of the OpenEXR image format and the exrs crate are not affiliated with the [OpenEXR project](https://www.openexr.com/) or the [ACADEMY SOFTWARE FOUNDATION](https://www.aswf.io/). ## The OpenEXR Image Format: [BSD-3-Clause](https://github.com/AcademySoftwareFoundation/openexr/blob/master/LICENSE.md) Copyright (c) Contributors to the OpenEXR Project. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. ## The `exrs` Project, and independent Rust implementation of the official OpenEXR Image Format: Also [BSD-3-Clause](https://github.com/AcademySoftwareFoundation/openexr/blob/master/LICENSE.md) Copyright (c) Contributors to the exrs Project. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-3"></a>

### notice-3

```text
# This is the list of Fancy Regex authors for copyright purposes. # # This does not necessarily list everyone who has contributed code, since in # some cases, their employer may be the copyright holder. To see the full list # of contributors, see the revision history in source control. Google LLC Raph Levien Robin Stocker Keith Hall Jon Perry
```

<a id="notice-4"></a>

### notice-4

```text
# This is the list of SimpleCSS's significant contributors. # # This does not necessarily list everyone who has contributed code, # especially since many employees of one corporation may be contributing. # To see the full list of contributors, see the revision history in # source control. Yevhenii Reizner
```

<a id="notice-5"></a>

### notice-5

```text
# This is the list of svgtypes' significant contributors. # # This does not necessarily list everyone who has contributed code, # especially since many employees of one corporation may be contributing. # To see the full list of contributors, see the revision history in # source control. Yevhenii Reizner
```

<a id="notice-6"></a>

### notice-6

```text
(C) 2024 Trifecta Tech Foundation
```

<a id="notice-7"></a>

### notice-7

```text
- Armin Ronacher - Koka El Kiwi
```

<a id="notice-8"></a>

### notice-8

```text
// Copyright (c) 2022 zune-image developers // Licensed under the Apache License, Version 2.0 // <LICENSE-APACHE or // https://www.apache.org/licenses/> or the MIT // license <LICENSE-MIT or http://opensource.org/licenses/MIT>, // at your option. All files in the project carrying such // notice may not be copied, modified, or distributed except // according to those terms
```

<a id="notice-9"></a>

### notice-9

```text
=============================================================================== Copyright 2014-2020 The Rust Project Developers Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or https://www.apache.org/licenses/> or the MIT license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option. All files in the project carrying such notice may not be copied, modified, or distributed except according to those terms.
```

<a id="notice-10"></a>

### notice-10

```text
BSD 2-Clause License Copyright (c) 2017-2022, the rav1e contributors All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: - Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. - Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-11"></a>

### notice-11

```text
BSD 2-Clause License Copyright (c) 2017-2023, the rav1e contributors All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: * Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. * Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-12"></a>

### notice-12

```text
BSD 2-Clause License Copyright (c) 2022-2022, the rav1e contributors All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: - Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. - Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-13"></a>

### notice-13

```text
BSD 3-Clause License Copyright (c) 2018, the respective contributors, as shown by the AUTHORS file. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: * Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. * Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. * Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. ----- src/simd.rs is licensed under the MIT license: Copyright (c) 2006-2009 Graydon Hoare Copyright (c) 2009-2013 Mozilla Foundation
```

<a id="notice-14"></a>

### notice-14

```text
BSD 3-Clause License Copyright (c) 2020, Cloudflare, Inc. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-15"></a>

### notice-15

```text
BSD 3-Clause License Copyright (c) 2020, Kornel All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-16"></a>

### notice-16

```text
COPYING -- Describes the terms under which Notepad++ is distributed. A copy of the GNU GPL is appended to this file. IMPORTANT NOTEPAD++ LICENSE TERMS Copyright (C)2021 Don HO <don.h@free.fr>. This program is free software; you may redistribute and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation; Version 3 with the clarifications and exceptions described below. This guarantees your right to use, modify, and redistribute this software under certain conditions. This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details. ******************************************************************
```

<a id="notice-17"></a>

### notice-17

```text
Copyright (C) 2001-2002, International Business Machines Corporation and others. All Rights Reserved. UNICODE LICENSE V3 COPYRIGHT AND PERMISSION NOTICE Copyright © 2016-2024 Unicode, Inc. NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.  SPDX-License-Identifier: Unicode-3.0 ICU License - ICU 1.8.1 to ICU 57.1 COPYRIGHT AND PERMISSION NOTICE Copyright (c) 1995-2016 International Business Machines Corporation and others All rights reserved. Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, provided that the above copyright notice(s) and this permission notice appear in all copies of the Software and that both the above copyright notice(s) and this permission notice appear in supporting documentation. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE. Except as contained in this notice, the name of a copyright holder shall not be used in advertising or otherwise to promote the sale, use or other dealings in this Software without prior written authorization of the copyright holder. All trademarks and registered trademarks mentioned herein are the property of their respective owners.
```

<a id="notice-18"></a>

### notice-18

```text
Copyright (C) 2002 Scott Kirkwood Permission to use, copy, modify, distribute and sell this code and its documentation for any purpose is hereby granted without fee, provided that the above copyright notice appear in all copies or any derived copies. Scott Kirkwood makes no representations about the suitability of this software for any purpose. It is provided "as is" without express or implied warranty. Modificated 2006 Jens Lorenz - Clean up the sources - Removing UCS-Bug in Utf8_Iter - Add convert function in Utf8_16_Write
```

<a id="notice-19"></a>

### notice-19

```text
Copyright (C) Jonas Schievink <jonasschievink@gmail.com> Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted. THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

<a id="notice-20"></a>

### notice-20

```text
Copyright (c) 2002 John Maddock Copyright (c) 2004 John Maddock
```

<a id="notice-21"></a>

### notice-21

```text
Copyright (c) 2006-2009 Graydon Hoare Copyright (c) 2009-2013 Mozilla Foundation Copyright (c) 2016 Artyom Pavlov
```

<a id="notice-22"></a>

### notice-22

```text
Copyright (c) 2010 The Rust Project Developers
```

<a id="notice-23"></a>

### notice-23

```text
Copyright (c) 2011 Google Inc. All rights reserved. Copyright (c) 2020 Yevhenii Reizner All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: * Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. * Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. * Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-24"></a>

### notice-24

```text
Copyright (c) 2012 - present Edditoria
```

<a id="notice-25"></a>

### notice-25

```text
Copyright (c) 2012-2013 Mozilla Foundation
```

<a id="notice-26"></a>

### notice-26

```text
Copyright (c) 2013 Nicolas Silva
```

<a id="notice-27"></a>

### notice-27

```text
Copyright (c) 2013-2016 The rust-url developers
```

<a id="notice-28"></a>

### notice-28

```text
Copyright (c) 2013-2025 The rust-url developers
```

<a id="notice-29"></a>

### notice-29

```text
Copyright (c) 2014 - 2022 Knut Sveidqvist
```

<a id="notice-30"></a>

### notice-30

```text
Copyright (c) 2014 Alex Crichton
```

<a id="notice-31"></a>

### notice-31

```text
Copyright (c) 2014 Chris Wong
```

<a id="notice-32"></a>

### notice-32

```text
Copyright (c) 2014 Mathijs van de Nes
```

<a id="notice-33"></a>

### notice-33

```text
Copyright (c) 2014 Paho Lurie-Gregg
```

<a id="notice-34"></a>

### notice-34

```text
Copyright (c) 2014 The Rust Project Developers
```

<a id="notice-35"></a>

### notice-35

```text
Copyright (c) 2014 The Rust Project Developers Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon
```

<a id="notice-36"></a>

### notice-36

```text
Copyright (c) 2014-2019 Geoffroy Couprie
```

<a id="notice-37"></a>

### notice-37

```text
Copyright (c) 2014-2020 Optimal Computing (NZ) Ltd
```

<a id="notice-38"></a>

### notice-38

```text
Copyright (c) 2014-2026 Alex Crichton
```

<a id="notice-39"></a>

### notice-39

```text
Copyright (c) 2014-2026 Sean McArthur
```

<a id="notice-40"></a>

### notice-40

```text
Copyright (c) 2015
```

<a id="notice-41"></a>

### notice-41

```text
Copyright (c) 2015 Alice Maz
```

<a id="notice-42"></a>

### notice-42

```text
Copyright (c) 2015 Andrew Gallant
```

<a id="notice-43"></a>

### notice-43

```text
Copyright (c) 2015 Bartłomiej Kamiński
```

<a id="notice-44"></a>

### notice-44

```text
Copyright (c) 2015 David Roundy <roundyd@physics.oregonstate.edu> All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-45"></a>

### notice-45

```text
Copyright (c) 2015 Jonathan Reem
```

<a id="notice-46"></a>

### notice-46

```text
Copyright (c) 2015 Steven Allen
```

<a id="notice-47"></a>

### notice-47

```text
Copyright (c) 2015 The Gimli Developers
```

<a id="notice-48"></a>

### notice-48

```text
Copyright (c) 2015 The Rust Project Developers
```

<a id="notice-49"></a>

### notice-49

```text
Copyright (c) 2015 The quick-error Developers
```

<a id="notice-50"></a>

### notice-50

```text
Copyright (c) 2015 The rust-float-ord Developers
```

<a id="notice-51"></a>

### notice-51

```text
Copyright (c) 2015 nwin
```

<a id="notice-52"></a>

### notice-52

```text
Copyright (c) 2015-2019 PistonDevelopers Copyright (c) 2019 image-rs contributors
```

<a id="notice-53"></a>

### notice-53

```text
Copyright (c) 2015-2025 Sean McArthur
```

<a id="notice-54"></a>

### notice-54

```text
Copyright (c) 2016 Alex Crichton Copyright (c) 2017 The Tokio Authors
```

<a id="notice-55"></a>

### notice-55

```text
Copyright (c) 2016 Alex Crichton Copyright (c) 2017 The Tokio Authors Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-56"></a>

### notice-56

```text
Copyright (c) 2016 Amanieu d'Antras
```

<a id="notice-57"></a>

### notice-57

```text
Copyright (c) 2016 Anthony Ramine
```

<a id="notice-58"></a>

### notice-58

```text
Copyright (c) 2016 Jelte Fennema
```

<a id="notice-59"></a>

### notice-59

```text
Copyright (c) 2016 Philipp Oppermann
```

<a id="notice-60"></a>

### notice-60

```text
Copyright (c) 2016 PistonDevelopers
```

<a id="notice-61"></a>

### notice-61

```text
Copyright (c) 2016 The Rust Project Developers
```

<a id="notice-62"></a>

### notice-62

```text
Copyright (c) 2016--2017
```

<a id="notice-63"></a>

### notice-63

```text
Copyright (c) 2016--2023
```

<a id="notice-64"></a>

### notice-64

```text
Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved. Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-65"></a>

### notice-65

```text
Copyright (c) 2016-2018 The gimli Developers
```

<a id="notice-66"></a>

### notice-66

```text
Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers
```

<a id="notice-67"></a>

### notice-67

```text
Copyright (c) 2017 Andrew Gallant
```

<a id="notice-68"></a>

### notice-68

```text
Copyright (c) 2017 Artyom Pavlov
```

<a id="notice-69"></a>

### notice-69

```text
Copyright (c) 2017 Brian Langenberger
```

<a id="notice-70"></a>

### notice-70

```text
Copyright (c) 2017 Jorge Aparicio
```

<a id="notice-71"></a>

### notice-71

```text
Copyright (c) 2017 Maiddog
```

<a id="notice-72"></a>

### notice-72

```text
Copyright (c) 2017 Robert Grosse
```

<a id="notice-73"></a>

### notice-73

```text
Copyright (c) 2017 The foreign-types Developers
```

<a id="notice-74"></a>

### notice-74

```text
Copyright (c) 2017 http-rs authors
```

<a id="notice-75"></a>

### notice-75

```text
Copyright (c) 2018 Carl Lerche
```

<a id="notice-76"></a>

### notice-76

```text
Copyright (c) 2018 Jorge Aparicio
```

<a id="notice-77"></a>

### notice-77

```text
Copyright (c) 2018 Luca Barbato
```

<a id="notice-78"></a>

### notice-78

```text
Copyright (c) 2018 PistonDevelopers
```

<a id="notice-79"></a>

### notice-79

```text
Copyright (c) 2018 Raph Levien
```

<a id="notice-80"></a>

### notice-80

```text
Copyright (c) 2018 Reizner Evgeniy
```

<a id="notice-81"></a>

### notice-81

```text
Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors
```

<a id="notice-82"></a>

### notice-82

```text
Copyright (c) 2018 The Servo Project Developers
```

<a id="notice-83"></a>

### notice-83

```text
Copyright (c) 2018 Visly Inc. Copyright (c) 2026 Taffy Authors
```

<a id="notice-84"></a>

### notice-84

```text
Copyright (c) 2018 Yevhenii Reizner
```

<a id="notice-85"></a>

### notice-85

```text
Copyright (c) 2018-2019 The RustCrypto Project Developers
```

<a id="notice-86"></a>

### notice-86

```text
Copyright (c) 2018-2019 dirs-rs contributors
```

<a id="notice-87"></a>

### notice-87

```text
Copyright (c) 2018-2025 The rust-random Project Developers Copyright (c) 2014 The Rust Project Developers
```

<a id="notice-88"></a>

### notice-88

```text
Copyright (c) 2018-2026 The rust-random Project Developers Copyright (c) 2014 The Rust Project Developers
```

<a id="notice-89"></a>

### notice-89

```text
Copyright (c) 2019 Carl Lerche
```

<a id="notice-90"></a>

### notice-90

```text
Copyright (c) 2019 Daniel "Lokathor" Gee.
```

<a id="notice-91"></a>

### notice-91

```text
Copyright (c) 2019 Daniel "Lokathor" Gee. Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions: The above copyright notice and this permission notice (including the next paragraph) shall be included in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

<a id="notice-92"></a>

### notice-92

```text
Copyright (c) 2019 Graham Esau
```

<a id="notice-93"></a>

### notice-93

```text
Copyright (c) 2019 Kornel
```

<a id="notice-94"></a>

### notice-94

```text
Copyright (c) 2019 Luca Barbato
```

<a id="notice-95"></a>

### notice-95

```text
Copyright (c) 2019 Manish Goregaokar
```

<a id="notice-96"></a>

### notice-96

```text
Copyright (c) 2019 Multimedia and Rust
```

<a id="notice-97"></a>

### notice-97

```text
Copyright (c) 2019 Nick Fitzgerald
```

<a id="notice-98"></a>

### notice-98

```text
Copyright (c) 2019 Nick Fitzgerald, 2021 Yuki Okushi
```

<a id="notice-99"></a>

### notice-99

```text
Copyright (c) 2019 Nick Fitzgerald, 2021 Yuki Okushi All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-100"></a>

### notice-100

```text
Copyright (c) 2019 Nicolas Silva
```

<a id="notice-101"></a>

### notice-101

```text
Copyright (c) 2019 Osspial
```

<a id="notice-102"></a>

### notice-102

```text
Copyright (c) 2019 Peter Glotfelty
```

<a id="notice-103"></a>

### notice-103

```text
Copyright (c) 2019 Reizner Evgeniy
```

<a id="notice-104"></a>

### notice-104

```text
Copyright (c) 2019 The Crossbeam Project Developers
```

<a id="notice-105"></a>

### notice-105

```text
Copyright (c) 2019 The CryptoCorrosion Contributors
```

<a id="notice-106"></a>

### notice-106

```text
Copyright (c) 2019 Tokio Contributors
```

<a id="notice-107"></a>

### notice-107

```text
Copyright (c) 2019 Yevhenii Reizner
```

<a id="notice-108"></a>

### notice-108

```text
Copyright (c) 2019-2026 Sean McArthur & Hyper Contributors
```

<a id="notice-109"></a>

### notice-109

```text
Copyright (c) 2020 Andrew D. Straw
```

<a id="notice-110"></a>

### notice-110

```text
Copyright (c) 2020 Nicolas Silva
```

<a id="notice-111"></a>

### notice-111

```text
Copyright (c) 2020 Osspial
```

<a id="notice-112"></a>

### notice-112

```text
Copyright (c) 2020 Philip Degarmo and other contributors
```

<a id="notice-113"></a>

### notice-113

```text
Copyright (c) 2020 Scripta Qumranica Electronica Created by Bronson Brown-deVost
```

<a id="notice-114"></a>

### notice-114

```text
Copyright (c) 2020 Yevhenii Reizner
```

<a id="notice-115"></a>

### notice-115

```text
Copyright (c) 2020 Yevhenii Reizner Copyright (c) 2015 Dan Burkert
```

<a id="notice-116"></a>

### notice-116

```text
Copyright (c) 2020-2021 Brendan Molloy <brendan@bbqsrc.net>
```

<a id="notice-117"></a>

### notice-117

```text
Copyright (c) 2020-2021 Joshua Barretto
```

<a id="notice-118"></a>

### notice-118

```text
Copyright (c) 2020-2025 The RustCrypto Project Developers
```

<a id="notice-119"></a>

### notice-119

```text
Copyright (c) 2021 Joshua Holmer
```

<a id="notice-120"></a>

### notice-120

```text
Copyright (c) 2021 Orson Peters <orsonpeters@gmail.com>
```

<a id="notice-121"></a>

### notice-121

```text
Copyright (c) 2021 RustCrypto Developers
```

<a id="notice-122"></a>

### notice-122

```text
Copyright (c) 2021 sarah
```

<a id="notice-123"></a>

### notice-123

```text
Copyright (c) 2022 Contributors to the lebe Project. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-124"></a>

### notice-124

```text
Copyright (c) 2022 Ivan Smirnov
```

<a id="notice-125"></a>

### notice-125

```text
Copyright (c) 2022 Yevhenii Reizner
```

<a id="notice-126"></a>

### notice-126

```text
Copyright (c) 2022 sarah
```

<a id="notice-127"></a>

### notice-127

```text
Copyright (c) 2023 Nathan Vegdahl
```

<a id="notice-128"></a>

### notice-128

```text
Copyright (c) 2023 The Rust Project Developers
```

<a id="notice-129"></a>

### notice-129

```text
Copyright (c) 2023 dAxpeDDa
```

<a id="notice-130"></a>

### notice-130

```text
Copyright (c) 2023 sarah
```

<a id="notice-131"></a>

### notice-131

```text
Copyright (c) 2024 Orson Peters
```

<a id="notice-132"></a>

### notice-132

```text
Copyright (c) 2024 Robert Bastian
```

<a id="notice-133"></a>

### notice-133

```text
Copyright (c) 2025 rutrum
```

<a id="notice-134"></a>

### notice-134

```text
Copyright (c) HarfBuzz developers Copyright (c) 2020 Yevhenii Reizner
```

<a id="notice-135"></a>

### notice-135

```text
Copyright (c) HeroicKatora 2020
```

<a id="notice-136"></a>

### notice-136

```text
Copyright (c) Individual contributors
```

<a id="notice-137"></a>

### notice-137

```text
Copyright (c) Microsoft Corporation. Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions: The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE
```

<a id="notice-138"></a>

### notice-138

```text
Copyright (c) Radzivon Bartoshyk. All rights reserved. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-139"></a>

### notice-139

```text
Copyright (c) Steven Sheldon
```

<a id="notice-140"></a>

### notice-140

```text
Copyright (c) The Rust Project Developers
```

<a id="notice-141"></a>

### notice-141

```text
Copyright (c) The rust-url developers
```

<a id="notice-142"></a>

### notice-142

```text
Copyright (c) Ulrik Sverdrup "bluss" 2015-2023
```

<a id="notice-143"></a>

### notice-143

```text
Copyright (c) [2021] [Marvin Countryman]
```

<a id="notice-144"></a>

### notice-144

```text
Copyright (c) zune-image developers
```

<a id="notice-145"></a>

### notice-145

```text
Copyright 2012-2016 The Rust Project Developers. Copyright 2016-2026 Frank Denis. Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or https://www.apache.org/licenses/> or the MIT license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
```

<a id="notice-146"></a>

### notice-146

```text
Copyright 2013-2014 RAD Game Tools and Valve Software Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC Copyright (c) 2017 Frommi Copyright (c) 2017-2024 oyvindln
```

<a id="notice-147"></a>

### notice-147

```text
Copyright 2013-2014 RAD Game Tools and Valve Software Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC Copyright (c) 2020 Frommi Copyright (c) 2017-2024 oyvindln
```

<a id="notice-148"></a>

### notice-148

```text
Copyright 2014 Paho Lurie-Gregg Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-149"></a>

### notice-149

```text
Copyright 2015 Google Inc. All rights reserved.
```

<a id="notice-150"></a>

### notice-150

```text
Copyright 2015 Google Inc. All rights reserved.  URL resolution: url 2.5.8 https://github.com/servo/rust-url The MIT option is used from the package's dual license. Copyright (c) 2013-2025 The rust-url developers  Parser case matching dependency: unicase 2.10.0 https://github.com/seanmonstar/unicase The MIT option is used from the package's dual license. Copyright (c) 2014-2026 Sean McArthur
```

<a id="notice-151"></a>

### notice-151

```text
Copyright 2015 The Fancy Regex Authors.
```

<a id="notice-152"></a>

### notice-152

```text
Copyright 2016 Nika Layzell
```

<a id="notice-153"></a>

### notice-153

```text
Copyright 2017 http-rs authors Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-154"></a>

### notice-154

```text
Copyright 2017 the Resvg Authors
```

<a id="notice-155"></a>

### notice-155

```text
Copyright 2018 Developers of the Rand project Copyright (c) 2014 The Rust Project Developers
```

<a id="notice-156"></a>

### notice-156

```text
Copyright 2019 The CryptoCorrosion Contributors Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-157"></a>

### notice-157

```text
Copyright 2019 The Fuchsia Authors. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: * Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. * Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-158"></a>

### notice-158

```text
Copyright 2020 Andrew Straw Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-159"></a>

### notice-159

```text
Copyright 2020 Nicolas Silva Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-160"></a>

### notice-160

```text
Copyright 2021 The Unicode-rs Developers Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-161"></a>

### notice-161

```text
Copyright 2022 - 2025 Zed Industries, Inc. Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-162"></a>

### notice-162

```text
Copyright 2022 by Neil Hodgson <neilh@scintilla.org> License for Lexilla, Scintilla, and SciTE Copyright 1998-2021 by Neil Hodgson <neilh@scintilla.org> All Rights Reserved
```

<a id="notice-163"></a>

### notice-163

```text
Copyright 2023 The Fuchsia Authors
```

<a id="notice-164"></a>

### notice-164

```text
Copyright 2023 The Fuchsia Authors Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-165"></a>

### notice-165

```text
Copyright 2023 dAxpeDDa Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-166"></a>

### notice-166

```text
Copyright 2024 Radzivon Bartoshyk Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-167"></a>

### notice-167

```text
Copyright Individual contributors Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-168"></a>

### notice-168

```text
Copyright Mozilla Foundation
```

<a id="notice-169"></a>

### notice-169

```text
Copyright [2015] [Dan Burkert] Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-170"></a>

### notice-170

```text
Copyright {yyyy} {name of copyright owner} Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at https://www.apache.org/licenses/ Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
```

<a id="notice-171"></a>

### notice-171

```text
Copyright © 2021 The pdf-rs contributers.
```

<a id="notice-172"></a>

### notice-172

```text
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft). Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

<a id="notice-173"></a>

### notice-173

```text
Creative Commons Legal Code CC0 1.0 Universal CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER. Statement of Purpose The laws of most jurisdictions throughout the world automatically confer exclusive Copyright and Related Rights (defined below) upon the creator and subsequent owner(s) (each and all, an "owner") of an original work of authorship and/or a database (each, a "Work"). Certain owners wish to permanently relinquish those rights to a Work for the purpose of contributing to a commons of creative, cultural and scientific works ("Commons") that the public can reliably and without fear of later claims of infringement build upon, modify, incorporate in other works, reuse and redistribute as freely as possible in any form whatsoever and for any purposes, including without limitation commercial purposes. These owners may contribute to the Commons to promote the ideal of a free culture and the further production of creative, cultural and scientific works, or to gain reputation or greater distribution for their Work in part through the use and efforts of others. For these and/or other purposes and motivations, and without any expectation of additional consideration or compensation, the person associating CC0 with a Work (the "Affirmer"), to the extent that he or she is an owner of Copyright and Related Rights in the Work, voluntarily elects to apply CC0 to the Work and publicly distribute the Work under its terms, with knowledge of his or her Copyright and Related Rights in the Work and the meaning and intended legal effect of CC0 on those rights. 1. Copyright and Related Rights. A Work made available under CC0 may be protected by copyright and related or neighboring rights ("Copyright and Related Rights"). Copyright and Related Rights include, but are not limited to, the following: i. the right to reproduce, adapt, distribute, perform, display, communicate, and translate a Work; ii. moral rights retained by the original author(s) and/or performer(s); iii. publicity and privacy rights pertaining to a person's image or likeness depicted in a Work; iv. rights protecting against unfair competition in regards to a Work, subject to the limitations in paragraph 4(a), below; v. rights protecting the extraction, dissemination, use and reuse of data in a Work; vi. database rights (such as those arising under Directive 96/9/EC of the European Parliament and of the Council of 11 March 1996 on the legal protection of databases, and under any national implementation thereof, including any amended or successor version of such directive); and vii. other similar, equivalent or corresponding rights throughout the world based on applicable law or treaty, and any national implementations thereof. 2. Waiver. To the greatest extent permitted by, but not in contravention of, applicable law, Affirmer hereby overtly, fully, permanently, irrevocably and unconditionally waives, abandons, and surrenders all of Affirmer's Copyright and Related Rights and associated claims and causes of action, whether now known or unknown (including existing as well as future claims and causes of action), in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each member of the public at large and to the detriment of Affirmer's heirs and successors, fully intending that such Waiver shall not be subject to revocation, rescission, cancellation, termination, or any other legal or equitable action to disrupt the quiet enjoyment of the Work by the public as contemplated by Affirmer's express Statement of Purpose. 3. Public License Fallback. Should any part of the Waiver for any reason be judged legally invalid or ineffective under applicable law, then the Waiver shall be preserved to the maximum extent permitted taking into account Affirmer's express Statement of Purpose. In addition, to the extent the Waiver is so judged Affirmer hereby grants to each affected person a royalty-free, non transferable, non sublicensable, non exclusive, irrevocable and unconditional license to exercise Affirmer's Copyright and Related Rights in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium and for any number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "License"). The License shall be deemed effective as of the date CC0 was applied by Affirmer to the Work. Should any part of the License for any reason be judged legally invalid or ineffective under applicable law, such partial invalidity or ineffectiveness shall not invalidate the remainder of the License, and in such case Affirmer hereby affirms that he or she will not (i) exercise any of his or her remaining Copyright and Related Rights in the Work or (ii) assert any associated claims and causes of action with respect to the Work, in either case contrary to Affirmer's express Statement of Purpose. 4. Limitations and Disclaimers. a. No trademark or patent rights held by Affirmer are waived, abandoned, surrendered, licensed or otherwise affected by this document. b. Affirmer offers the Work as-is and makes no representations or warranties of any kind concerning the Work, express, implied, statutory or otherwise, including without limitation warranties of title, merchantability, fitness for a particular purpose, non infringement, or the absence of latent or other defects, accuracy, or the present or absence of errors, whether or not discoverable, all to the greatest extent permissible under applicable law. c. Affirmer disclaims responsibility for clearing rights of other persons that may apply to the Work or any use thereof, including without limitation any person's Copyright and Related Rights in the Work. Further, Affirmer disclaims responsibility for obtaining any necessary consents, permissions or other rights required for any use of the Work. d. Affirmer understands and acknowledges that Creative Commons is not a party to this document and has no duty or obligation with respect to this CC0 or use of the Work.
```

<a id="notice-174"></a>

### notice-174

```text
Declared license: MIT OR Apache-2.0 OR Zlib. Copyright (c) 2023.
```

<a id="notice-175"></a>

### notice-175

```text
Declared license: MIT OR Apache-2.0 OR Zlib. Package authors: caleb <etemesicaleb@gmail.com>. Copyright (c) 2023. Copyright (c) 2025.
```

<a id="notice-176"></a>

### notice-176

```text
Declared license: MIT.
```

<a id="notice-177"></a>

### notice-177

```text
Declared license: MIT. Package authors: Luca Barbato <lu_zero@gentoo.org>.
```

<a id="notice-178"></a>

### notice-178

```text
Declared license: MIT. Package authors: Mads Marquart <mads@marquart.dk>.
```

<a id="notice-179"></a>

### notice-179

```text
Declared license: MIT. Package authors: Steven Sheldon; Mads Marquart <mads@marquart.dk>.
```

<a id="notice-180"></a>

### notice-180

```text
Declared license: MIT. Package authors: ticki <ticki@users.noreply.github.com>; Tom Almeida <tom@tommoa.me>.
```

<a id="notice-181"></a>

### notice-181

```text
Declared license: MIT/Apache-2.0. Package authors: Patrick Walton <pcwalton@mimiga.net>. Copyright © 2019 The Pathfinder Project Developers. Copyright © 2020 The Pathfinder Project Developers.
```

<a id="notice-182"></a>

### notice-182

```text
Declared license: Zlib OR Apache-2.0 OR MIT.
```

<a id="notice-183"></a>

### notice-183

```text
Declared license: Zlib OR Apache-2.0 OR MIT. Package authors: Mads Marquart <mads@marquart.dk>; Mary <mary@mary.zone>.
```

<a id="notice-184"></a>

### notice-184

```text
JSON lexer original author: nkmathew. CaseConvert.cxx: Copyright 2013 Neil Hodgson <neilh@scintilla.org>. Change-history adaptations dated 2026-11-11; Unicode I/O adaptations dated 2026-11-10.
```

<a id="notice-185"></a>

### notice-185

```text
MIT OR Apache-2.0
```

<a id="notice-186"></a>

### notice-186

```text
Rust UDL lexer: Copyright (C) 2003 Don HO; GPL version 2 or later, used under GPL version 3. URI scanner: Copyright (C) 2021 Don HO <don.h@free.fr>; GPL version 3 or later. Converter: Copyright (C) 2022 Don HO <don.h@free.fr>; GPL version 3 or later. These components were adapted for Notepad Matrix; upstream libraries are not linked.
```

<a id="notice-187"></a>

### notice-187

```text
Rust-chrono is dual-licensed under The MIT License [1] and Apache 2.0 License [2]. Copyright (c) 2014--2026, Kang Seonghoon and contributors. Nota Bene: This is same as the Rust Project's own license. [1]: <http://opensource.org/licenses/MIT>, which is reproduced below: ~~~~ The MIT License (MIT) Copyright (c) 2014, Kang Seonghoon.  ~~~~ [2]: <https://www.apache.org/licenses/>, which is reproduced below: ~~~~
```

<a id="notice-188"></a>

### notice-188

```text
This project is dual-licensed under the Unlicense and MIT licenses. You may use this code under the terms of either license.
```

<a id="notice-189"></a>

### notice-189

```text
This software was written by the following people: Matt Brubeck <mbrubeck@limpet.net> Behnam Esfahbod <behnam@zwnj.org>
```

<a id="notice-190"></a>

### notice-190

```text
UNICODE LICENSE V3 COPYRIGHT AND PERMISSION NOTICE Copyright © 1991-2023 Unicode, Inc. NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.
```

<a id="notice-191"></a>

### notice-191

```text
UNICODE LICENSE V3 COPYRIGHT AND PERMISSION NOTICE Copyright © 1991-2026 Unicode, Inc. NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.
```

<a id="notice-192"></a>

### notice-192

```text
UNICODE LICENSE V3 COPYRIGHT AND PERMISSION NOTICE Copyright © 2020-2024 Unicode, Inc. NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.  SPDX-License-Identifier: Unicode-3.0 — Portions of ICU4X may have been adapted from ICU4C and/or ICU4J. ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

<a id="notice-193"></a>

### notice-193

```text
UNICODE, INC. LICENSE AGREEMENT - DATA FILES AND SOFTWARE Unicode Data Files include all data files under the directories http://www.unicode.org/Public/, http://www.unicode.org/reports/, http://www.unicode.org/cldr/data/, http://source.icu-project.org/repos/icu/, and http://www.unicode.org/utility/trac/browser/. Unicode Data Files do not include PDF online code charts under the directory http://www.unicode.org/Public/. Software includes any source code published in the Unicode Standard or under the directories http://www.unicode.org/Public/, http://www.unicode.org/reports/, http://www.unicode.org/cldr/data/, http://source.icu-project.org/repos/icu/, and http://www.unicode.org/utility/trac/browser/. NOTICE TO USER: Carefully read the following legal agreement. BY DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING UNICODE INC.'S DATA FILES ("DATA FILES"), AND/OR SOFTWARE ("SOFTWARE"), YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE. COPYRIGHT AND PERMISSION NOTICE Copyright © 1991-2018 Unicode, Inc. All rights reserved. Distributed under the Terms of Use in http://www.unicode.org/copyright.html. Permission is hereby granted, free of charge, to any person obtaining a copy of the Unicode data files and any associated documentation (the "Data Files") or Unicode software and any associated documentation (the "Software") to deal in the Data Files or Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, and/or sell copies of the Data Files or Software, and to permit persons to whom the Data Files or Software are furnished to do so, provided that either (a) this copyright and permission notice appear with all copies of the Data Files or Software, or (b) this copyright and permission notice appear in associated Documentation. THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA FILES OR SOFTWARE. Except as contained in this notice, the name of a copyright holder shall not be used in advertising or otherwise to promote the sale, use or other dealings in these Data Files or Software without prior written authorization of the copyright holder.
```

<a id="notice-194"></a>

### notice-194

```text
We, the copyright holders of this work, hereby release it into the public domain. This applies worldwide. In case this is not legally possible, We grant any entity the right to use this work for any purpose, without any conditions, unless such conditions are required by law. Thatcher Ulrich <tu@tulrich.com> http://tulrich.com Karoly Barta bartakarcsi@gmail.com Michael Evans http://www.evertype.com
```

<a id="notice-195"></a>

### notice-195

```text
dompurify/dist/purify.es.mjs: (*! @license DOMPurify 3.4.17 | (c) Cure53 and other contributors | Released under the Apache license 2.0 and Mozilla Public License 2.0 | github.com/cure53/DOMPurify/blob/3.4.17/LICENSE *) (*! regenerator-runtime -- Copyright (c) 2014-present, Facebook, Inc. -- license (MIT): https://github.com/babel/babel/blob/main/packages/babel-helpers/LICENSE *) lodash-es/lodash.js: (** * @license * Lodash (Custom Build) <https://lodash.com/> * Build: `lodash modularize exports="es" --repo lodash/lodash#4.18.1 -o ./` * Copyright OpenJS Foundation and other contributors <https://openjsf.org/> * Released under MIT license <https://lodash.com/license> * Based on Underscore.js 1.8.3 <http://underscorejs.org/LICENSE> * Copyright Jeremy Ashkenas, DocumentCloud and Investigative Reporters & Editors *) cytoscape/dist/cytoscape.esm.mjs: (*! Embeddable Minimum Strictly-Compliant Promises/A+ 1.1.1 Thenable Copyright (c) 2013-2014 Ralf S. Engelschall (http://engelschall.com) Licensed under The MIT License (http://opensource.org/licenses/MIT) *) (*! Event object based on jQuery events, MIT license https://jquery.org/license/ https://tldrlegal.com/license/mit-license https://github.com/jquery/jquery/blob/master/src/event.js *) (*! Bezier curve function generator. Copyright Gaetan Renaudeau. MIT License: http://en.wikipedia.org/wiki/MIT_License *) (*! Runge-Kutta spring physics function generator. Adapted from Framer.js, copyright Koen Bok. MIT License: http://en.wikipedia.org/wiki/MIT_License *)
```

<a id="notice-196"></a>

### notice-196

```text
rust-lang/libm as a whole is available for use under the MIT license: ------------------------------------------------------------------------------  ------------------------------------------------------------------------------ As a contributor, you agree that your code can be used under either the MIT license or the Apache-2.0 license: ------------------------------------------------------------------------------   Copyright (c) 2018 Jorge Aparicio Portions of this software are derived from third-party works licensed under terms compatible with the above MIT license: * musl libc https://www.musl-libc.org/. This library contains the following copyright: Copyright © 2005-2020 Rich Felker, et al. * The CORE-MATH project https://core-math.gitlabpages.inria.fr/. CORE-MATH routines are available under the MIT license on a per-file basis. The musl libc COPYRIGHT file also includes the following notice relevant to math portions of the library: ------------------------------------------------------------------------------ Much of the math library code (src/math/* and src/complex/*) is Copyright © 1993,2004 Sun Microsystems or Copyright © 2003-2011 David Schultz or Copyright © 2003-2009 Steven G. Kargl or Copyright © 2003-2009 Bruce D. Evans or Copyright © 2008 Stephen L. Moshier or Copyright © 2017-2018 Arm Limited and labelled as such in comments in the individual source files. All have been licensed under extremely permissive terms. ------------------------------------------------------------------------------ Copyright notices are retained in src/* files where relevant.
```

<a id="notice-197"></a>

### notice-197

```text
zlib License (C) zune-image developers
```

<a id="notice-198"></a>

### notice-198

```text
© Kornel Lesiński
```
