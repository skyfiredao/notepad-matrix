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
    SavedDocument,
    ModifiedDocument,
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
}

macro_rules! embedded_image {
    ($path:literal) => {{
        static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
        IMAGE
            .get_or_init(|| {
                Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    include_bytes!(concat!("../../assets/notepad-plus-plus/", $path)).to_vec(),
                ))
            })
            .clone()
    }};
}

impl ToolbarIcon {
    pub(super) fn image(self) -> Arc<Image> {
        match self {
            Self::New => embedded_image!("standard/newFile.png"),
            Self::Open => embedded_image!("standard/openFile.png"),
            Self::Save => embedded_image!("standard/saveFile.png"),
            Self::SaveAll => embedded_image!("standard/saveAll.png"),
            Self::Close => embedded_image!("standard/closeFile.png"),
            Self::CloseAll => embedded_image!("standard/closeAll.png"),
            Self::Cut => embedded_image!("standard/cut.png"),
            Self::Copy => embedded_image!("standard/copy.png"),
            Self::Paste => embedded_image!("standard/paste.png"),
            Self::Undo => embedded_image!("standard/undo.png"),
            Self::Redo => embedded_image!("standard/redo.png"),
            Self::Find => embedded_image!("standard/find.png"),
            Self::Replace => embedded_image!("standard/findReplace.png"),
            Self::DocumentList => embedded_image!("standard/docList.png"),
            Self::ZoomIn => embedded_image!("standard/zoomIn.png"),
            Self::ZoomOut => embedded_image!("standard/zoomOut.png"),
            Self::WordWrap => embedded_image!("standard/wrap.png"),
            Self::SyncVertical => embedded_image!("standard/syncV.png"),
            Self::SyncHorizontal => embedded_image!("standard/syncH.png"),
            Self::DefineUserLanguage => embedded_image!("standard/udl.png"),
            Self::AllCharacters => embedded_image!("standard/allChars.png"),
            Self::IndentGuides => embedded_image!("standard/indentGuide.png"),
            Self::DocumentMap => embedded_image!("standard/docMap.png"),
            Self::FolderWorkspace => embedded_image!("standard/fileBrowser.png"),
            Self::FunctionList => embedded_image!("standard/funcList.png"),
            Self::Monitoring => embedded_image!("standard/monitoring.png"),
            Self::StartRecording => embedded_image!("standard/startRecord.png"),
            Self::StopRecording => embedded_image!("standard/stopRecord.png"),
            Self::Playback => embedded_image!("standard/playRecord.png"),
            Self::RunMacroMultiple => embedded_image!("standard/playRecord_m.png"),
            Self::SaveMacro => embedded_image!("standard/saveRecord.png"),
            Self::SavedDocument => embedded_image!("saved.png"),
            Self::ModifiedDocument => embedded_image!("unsaved.png"),
            Self::PinTab => embedded_image!("tab-pin.png"),
            Self::PinnedTab => embedded_image!("tab-pinned.png"),
            Self::PinTabDark => embedded_image!("tab-pin-dark.png"),
            Self::PinnedTabDark => embedded_image!("tab-pinned-dark.png"),
            Self::CloseTab => embedded_image!("tab-close.png"),
            Self::CloseTabHover => embedded_image!("tab-close-hover.png"),
            Self::CloseTabOnTab => embedded_image!("tab-close-on-tab.png"),
            Self::CloseTabPressed => embedded_image!("tab-close-pressed.png"),
            Self::CloseTabDark => embedded_image!("tab-close-dark.png"),
            Self::CloseTabHoverDark => embedded_image!("tab-close-hover-dark.png"),
            Self::CloseTabOnTabDark => embedded_image!("tab-close-on-tab-dark.png"),
            Self::CloseTabPressedDark => embedded_image!("tab-close-pressed-dark.png"),
            Self::Application => embedded_image!("app16.png"),
        }
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn reference_standard_bitmap_artwork_decodes_at_original_size(cx: &mut gpui::TestAppContext) {
        let icons = [
            ToolbarIcon::New,
            ToolbarIcon::Open,
            ToolbarIcon::Save,
            ToolbarIcon::SaveAll,
            ToolbarIcon::Close,
            ToolbarIcon::CloseAll,
            ToolbarIcon::Cut,
            ToolbarIcon::Copy,
            ToolbarIcon::Paste,
            ToolbarIcon::Undo,
            ToolbarIcon::Redo,
            ToolbarIcon::Find,
            ToolbarIcon::Replace,
            ToolbarIcon::DocumentList,
            ToolbarIcon::ZoomIn,
            ToolbarIcon::ZoomOut,
            ToolbarIcon::WordWrap,
            ToolbarIcon::SyncVertical,
            ToolbarIcon::SyncHorizontal,
            ToolbarIcon::DefineUserLanguage,
            ToolbarIcon::AllCharacters,
            ToolbarIcon::IndentGuides,
            ToolbarIcon::DocumentMap,
            ToolbarIcon::FolderWorkspace,
            ToolbarIcon::FunctionList,
            ToolbarIcon::Monitoring,
            ToolbarIcon::StartRecording,
            ToolbarIcon::StopRecording,
            ToolbarIcon::Playback,
            ToolbarIcon::RunMacroMultiple,
            ToolbarIcon::SaveMacro,
            ToolbarIcon::SavedDocument,
            ToolbarIcon::ModifiedDocument,
            ToolbarIcon::PinTab,
            ToolbarIcon::PinnedTab,
            ToolbarIcon::PinTabDark,
            ToolbarIcon::PinnedTabDark,
            ToolbarIcon::Application,
        ];
        cx.update(|cx| {
            for icon in icons {
                let image = icon.image();
                assert_eq!(image.format, ImageFormat::Png);
                let decoded = image
                    .to_image_data(cx.svg_renderer())
                    .expect("original artwork must decode");
                assert_eq!(
                    decoded.size(0),
                    gpui::size(gpui::DevicePixels(16), gpui::DevicePixels(16))
                );
            }
        });
    }
}
