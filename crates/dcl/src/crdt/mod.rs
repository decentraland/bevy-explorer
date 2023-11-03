use dcl_component::{DclWriter, SceneComponentId, SceneCrdtTimestamp, SceneEntityId};

use super::interface::CrdtMessageType;

pub mod growonly;
pub mod lww;

// helpers to make message byte streams
pub fn put_component(
    entity_id: &SceneEntityId,
    component_id: &SceneComponentId,
    timestamp: &SceneCrdtTimestamp,
    maybe_entry: Option<&[u8]>,
) -> Vec<u8> {
    let content_len = maybe_entry.map(|entry| entry.len()).unwrap_or(0);
    let length = content_len + 12 + if maybe_entry.is_some() { 4 } else { 0 } + 8;

    let mut buf = Vec::with_capacity(length);
    let mut writer = DclWriter::new(&mut buf);
    writer.write_u32(length as u32);

    if maybe_entry.is_some() {
        writer.write(&CrdtMessageType::PutComponent);
    } else {
        writer.write(&CrdtMessageType::DeleteComponent);
    }

    writer.write(entity_id);
    writer.write(component_id);
    writer.write(timestamp);

    if let Some(entry) = maybe_entry {
        writer.write_u32(content_len as u32);
        writer.write_raw(entry)
    }

    buf
}

pub fn append_component(
    entity_id: &SceneEntityId,
    component_id: &SceneComponentId,
    entry: &[u8],
) -> Vec<u8> {
    let content_len = entry.len();
    let length = content_len + 12 + 4 + 8;

    let mut buf = Vec::with_capacity(length);
    let mut writer = DclWriter::new(&mut buf);
    writer.write_u32(length as u32);
    writer.write(&CrdtMessageType::AppendValue);

    writer.write(entity_id);
    writer.write(component_id);
    writer.write(&SceneCrdtTimestamp(0));

    writer.write_u32(content_len as u32);
    writer.write_raw(entry);

    buf
}

pub fn delete_entity(entity_id: &SceneEntityId) -> Vec<u8> {
    let mut buf = Vec::with_capacity(12);
    let mut writer = DclWriter::new(&mut buf);

    writer.write_u32(12);
    writer.write(&CrdtMessageType::DeleteEntity);
    writer.write(entity_id);

    buf
}
