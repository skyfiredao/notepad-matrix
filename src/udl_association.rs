use crate::{
    syntax::Language,
    udl_registry::UdlRegistry,
    workspace::{Buffer, BufferId},
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Refresh {
    pub changed: bool,
    pub missing: Vec<(BufferId, u64)>,
}

pub fn refresh<'a>(
    registry: &UdlRegistry,
    buffers: impl IntoIterator<Item = &'a mut Buffer>,
    dark: bool,
) -> Refresh {
    let mut result = Refresh::default();
    for buffer in buffers {
        if let Language::UserDefined(id) = buffer.language()
            && registry.definition(id).is_none()
        {
            result.missing.push((buffer.id(), id));
            continue;
        }
        if buffer.language_is_explicit() {
            continue;
        }
        let Some(file) = &buffer.disk_file else {
            continue;
        };
        let detected = registry
            .match_path_for_theme(file.path(), dark)
            .map(Language::UserDefined)
            .unwrap_or_else(|| Language::from_path(file.path()));
        if detected != buffer.language() {
            buffer.set_detected_language(detected);
            result.changed = true;
        }
    }
    result
}

pub fn removed<'a>(buffers: impl IntoIterator<Item = &'a mut Buffer>, id: u64) {
    for buffer in buffers {
        if buffer.language() == Language::UserDefined(id) {
            buffer.set_language(Language::UserDefined(0));
        }
    }
}
