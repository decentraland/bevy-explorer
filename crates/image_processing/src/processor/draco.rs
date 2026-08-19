// Decompresses KHR_draco_mesh_compression geometry in place: decoded attribute
// and index data is appended to the binary chunk as new buffer views, the
// primitives' accessors are pointed at them, and the extension is stripped, so
// the result is a plain gltf the engine can load.

use draco_oxide_core::{attribute::Attribute, mesh::Mesh};
use gltf_json::{
    accessor::{ComponentType, GenericComponentType, Type},
    validation::Checked,
    Index,
};

const DRACO_EXTENSION: &str = "KHR_draco_mesh_compression";

#[derive(Debug)]
#[allow(dead_code)] // we use the ignored debug impl
pub enum DracoError {
    Decode(draco_oxide_decoder::Err),
    Malformed(&'static str),
    Unsupported(&'static str),
}

struct PrimitiveWork {
    view: usize,
    // (draco attribute unique id, accessor index)
    attributes: Vec<(usize, usize)>,
    indices: Option<usize>,
}

/// Decodes every draco-compressed primitive, rewriting `root` and appending the
/// decoded data to `bin`. Returns the indices of the now-dead compressed buffer
/// views (for the caller to drop when it rebuilds the binary chunk).
pub fn decompress(root: &mut gltf_json::Root, bin: &mut Vec<u8>) -> Result<Vec<usize>, DracoError> {
    // gather work first: decoding mutates accessors/views/bin, which can't be
    // borrowed while iterating primitives
    let mut work = Vec::new();
    for mesh in root.meshes.iter_mut() {
        for primitive in mesh.primitives.iter_mut() {
            let Some(ext) = primitive
                .extensions
                .as_mut()
                .and_then(|e| e.others.remove(DRACO_EXTENSION))
            else {
                continue;
            };

            let view =
                ext.get("bufferView")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or(DracoError::Malformed("extension bufferView"))? as usize;
            let ext_attributes = ext
                .get("attributes")
                .and_then(serde_json::Value::as_object)
                .ok_or(DracoError::Malformed("extension attributes"))?;

            let mut attributes = Vec::new();
            for (semantic, unique_id) in ext_attributes {
                let unique_id = unique_id
                    .as_u64()
                    .ok_or(DracoError::Malformed("attribute id"))?
                    as usize;
                let accessor = primitive
                    .attributes
                    .iter()
                    .find(|(k, _)| k.to_string() == *semantic)
                    .map(|(_, v)| v.value())
                    .ok_or(DracoError::Malformed(
                        "extension attribute not in primitive",
                    ))?;
                attributes.push((unique_id, accessor));
            }

            work.push(PrimitiveWork {
                view,
                attributes,
                indices: primitive.indices.map(|i| i.value()),
            });
        }
    }

    let mut dead_views = Vec::new();
    for prim in work {
        let view = root
            .buffer_views
            .get(prim.view)
            .ok_or(DracoError::Malformed("bufferView out of range"))?;
        let start = view.byte_offset.unwrap_or_default().0 as usize;
        let len = view.byte_length.0 as usize;
        let compressed = bin
            .get(start..start + len)
            .ok_or(DracoError::Malformed("bufferView out of binary range"))?
            .to_vec();

        let mesh = draco_oxide_decoder::decode_mesh(&compressed).map_err(DracoError::Decode)?;

        if let Some(acc_idx) = prim.indices {
            write_indices(root, bin, &mesh, acc_idx)?;
        }

        for (unique_id, acc_idx) in prim.attributes {
            let attribute = mesh
                .get_attributes()
                .iter()
                .find(|a| a.get_id().as_usize() == unique_id)
                .ok_or(DracoError::Malformed("attribute id not in draco stream"))?;
            write_attribute(root, bin, attribute, acc_idx)?;
        }

        dead_views.push(prim.view);
    }

    if !dead_views.is_empty() {
        root.extensions_used.retain(|e| e != DRACO_EXTENSION);
        root.extensions_required.retain(|e| e != DRACO_EXTENSION);
    }

    dead_views.sort_unstable();
    dead_views.dedup();
    Ok(dead_views)
}

fn write_indices(
    root: &mut gltf_json::Root,
    bin: &mut Vec<u8>,
    mesh: &Mesh,
    acc_idx: usize,
) -> Result<(), DracoError> {
    let faces = mesh.get_faces();
    let accessor = root
        .accessors
        .get(acc_idx)
        .ok_or(DracoError::Malformed("indices accessor out of range"))?;

    // keep the declared index width where the point count still fits, else widen
    let max = faces
        .iter()
        .flatten()
        .map(|p| usize::from(*p))
        .max()
        .unwrap_or(0);
    let declared = match accessor.component_type {
        Checked::Valid(GenericComponentType(ct)) => ct,
        Checked::Invalid => ComponentType::U32,
    };
    let component_type = match declared {
        ComponentType::U8 if max <= u8::MAX as usize => ComponentType::U8,
        ComponentType::U16 if max <= u16::MAX as usize => ComponentType::U16,
        _ => ComponentType::U32,
    };

    let mut data = Vec::with_capacity(faces.len() * 3 * component_type.size());
    for index in faces.iter().flatten() {
        let index = usize::from(*index);
        match component_type {
            ComponentType::U8 => data.push(index as u8),
            ComponentType::U16 => data.extend_from_slice(&(index as u16).to_le_bytes()),
            _ => data.extend_from_slice(&(index as u32).to_le_bytes()),
        }
    }

    let view = push_view(root, bin, &data);
    let accessor = &mut root.accessors[acc_idx];
    accessor.buffer_view = Some(view);
    accessor.byte_offset = None;
    accessor.count = (faces.len() * 3).into();
    accessor.component_type = Checked::Valid(GenericComponentType(component_type));
    Ok(())
}

fn write_attribute(
    root: &mut gltf_json::Root,
    bin: &mut Vec<u8>,
    attribute: &Attribute,
    acc_idx: usize,
) -> Result<(), DracoError> {
    use draco_oxide_core::attribute::ComponentDataType;

    let component_type = match attribute.get_component_type() {
        ComponentDataType::F32 => ComponentType::F32,
        ComponentDataType::U8 => ComponentType::U8,
        ComponentDataType::U16 => ComponentType::U16,
        ComponentDataType::U32 => ComponentType::U32,
        ComponentDataType::I8 => ComponentType::I8,
        ComponentDataType::I16 => ComponentType::I16,
        _ => return Err(DracoError::Unsupported("component type")),
    };

    let accessor = root
        .accessors
        .get(acc_idx)
        .ok_or(DracoError::Malformed("attribute accessor out of range"))?;
    let multiplicity = match accessor.type_ {
        Checked::Valid(Type::Scalar) => 1,
        Checked::Valid(Type::Vec2) => 2,
        Checked::Valid(Type::Vec3) => 3,
        Checked::Valid(Type::Vec4) => 4,
        Checked::Valid(Type::Mat2) => 4,
        Checked::Valid(Type::Mat3) => 9,
        Checked::Valid(Type::Mat4) => 16,
        Checked::Invalid => return Err(DracoError::Malformed("accessor type")),
    };
    if attribute.get_num_components() != multiplicity {
        return Err(DracoError::Unsupported(
            "attribute component count mismatch",
        ));
    }

    // flatten the unique-value buffer out to per-point values
    let element_size = component_type.size() * multiplicity;
    let values = attribute.get_data_as_bytes();
    let data = match attribute.point_map_as_slice() {
        None => values.to_vec(),
        Some(map) => {
            let mut data = Vec::with_capacity(map.len() * element_size);
            for value_idx in map {
                let offset = usize::from(*value_idx) * element_size;
                data.extend_from_slice(&values[offset..offset + element_size]);
            }
            data
        }
    };

    let view = push_view(root, bin, &data);
    let accessor = &mut root.accessors[acc_idx];
    accessor.buffer_view = Some(view);
    accessor.byte_offset = None;
    accessor.count = attribute.len().into();
    accessor.component_type = Checked::Valid(GenericComponentType(component_type));
    Ok(())
}

fn push_view(
    root: &mut gltf_json::Root,
    bin: &mut Vec<u8>,
    data: &[u8],
) -> Index<gltf_json::buffer::View> {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let offset = bin.len() as u64;
    bin.extend_from_slice(data);
    let index = Index::new(root.buffer_views.len() as u32);
    root.buffer_views.push(gltf_json::buffer::View {
        buffer: gltf_json::Index::new(0),
        byte_length: data.len().into(),
        byte_offset: Some(offset.into()),
        byte_stride: None,
        name: Some("Draco_Decoded".into()),
        target: None,
        extensions: None,
        extras: Default::default(),
    });
    index
}
