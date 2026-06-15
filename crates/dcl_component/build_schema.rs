//! Host-only (build-time) component-schema generator.
//!
//! Reflects over the proto FileDescriptorSet emitted by `build.rs` and walks each
//! `decentraland.sdk.components.PB*` message into a structural schema tree, written to
//! `component_schemas.json` in OUT_DIR. The curated semantic overlay (semantics/ranges/defaults/
//! placement/requires) now lives in the editor scene, applied there onto this raw schema. The
//! runtime just `include_str!`s the JSON — no prost-reflect / descriptor in the final (wasm) binary.

use std::collections::BTreeMap;

use prost_reflect::{DescriptorPool, FieldDescriptor, Kind, MessageDescriptor};
use serde_json::{json, Map, Value};

pub fn generate(descriptor_bytes: &[u8], out_path: &std::path::Path) {
    let pool = DescriptorPool::decode(descriptor_bytes).expect("decode descriptor");
    let mut schemas: BTreeMap<String, Value> = BTreeMap::new();

    for msg in pool.all_messages() {
        // top-level PB* messages in the sdk.components package
        if msg.parent_message().is_some() {
            continue;
        }
        if msg.package_name() != "decentraland.sdk.components" {
            continue;
        }
        let raw = msg.name();
        if !raw.starts_with("PB") {
            continue;
        }
        let name = raw.trim_start_matches("PB").to_string();
        // Helper sub-messages that are PB-prefixed but aren't standalone components
        // (no ecs_component_id; already inlined into their owning component's tree).
        if matches!(name.as_str(), "AnimationState") {
            continue;
        }

        let mut enums: BTreeMap<String, Value> = BTreeMap::new();
        let root = walk_message(&msg, &mut enums);
        let enums_val: Map<String, Value> = enums.into_iter().collect();

        // The structural schema only (placement "any" / no requires — the editor's curated overlay
        // supplies the real placement/requires + field semantics/ranges/defaults). Transform is not
        // a proto message, so it's omitted here and owned entirely by the editor scene.
        schemas.insert(
            name.clone(),
            json!({
                "name": name,
                "placement": "any",
                "readOnly": false,
                "requires": [],
                "root": root,
                "enums": Value::Object(enums_val),
            }),
        );
    }

    write_json(out_path, schemas);
}

fn write_json(path: &std::path::Path, map: BTreeMap<String, Value>) {
    let obj: Map<String, Value> = map.into_iter().collect();
    let out = serde_json::to_string(&Value::Object(obj)).expect("serialize schemas");
    std::fs::write(path, out).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// Walk a message into `{ "kind":"message", "fields":[ ... ] }`, accumulating referenced
/// enums into `enums`.
fn walk_message(msg: &MessageDescriptor, enums: &mut BTreeMap<String, Value>) -> Value {
    // real oneofs (skip proto3-optional synthetic ones, which protoc names "_field")
    let real_oneofs: Vec<_> = msg
        .oneofs()
        .filter(|o| !o.name().starts_with('_'))
        .collect();
    let oneof_field_names: std::collections::HashSet<String> = real_oneofs
        .iter()
        .flat_map(|o| o.fields().map(|f| f.name().to_string()))
        .collect();

    let mut fields: Vec<Value> = Vec::new();
    for f in msg.fields() {
        if oneof_field_names.contains(f.name()) {
            continue;
        }
        fields.push(field_node(&f, enums));
    }
    for o in &real_oneofs {
        let cases: Vec<Value> = o
            .fields()
            .map(|cf| json!({ "name": camel(cf.name()), "field": field_node(&cf, enums) }))
            .collect();
        fields.push(json!({ "name": camel(o.name()), "kind": "oneof", "cases": cases }));
    }

    json!({ "kind": "message", "fields": fields })
}

/// A single field node: `{ name, kind, semantic?, enum?, optional, default?, element? }`.
fn field_node(f: &FieldDescriptor, enums: &mut BTreeMap<String, Value>) -> Value {
    let name = camel(f.name());
    if f.is_list() {
        let element = element_node(f, enums);
        return json!({ "name": name, "kind": "repeated", "optional": false, "element": element });
    }
    let mut node = element_node(f, enums);
    // attach name + presence onto the (object) element node
    if let Value::Object(ref mut m) = node {
        m.insert("name".into(), json!(name));
        m.insert("optional".into(), json!(f.supports_presence()));
        // structural default for required scalar leaves only
        if !f.supports_presence() {
            if let Some(d) = scalar_zero(&f.kind()) {
                m.entry("default".to_string()).or_insert(d);
            }
        }
    }
    node
}

/// The kind+semantic of a field's element (the field itself, or a repeated element).
fn element_node(f: &FieldDescriptor, enums: &mut BTreeMap<String, Value>) -> Value {
    match f.kind() {
        Kind::Message(inner) => {
            if let Some(sem) = leaf_message_semantic(inner.full_name()) {
                json!({ "kind": "leaf", "semantic": sem })
            } else {
                walk_message(&inner, enums)
            }
        }
        Kind::Enum(ed) => {
            enums.entry(ed.name().to_string()).or_insert_with(|| {
                Value::Array(ed.values().map(|v| json!([v.name(), v.number()])).collect())
            });
            json!({ "kind": "leaf", "semantic": "enum", "enum": ed.name() })
        }
        k => json!({ "kind": "leaf", "semantic": scalar_semantic(&k) }),
    }
}

fn leaf_message_semantic(full_name: &str) -> Option<&'static str> {
    match full_name {
        "decentraland.common.Color3" => Some("color3"),
        "decentraland.common.Color4" => Some("color4"),
        "decentraland.common.Vector2" => Some("vector2"),
        "decentraland.common.Vector3" => Some("vector3"),
        "decentraland.common.Quaternion" => Some("quaternion"),
        // reusable composites with built-in editor renderers (don't recurse their internals).
        // NB: only flatten plain structs here — types containing a `oneof` (e.g. TextureUnion's
        // `tex`) must be emitted structurally so the schema exposes the oneof (its `$case` is
        // needed to round-trip through the composite).
        "decentraland.common.BorderRect" => Some("borderRect"),
        _ => None,
    }
}

fn scalar_semantic(k: &Kind) -> &'static str {
    match k {
        Kind::Double | Kind::Float => "number",
        Kind::Int32
        | Kind::Int64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Sfixed32
        | Kind::Sfixed64 => "int",
        Kind::Uint32 | Kind::Uint64 | Kind::Fixed32 | Kind::Fixed64 => "uint",
        Kind::Bool => "bool",
        Kind::String => "string",
        Kind::Bytes => "string",
        Kind::Message(_) | Kind::Enum(_) => "message",
    }
}

fn scalar_zero(k: &Kind) -> Option<Value> {
    match k {
        Kind::Double | Kind::Float => Some(json!(0)),
        Kind::Int32
        | Kind::Int64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Sfixed32
        | Kind::Sfixed64
        | Kind::Uint32
        | Kind::Uint64
        | Kind::Fixed32
        | Kind::Fixed64 => Some(json!(0)),
        Kind::Bool => Some(json!(false)),
        Kind::String => Some(json!("")),
        _ => None,
    }
}

/// snake_case -> lowerCamelCase (matches the serde rename_all the CRDT JSON uses).
fn camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut up = false;
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else if i == 0 {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
