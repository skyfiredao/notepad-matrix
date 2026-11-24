# gpui-pre-macos 0.3.11

Source: https://crates.io/crates/gpui-pre-macos/0.3.11

Upstream Zed revision: `f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`.

Local changes dated 2026-11-24:

- Require macOS 14 or later when opening a window.
- Keep the existing `CAMetalDisplayLink` frame source and idle pause behavior.
- Remove the older `CVDisplayLink` frame source, native declarations and associated tests.
- Select this source with the root Cargo manifest's crates.io patch.

License: [Apache-2.0](LICENSE-APACHE). Unchanged upstream source retains its original notices.
