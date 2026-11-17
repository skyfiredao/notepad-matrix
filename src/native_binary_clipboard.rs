use std::io;

use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

const CONTENT: &str = "com.notepad-matrix.binary-content";
const LENGTH: &str = "com.notepad-matrix.binary-content-length";
const TEXT: &str = "public.utf8-plain-text";

pub(crate) struct Payload {
    pub(crate) bytes: Vec<u8>,
    pub(crate) recorded_length: bool,
}

pub(crate) fn write(board: &NSPasteboard, bytes: &[u8]) -> io::Result<()> {
    let length = bytes.len() as u32;
    let item = NSPasteboardItem::new();
    let mut terminated = bytes.to_vec();
    terminated.push(0);
    if !item.setData_forType(
        &NSData::with_bytes(&terminated),
        &NSString::from_str(CONTENT),
    ) || !item.setData_forType(
        &NSData::with_bytes(&length.to_le_bytes()),
        &NSString::from_str(LENGTH),
    ) {
        return Err(io::Error::other(
            "binary clipboard item could not be prepared",
        ));
    }
    if let Ok(text) = std::str::from_utf8(bytes)
        && !item.setString_forType(&NSString::from_str(text), &NSString::from_str(TEXT))
    {
        return Err(io::Error::other("clipboard text could not be prepared"));
    }
    board.clearContents();
    let item: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*item);
    if !board.writeObjects(&NSArray::from_slice(&[item])) {
        return Err(io::Error::other("binary clipboard could not be published"));
    }
    Ok(())
}

pub(crate) fn read(board: &NSPasteboard) -> Option<Payload> {
    let content = NSString::from_str(CONTENT);
    let text = NSString::from_str(TEXT);
    let items = board.pasteboardItems()?;
    let item = items
        .iter()
        .find(|item| item.dataForType(&content).is_some())
        .or_else(|| items.iter().find(|item| item.dataForType(&text).is_some()))?;
    let data = item
        .dataForType(&content)
        .or_else(|| item.dataForType(&text))?;
    let mut bytes = data.to_vec();
    let length = item.dataForType(&NSString::from_str(LENGTH));
    let recorded_length = length.is_some();
    if let Some(length) = length {
        let length = length.to_vec();
        let length = u32::from_le_bytes(length.get(..4)?.try_into().ok()?) as usize;
        bytes.truncate(length);
    } else if let Some(end) = bytes.iter().position(|byte| *byte == 0) {
        bytes.truncate(end);
    }
    Some(Payload {
        bytes,
        recorded_length,
    })
}

