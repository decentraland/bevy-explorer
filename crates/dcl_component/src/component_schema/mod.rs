//! Runtime access to the component schema.
//!
//! The structural schema is generated at BUILD time (`build_schema.rs` reflects over the proto
//! descriptor) and embedded here as a static JSON string. Nothing reflective ships in the binary —
//! on wasm this is just a `&'static str`. The curated semantic overlay (semantics/ranges/defaults/
//! placement/requires) is applied by the editor scene. See `CATALOG.md` / `DESIGN.md` for the format.

use serde_json::Value;

/// The full `{ componentName: schema, … }` JSON for every editable component.
pub fn all_schemas_json() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/component_schemas.json"))
}

/// The schema JSON for one component, if present.
pub fn schema_for(name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(all_schemas_json()).ok()?;
    v.get(name).map(|s| s.to_string())
}

/// The names of every component that has a schema.
pub fn schema_names() -> Vec<String> {
    serde_json::from_str::<Value>(all_schemas_json())
        .ok()
        .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
        .unwrap_or_default()
}
