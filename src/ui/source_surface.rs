use super::*;

impl MatrixWindow {
    pub(super) fn execute_extended_source(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.select_source_encoding(id, window, cx)
            || self.execute_view_source(id, window, cx)
            || self.execute_help_source(id, window, cx)
        { return true; }
        if (48501..=48512).contains(&id) {
            let algorithm = match id {
                48501..=48503 => crate::hashes::HashAlgorithm::Md5,
                48504..=48506 => crate::hashes::HashAlgorithm::Sha256,
                48507..=48509 => crate::hashes::HashAlgorithm::Sha1,
                _ => crate::hashes::HashAlgorithm::Sha512,
            };
            match (id - 48501) % 3 {
                0 => self.show_hash_text(algorithm, window, cx),
                1 => self.show_hash_files(algorithm, window, cx),
                _ => self.copy_selection_hash(algorithm, window, cx),
            }
            return true;
        }
        match id {
            11001 => self.show_window_manager(window, cx),
            59003 => self.show_converter_panel(window, cx),
            59005 => self.edit_converter_configuration(window, cx),
            59006 => self.show_converter_about(window, cx),
            _ => return false,
        }
        true
    }

    pub(super) fn menu_source_checked(&self, id: u32) -> Option<bool> {
        self.source_encoding_checked(id).or_else(|| self.view_source_checked(id)).or_else(|| {
            (id == 59003).then(|| self.converter_panel_visible())
        })
    }

    pub(super) fn menu_source_enabled(&self, id: u32) -> Option<bool> {
        self.source_encoding_enabled(id).or_else(|| self.view_source_enabled(id)).or_else(|| {
            matches!(id, 59003 | 59005 | 59006).then_some(self.converter_enabled)
        })
    }
}
