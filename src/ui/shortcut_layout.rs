use std::ffi::c_void;
use std::ptr::NonNull;

type CFRef = *const c_void;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> CFRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFRef;
    fn TISGetInputSourceProperty(source: CFRef, property: CFRef) -> CFRef;
    static kTISPropertyUnicodeKeyLayoutData: CFRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(layout: CFRef, code: u16, action: u16, modifiers: u32, keyboard_type: u32, options: u32, dead_key: *mut u32, capacity: usize, length: *mut usize, text: *mut u16) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CFRef);
    fn CFDataGetBytePtr(data: CFRef) -> *const u8;
    fn CFDataGetLength(data: CFRef) -> isize;
}

struct InputSource(NonNull<c_void>);

impl InputSource {
    fn copied(source: CFRef) -> Option<Self> { NonNull::new(source.cast_mut()).map(Self) }

    fn layout(&self) -> Option<CFRef> {
        let data = unsafe { TISGetInputSourceProperty(self.0.as_ptr(), kTISPropertyUnicodeKeyLayoutData) };
        if data.is_null() || unsafe { CFDataGetLength(data) } <= 0 { return None; }
        let bytes = unsafe { CFDataGetBytePtr(data) };
        if bytes.is_null() { None } else { Some(bytes.cast()) }
    }
}

impl Drop for InputSource {
    fn drop(&mut self) { unsafe { CFRelease(self.0.as_ptr()); } }
}

pub(crate) struct Snapshot {
    _current: Option<InputSource>,
    _underlying: Option<InputSource>,
    layout: Option<CFRef>,
    keyboard_type: u32,
}

impl Snapshot {
    pub(crate) fn new() -> Self {
        let current = InputSource::copied(unsafe { TISCopyCurrentKeyboardInputSource() });
        let layout = current.as_ref().and_then(InputSource::layout);
        let underlying = if layout.is_none() {
            InputSource::copied(unsafe { TISCopyCurrentKeyboardLayoutInputSource() })
        } else { None };
        let layout = layout.or_else(|| underlying.as_ref().and_then(InputSource::layout));
        Self { _current: current, _underlying: underlying, layout, keyboard_type: u32::from(unsafe { LMGetKbdType() }) }
    }

    fn character(&self, code: u16) -> Option<String> {
        let layout = self.layout?;
        let mut text = [0u16; 32];
        let mut length = 0;
        let mut dead_key = 0;
        let status = unsafe { UCKeyTranslate(layout, code, 3, 0, self.keyboard_type, 1, &mut dead_key, text.len(), &mut length, text.as_mut_ptr()) };
        if status != 0 || length == 0 || length > text.len() { return None; }
        let name = String::from_utf16(&text[..length]).ok()?;
        if name.chars().any(char::is_control) { None } else { Some(name) }
    }

    pub(crate) fn key_name(&self, key: u8) -> Option<String> {
        let code = hardware_code(key)?;
        if matches!(key, 96..=105) { return Some(format!("Numpad {}", key - 96)); }
        let name = self.character(code).unwrap_or_else(|| format!("Key {key}"));
        Some(if matches!(key, 106 | 107 | 109..=111) { format!("Num {name}") } else { name })
    }
}

pub(crate) fn hardware_code(key: u8) -> Option<u16> {
    Some(match key {
        96 => 82, 97 => 83, 98 => 84, 99 => 85, 100 => 86, 101 => 87, 102 => 88, 103 => 89, 104 => 91, 105 => 92,
        106 => 67, 107 => 69, 109 => 78, 110 => 65, 111 => 75,
        186 => 41, 187 => 24, 188 => 43, 189 => 27, 190 => 47, 191 => 44, 192 => 50,
        219 => 33, 220 => 42, 221 => 30, 222 => 39, 226 => 10,
        _ => return None,
    })
}

pub(crate) fn legacy_key(code: u16) -> Option<u8> {
    crate::shortcuts::key_choices().into_iter().find(|key| hardware_code(*key) == Some(code))
}

pub(crate) fn key_name(key: u8) -> Option<String> {
    hardware_code(key)?;
    Snapshot::new().key_name(key)
}
