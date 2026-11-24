use std::sync::{Arc, OnceLock};

use gpui::{Image, ImageFormat};

#[derive(Clone, Copy)]
pub(super) enum ToolbarIcon {
    New,
    Open,
    Save,
    SaveAll,
    Close,
    CloseAll,
    Print,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    Find,
    Replace,
    DocumentList,
    ZoomIn,
    ZoomOut,
    WordWrap,
    SyncVertical,
    SyncHorizontal,
    DefineUserLanguage,
    AllCharacters,
    IndentGuides,
    DocumentMap,
    FolderWorkspace,
    FunctionList,
    Monitoring,
    StartRecording,
    StopRecording,
    Playback,
    RunMacroMultiple,
    SaveMacro,
    MarkdownPreview,
    SavedDocument,
    ModifiedDocument,
    SavedDocumentDark,
    ModifiedDocumentDark,
    ReadOnlyDocument,
    ReadOnlyDocumentDark,
    SystemReadOnlyDocument,
    SystemReadOnlyDocumentDark,
    MonitoringDocument,
    MonitoringDocumentDark,
    PinTab,
    PinnedTab,
    PinTabDark,
    PinnedTabDark,
    CloseTab,
    CloseTabHover,
    CloseTabOnTab,
    CloseTabPressed,
    CloseTabDark,
    CloseTabHoverDark,
    CloseTabOnTabDark,
    CloseTabPressedDark,
    Application,
    StatusInfo,
}

macro_rules! embedded_image {
    ($path:literal) => {{
        static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
        IMAGE
            .get_or_init(|| {
                Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    include_bytes!(concat!("../../assets/ui/", $path)).to_vec(),
                ))
            })
            .clone()
    }};
}

impl ToolbarIcon {
    pub(super) fn tab_image(self, alternate: bool) -> Arc<Image> {
        if !alternate {
            return self.image();
        }
        macro_rules! alternative {
            ($name:literal) => {{
                static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
                IMAGE.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Svg,
                    include_bytes!(concat!("../../assets/ui/icons/standard/tabbar/", $name, "_alt.svg")).to_vec()))).clone()
            }};
        }
        match self {
            Self::SavedDocument | Self::SavedDocumentDark => alternative!("saved"),
            Self::ModifiedDocument | Self::ModifiedDocumentDark => alternative!("unsaved"),
            Self::ReadOnlyDocument | Self::ReadOnlyDocumentDark => alternative!("readonly"),
            Self::SystemReadOnlyDocument | Self::SystemReadOnlyDocumentDark => {
                alternative!("readonly_sys")
            }
            _ => self.image(),
        }
    }

    pub(super) fn has_disabled_fluent(self) -> bool {
        matches!(
            self,
            Self::Save
                | Self::SaveAll
                | Self::Cut
                | Self::Copy
                | Self::Paste
                | Self::Undo
                | Self::Redo
                | Self::SyncVertical
                | Self::SyncHorizontal
                | Self::Monitoring
                | Self::StartRecording
                | Self::StopRecording
                | Self::Playback
                | Self::RunMacroMultiple
                | Self::SaveMacro
        )
    }

    pub(super) fn preference_image(
        self,
        view: &super::MatrixWindow,
        cx: &gpui::App,
        enabled: bool,
    ) -> Arc<Image> {
        let set = view.pref_string(
            "ToolBar.iconSet",
            if view.dark { "small" } else { "standard" },
        );
        if set == "standard" {
            return self.image();
        }
        macro_rules! fluent {
            ($name:literal) => { fluent!($name, "off") };
            ($name:literal, $state:literal) => {{
                let bytes: &[u8] = match (view.dark, matches!(set.as_str(), "small2" | "large2")) {
                    (false, false) => include_bytes!(concat!("../../assets/ui/icons/light/toolbar/regular/", $name, "_", $state, ".svg")),
                    (false, true) => include_bytes!(concat!("../../assets/ui/icons/light/toolbar/filled/", $name, "_", $state, ".svg")),
                    (true, false) => include_bytes!(concat!("../../assets/ui/icons/dark/toolbar/regular/", $name, "_", $state, ".svg")),
                    (true, true) => include_bytes!(concat!("../../assets/ui/icons/dark/toolbar/filled/", $name, "_", $state, ".svg")),
                };
                Arc::new(Image::from_bytes(ImageFormat::Svg, bytes.to_vec()))
            }};
        }
        if !enabled && self.has_disabled_fluent() {
            return match self {
                Self::Save => fluent!("save", "dis"),
                Self::SaveAll => fluent!("saveall", "dis"),
                Self::Cut => fluent!("cut", "dis"),
                Self::Copy => fluent!("copy", "dis"),
                Self::Paste => fluent!("paste", "dis"),
                Self::Undo => fluent!("undo", "dis"),
                Self::Redo => fluent!("redo", "dis"),
                Self::SyncVertical => fluent!("syncV", "dis"),
                Self::SyncHorizontal => fluent!("syncH", "dis"),
                Self::Monitoring => fluent!("monitoring", "dis"),
                Self::StartRecording => fluent!("startrecord", "dis"),
                Self::StopRecording => fluent!("stoprecord", "dis"),
                Self::Playback => fluent!("playrecord", "dis"),
                Self::RunMacroMultiple => fluent!("playrecord_m", "dis"),
                Self::SaveMacro => fluent!("saverecord", "dis"),
                _ => self.image(),
            };
        }
        let image = match self {
            Self::New => fluent!("new"),
            Self::Open => fluent!("open"),
            Self::Save => fluent!("save"),
            Self::SaveAll => fluent!("saveall"),
            Self::Close => fluent!("close"),
            Self::CloseAll => fluent!("closeall"),
            Self::Print => fluent!("print"),
            Self::Cut => fluent!("cut"),
            Self::Copy => fluent!("copy"),
            Self::Paste => fluent!("paste"),
            Self::Undo => fluent!("undo"),
            Self::Redo => fluent!("redo"),
            Self::Find => fluent!("find"),
            Self::Replace => fluent!("findrep"),
            Self::DocumentList => fluent!("docList"),
            Self::ZoomIn => fluent!("zoomIn"),
            Self::ZoomOut => fluent!("zoomOut"),
            Self::WordWrap => fluent!("wrap"),
            Self::SyncVertical => fluent!("syncV"),
            Self::SyncHorizontal => fluent!("syncH"),
            Self::DefineUserLanguage => fluent!("udl"),
            Self::AllCharacters => fluent!("allChars"),
            Self::IndentGuides => fluent!("indentGuide"),
            Self::DocumentMap => fluent!("docMap"),
            Self::FolderWorkspace => fluent!("fileBrowser"),
            Self::FunctionList => fluent!("funcList"),
            Self::Monitoring => fluent!("monitoring"),
            Self::StartRecording => fluent!("startrecord"),
            Self::StopRecording => fluent!("stoprecord"),
            Self::Playback => fluent!("playrecord"),
            Self::RunMacroMultiple => fluent!("playrecord_m"),
            Self::SaveMacro => fluent!("saverecord"),
            _ => return self.image(),
        };
        let choice = view.pref_int("ToolBar.fluentColor", 0);
        let mono = view.pref_bool("ToolBar.fluentMono", false);
        let color = match choice {
            1 => 0xe81123,
            2 => 0x008b00,
            3 => 0x0078d4,
            4 => 0xb146c2,
            5 => 0x00b7c3,
            6 => 0x498205,
            7 => 0xffb900,
            9 => {
                let value = view.pref_int("ToolBar.fluentCustomColor", 0) as u32;
                ((value & 255) << 16) | (value & 0xff00) | ((value >> 16) & 255)
            }
            8 => system_accent_color(),
            _ => {
                if mono {
                    if view.dark { 0xdedede } else { 0x212121 }
                } else {
                    return image;
                }
            }
        };
        let color = if choice == 9 && color == 0 {
            if mono {
                if view.dark { 0xdedede } else { 0x212121 }
            } else {
                return image;
            }
        } else {
            color
        };
        let key = (image.id(), color, mono);
        if let Some(cached) = ICON_CACHE.with(|cache| cache.borrow().1.get(&key).cloned()) {
            return cached;
        }
        let rendered = match image.to_image_data(cx.svg_renderer()) {
            Ok(rendered) => rendered,
            Err(error) => {
                eprintln!("Toolbar icon decode failed: {error}");
                return image;
            }
        };
        let Some(pixels) = rendered.as_bytes(0) else {
            return image;
        };
        let size = rendered.size(0);
        let width = size.width.0;
        let height = size.height.0;
        let mut bitmap = vec![0u8; 122];
        bitmap[0..2].copy_from_slice(b"BM");
        bitmap[2..6].copy_from_slice(&(122u32 + pixels.len() as u32).to_le_bytes());
        bitmap[10..14].copy_from_slice(&122u32.to_le_bytes());
        bitmap[14..18].copy_from_slice(&108u32.to_le_bytes());
        bitmap[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        bitmap[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
        bitmap[26..28].copy_from_slice(&1u16.to_le_bytes());
        bitmap[28..30].copy_from_slice(&32u16.to_le_bytes());
        bitmap[30..34].copy_from_slice(&3u32.to_le_bytes());
        for (offset, mask) in [
            (54, 0x00ff0000u32),
            (58, 0x0000ff00),
            (62, 0x000000ff),
            (66, 0xff000000),
        ] {
            bitmap[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
        }
        let secondary = if view.dark { 0x4cc2ff } else { 0x0078d4 };
        for pixel in pixels.chunks_exact(4) {
            let old =
                (u32::from(pixel[2]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[0]);
            let matches_secondary = [0, 8, 16]
                .into_iter()
                .all(|shift| ((old >> shift) & 255).abs_diff((secondary >> shift) & 255) <= 3);
            if pixel[3] != 0 && (mono || matches_secondary) {
                bitmap.extend_from_slice(&[
                    color as u8,
                    (color >> 8) as u8,
                    (color >> 16) as u8,
                    pixel[3],
                ]);
            } else {
                bitmap.extend_from_slice(pixel);
            }
        }
        let colored = Arc::new(Image::from_bytes(ImageFormat::Bmp, bitmap));
        ICON_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let palette = (color, mono);
            if cache.0 != Some(palette) {
                cache.1.clear();
                cache.0 = Some(palette);
            }
            cache.1.insert(key, colored.clone());
        });
        colored
    }

    pub(super) fn image(self) -> Arc<Image> {
        match self {
            Self::StatusInfo => embedded_image!("standard/moreOnTooltip.svg"),
            Self::New => embedded_image!("standard/newFile.svg"),
            Self::Open => embedded_image!("standard/openFile.svg"),
            Self::Save => embedded_image!("standard/saveFile.svg"),
            Self::SaveAll => embedded_image!("standard/saveAll.svg"),
            Self::Close => embedded_image!("standard/closeFile.svg"),
            Self::CloseAll => embedded_image!("standard/closeAll.svg"),
            Self::Print => embedded_image!("standard/print.svg"),
            Self::Cut => embedded_image!("standard/cut.svg"),
            Self::Copy => embedded_image!("standard/copy.svg"),
            Self::Paste => embedded_image!("standard/paste.svg"),
            Self::Undo => embedded_image!("standard/undo.svg"),
            Self::Redo => embedded_image!("standard/redo.svg"),
            Self::Find => embedded_image!("standard/find.svg"),
            Self::Replace => embedded_image!("standard/findReplace.svg"),
            Self::DocumentList => embedded_image!("standard/docList.svg"),
            Self::ZoomIn => embedded_image!("standard/zoomIn.svg"),
            Self::ZoomOut => embedded_image!("standard/zoomOut.svg"),
            Self::WordWrap => embedded_image!("standard/wrap.svg"),
            Self::SyncVertical => embedded_image!("standard/syncV.svg"),
            Self::SyncHorizontal => embedded_image!("standard/syncH.svg"),
            Self::DefineUserLanguage => embedded_image!("standard/udl.svg"),
            Self::AllCharacters => embedded_image!("standard/allChars.svg"),
            Self::IndentGuides => embedded_image!("standard/indentGuide.svg"),
            Self::DocumentMap => embedded_image!("standard/docMap.svg"),
            Self::FolderWorkspace => embedded_image!("standard/fileBrowser.svg"),
            Self::FunctionList => embedded_image!("standard/funcList.svg"),
            Self::Monitoring => embedded_image!("standard/monitoring.svg"),
            Self::StartRecording => embedded_image!("standard/startRecord.svg"),
            Self::StopRecording => embedded_image!("standard/stopRecord.svg"),
            Self::Playback => embedded_image!("standard/playRecord.svg"),
            Self::RunMacroMultiple => embedded_image!("standard/playRecord_m.svg"),
            Self::SaveMacro => embedded_image!("standard/saveRecord.svg"),
            Self::MarkdownPreview => {
                static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
                IMAGE
                    .get_or_init(|| {
                        Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            include_bytes!("../../assets/markdown-preview.svg").to_vec(),
                        ))
                    })
                    .clone()
            }
            Self::SavedDocument => embedded_image!("saved.svg"),
            Self::ModifiedDocument => embedded_image!("unsaved.svg"),
            Self::SavedDocumentDark => embedded_image!("saved-dark.svg"),
            Self::ModifiedDocumentDark => embedded_image!("unsaved-dark.svg"),
            Self::ReadOnlyDocument => embedded_image!("readonly.svg"),
            Self::ReadOnlyDocumentDark => embedded_image!("readonly-dark.svg"),
            Self::SystemReadOnlyDocument => embedded_image!("readonly-system.svg"),
            Self::SystemReadOnlyDocumentDark => embedded_image!("readonly-system-dark.svg"),
            Self::MonitoringDocument => embedded_image!("monitoring-tab.svg"),
            Self::MonitoringDocumentDark => embedded_image!("monitoring-tab-dark.svg"),
            Self::PinTab => embedded_image!("tab-pin.svg"),
            Self::PinnedTab => embedded_image!("tab-pinned.svg"),
            Self::PinTabDark => embedded_image!("tab-pin-dark.svg"),
            Self::PinnedTabDark => embedded_image!("tab-pinned-dark.svg"),
            Self::CloseTab => embedded_image!("tab-close.svg"),
            Self::CloseTabHover => embedded_image!("tab-close-hover.svg"),
            Self::CloseTabOnTab => embedded_image!("tab-close-on-tab.svg"),
            Self::CloseTabPressed => embedded_image!("tab-close-pressed.svg"),
            Self::CloseTabDark => embedded_image!("tab-close-dark.svg"),
            Self::CloseTabHoverDark => embedded_image!("tab-close-hover-dark.svg"),
            Self::CloseTabOnTabDark => embedded_image!("tab-close-on-tab-dark.svg"),
            Self::CloseTabPressedDark => embedded_image!("tab-close-pressed-dark.svg"),
            Self::Application => {
                static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
                IMAGE.get_or_init(|| Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    include_bytes!(concat!(env!("OUT_DIR"), "/application-icon.png")).to_vec(),
                ))).clone()
            }
        }
    }
}

fn system_accent_color() -> u32 {
    unsafe {
        let color: *mut objc2::runtime::AnyObject =
            objc2::msg_send![objc2::class!(NSColor), controlAccentColor];
        let space: *mut objc2::runtime::AnyObject =
            objc2::msg_send![objc2::class!(NSColorSpace), sRGBColorSpace];
        let color: *mut objc2::runtime::AnyObject =
            objc2::msg_send![color, colorUsingColorSpace: space];
        if color.is_null() {
            return 0x0078d4;
        }
        let red: f64 = objc2::msg_send![color, redComponent];
        let green: f64 = objc2::msg_send![color, greenComponent];
        let blue: f64 = objc2::msg_send![color, blueComponent];
        ((red.clamp(0., 1.) * 255.).round() as u32) << 16
            | ((green.clamp(0., 1.) * 255.).round() as u32) << 8
            | (blue.clamp(0., 1.) * 255.).round() as u32
    }
}

thread_local! {
    static ICON_CACHE: std::cell::RefCell<(Option<(u32, bool)>, std::collections::BTreeMap<(u64, u32, bool), Arc<Image>>)> = std::cell::RefCell::new((None, std::collections::BTreeMap::new()));
}
