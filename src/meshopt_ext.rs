use bevy::{
    app::{App, Plugin},
    asset::{
        io::{Reader, VecReader},
        AssetApp, AssetLoader, LoadContext,
    },
    gltf::{Gltf, GltfLoader, GltfLoaderSettings},
    image::CompressedImageFormats,
    render::renderer::RenderDevice,
};

/// Support for the `EXT_meshopt_compression` gltf extension (meshoptimizer-encoded
/// vertex/index streams, as produced by gltfpack).
///
/// bevy's gltf loader has no hook for buffer-view compression, so the stock loader is
/// wrapped: compressed buffer views are decompressed up front, the mesh accessors that
/// bevy cannot read post-decode (quantized positions/normals/etc from the companion
/// KHR_mesh_quantization extension) are rewritten to plain formats, and the resulting
/// ordinary gltf is handed to the inner `GltfLoader`. Files that don't use the
/// extension pass through byte-identical.
pub struct MeshoptPlugin;

impl Plugin for MeshoptPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        // mirrors GltfPlugin::finish; registering later makes this the default
        // loader for gltf/glb
        let supported_compressed_formats = match app.world().get_resource::<RenderDevice>() {
            Some(render_device) => CompressedImageFormats::from_features(render_device.features()),
            None => CompressedImageFormats::NONE,
        };
        app.register_asset_loader(MeshoptGltfLoader {
            inner: GltfLoader {
                supported_compressed_formats,
                custom_vertex_attributes: Default::default(),
            },
        });
    }
}

pub struct MeshoptGltfLoader {
    inner: GltfLoader,
}

impl AssetLoader for MeshoptGltfLoader {
    type Asset = Gltf;
    type Settings = GltfLoaderSettings;
    type Error = anyhow::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &GltfLoaderSettings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Gltf, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if let Some(rewritten) = imp::decompress_gltf(&bytes).map_err(|e| {
            anyhow::anyhow!(
                "meshopt: failed to decompress {:?}: {e}",
                load_context.path()
            )
        })? {
            bytes = rewritten;
        }
        Ok(self
            .inner
            .load(&mut VecReader::new(bytes), settings, load_context)
            .await?)
    }

    fn extensions(&self) -> &[&str] {
        &["gltf", "glb"]
    }
}

mod imp {
    use std::borrow::Cow;
    use std::collections::HashSet;

    use base64::Engine;
    use gltf::accessor::{DataType, Dimensions};
    use gltf::{Accessor, Document, Semantic};
    use serde_json::{json, Value};

    const EXT: &str = "EXT_meshopt_compression";
    const QUANTIZATION_EXT: &str = "KHR_mesh_quantization";

    /// Returns `Ok(None)` when the file does not use `EXT_meshopt_compression` (it must
    /// reach the stock loader untouched), `Ok(Some(bytes))` with a rewritten,
    /// extension-free gltf when it does, and `Err` when compressed content is malformed.
    pub(super) fn decompress_gltf(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let Some(container) = Container::parse(bytes) else {
            return Ok(None);
        };
        if !contains(container.json, EXT.as_bytes()) {
            return Ok(None);
        }
        // unparseable files keep their stock loader error
        let Ok(file) = gltf::Gltf::from_slice_without_validation(bytes) else {
            return Ok(None);
        };
        check_walk_refs(file.document.as_json())?;
        check_validation(file.document.as_json())?;
        transform(&container, file).map(Some)
    }

    // the transform walk hits panicking unwraps in the gltf crate on dangling
    // cross-references (and the crate's own validator indexes the POSITION accessor
    // directly), so these must be range-checked before anything walks the document
    fn check_walk_refs(root: &gltf::json::Root) -> Result<(), String> {
        for view in &root.buffer_views {
            if view.buffer.value() >= root.buffers.len() {
                return Err(format!("bufferView buffer {} out of range", view.buffer.value()));
            }
        }
        for accessor in &root.accessors {
            if accessor
                .buffer_view
                .is_some_and(|v| v.value() >= root.buffer_views.len())
            {
                return Err("accessor bufferView out of range".into());
            }
        }
        for mesh in &root.meshes {
            for prim in &mesh.primitives {
                for index in prim.attributes.values().copied().chain(prim.indices) {
                    if index.value() >= root.accessors.len() {
                        return Err(format!("primitive accessor {} out of range", index.value()));
                    }
                }
            }
        }
        Ok(())
    }

    // hostile input must never reach the gltf crate's unwraps, so anything the stock
    // loader would reject is rejected here with the validator's own message; tolerated
    // are exactly the extensions this module decodes plus the zero-filled
    // accessor-without-bufferView pattern the stock loader retries without validation
    fn check_validation(root: &gltf::json::Root) -> Result<(), String> {
        use gltf::json::validation::Validate;
        let mut errors = Vec::new();
        root.validate(root, gltf::json::Path::new, &mut |path, error| {
            errors.push((path(), error));
        });
        errors.retain(|(path, error)| !tolerated(path.as_str(), *error));
        match errors.first() {
            Some((path, error)) => Err(format!("validation: {error} at {path}")),
            None => Ok(()),
        }
    }

    fn tolerated(path: &str, error: gltf::json::validation::Error) -> bool {
        use gltf::json::validation::Error;
        match error {
            Error::Unsupported => {
                path.starts_with("extensionsRequired")
                    && [EXT, QUANTIZATION_EXT]
                        .iter()
                        .any(|ext| path.ends_with(&format!("= \"{ext}\"")))
            }
            Error::Missing => path.starts_with("accessors[") && path.ends_with(".bufferView"),
            _ => false,
        }
    }

    struct Container<'a> {
        json: &'a [u8],
        is_glb: bool,
    }

    impl<'a> Container<'a> {
        fn parse(bytes: &'a [u8]) -> Option<Self> {
            if bytes.len() >= 12 && &bytes[0..4] == b"glTF" {
                let mut offset = 12usize;
                while offset + 8 <= bytes.len() {
                    let len =
                        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
                    let ty = &bytes[offset + 4..offset + 8];
                    let start = offset + 8;
                    let end = start.checked_add(len)?;
                    if end > bytes.len() {
                        return None;
                    }
                    if ty == b"JSON" {
                        return Some(Container {
                            json: &bytes[start..end],
                            is_glb: true,
                        });
                    }
                    offset = end;
                }
                None
            } else if bytes
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| *b == b'{')
            {
                Some(Container {
                    json: bytes,
                    is_glb: false,
                })
            } else {
                None
            }
        }
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    fn transform(container: &Container, file: gltf::Gltf) -> Result<Vec<u8>, String> {
        let document = file.document;
        let mut blob = file.blob;
        let buffers = read_buffers(&document, blob.as_deref());
        let mut budget = MAX_DECODE_TOTAL;

        let n_views = document.views().count();
        let mut decoded: Vec<Option<DecodedView>> = Vec::with_capacity(n_views);
        decoded.resize_with(n_views, || None);
        for view in document.views() {
            let Some(ext) = view.extension_value(EXT) else {
                continue;
            };
            let f = parse_ext(ext)?;
            let buf = buffers
                .get(f.buffer)
                .and_then(|b| b.as_ref())
                .ok_or_else(|| format!("compressed stream in unavailable buffer {}", f.buffer))?;
            let end = f.offset.checked_add(f.length).ok_or("ext range overflow")?;
            let src = buf
                .get(f.offset..end)
                .ok_or("ext byte range out of range")?;
            let dv = match f.mode.as_str() {
                "ATTRIBUTES" => DecodedView {
                    bytes: decode_vertex_stream(&f, src, &mut budget)?,
                    stride: f.stride,
                    attributes: true,
                },
                "TRIANGLES" => DecodedView {
                    bytes: decode_index_stream(&f, src, &mut budget)?,
                    stride: f.stride,
                    attributes: false,
                },
                other => return Err(format!("unsupported compression mode {other}")),
            };
            decoded[view.index()] = Some(dv);
        }

        // rewrite the attributes of every compressed primitive to formats the loader
        // reads natively; leftover accessors (animation samplers, inverse bind
        // matrices, morph deltas) keep their layout and just point at decoded bytes
        let mut planned: Vec<Planned> = Vec::new();
        let mut planned_set: HashSet<usize> = HashSet::new();
        for mesh in document.meshes() {
            for prim in mesh.primitives() {
                let compressed = prim
                    .attributes()
                    .map(|(_, a)| a)
                    .chain(prim.indices())
                    .any(|a| a.view().is_some_and(|v| decoded[v.index()].is_some()));
                if !compressed {
                    continue;
                }
                let mut n_verts: Option<usize> = None;
                for (semantic, accessor) in prim.attributes() {
                    n_verts = Some(n_verts.unwrap_or(usize::MAX).min(accessor.count()));
                    if planned_set.contains(&accessor.index()) {
                        continue;
                    }
                    let Some(target) = target_for(&semantic) else {
                        continue;
                    };
                    planned.push(plan_accessor(target, &accessor, &decoded, &buffers)?);
                    planned_set.insert(accessor.index());
                }
                // reject out-of-range decoded indices before the gpu sees them
                if let (Some(n_verts), Some(accessor)) = (n_verts, prim.indices()) {
                    check_indices(&accessor, &decoded, n_verts)?;
                }
            }
        }
        drop(buffers);

        let mut root: Value = serde_json::from_slice(container.json)
            .map_err(|e| format!("invalid gltf json: {e}"))?;

        // sink for decompressed bytes: extend the glb bin chunk when there is one,
        // otherwise add a data-uri buffer
        let bin_buffer = document
            .buffers()
            .find(|b| matches!(b.source(), gltf::buffer::Source::Bin))
            .map(|b| b.index());
        let n_buffers = document.buffers().count();
        let sink_is_blob = bin_buffer.is_some() && blob.is_some();
        let (sink_buffer, mut sink_bytes) = if sink_is_blob {
            (bin_buffer.unwrap(), blob.take().unwrap())
        } else {
            (n_buffers, Vec::new())
        };

        {
            let views = root
                .get_mut("bufferViews")
                .and_then(Value::as_array_mut)
                .ok_or("missing bufferViews")?;
            for (idx, dv) in decoded.iter().enumerate() {
                let Some(dv) = dv else { continue };
                let offset = append_aligned(&mut sink_bytes, &dv.bytes);
                let view = views
                    .get_mut(idx)
                    .and_then(Value::as_object_mut)
                    .ok_or("bufferView index out of range")?;
                view.insert("buffer".to_owned(), json!(sink_buffer));
                view.insert("byteOffset".to_owned(), json!(offset));
                view.insert("byteLength".to_owned(), json!(dv.bytes.len()));
                // byteStride is capped at 252 by the gltf spec; a wider stream can only
                // be tightly packed, which an omitted stride also means
                if dv.attributes && dv.stride <= 252 {
                    view.insert("byteStride".to_owned(), json!(dv.stride));
                } else {
                    view.remove("byteStride");
                }
                let empty = view
                    .get_mut("extensions")
                    .and_then(Value::as_object_mut)
                    .map(|exts| {
                        exts.remove(EXT);
                        exts.is_empty()
                    });
                if empty == Some(true) {
                    view.remove("extensions");
                }
            }
        }

        for plan in &planned {
            let offset = append_aligned(&mut sink_bytes, &plan.bytes);
            let views = root
                .get_mut("bufferViews")
                .and_then(Value::as_array_mut)
                .ok_or("missing bufferViews")?;
            let view_index = views.len();
            views.push(json!({
                "buffer": sink_buffer,
                "byteOffset": offset,
                "byteLength": plan.bytes.len(),
            }));
            let accessor = root
                .get_mut("accessors")
                .and_then(Value::as_array_mut)
                .and_then(|a| a.get_mut(plan.accessor))
                .and_then(Value::as_object_mut)
                .ok_or("accessor index out of range")?;
            accessor.insert("bufferView".to_owned(), json!(view_index));
            accessor.insert("byteOffset".to_owned(), json!(0));
            accessor.insert("componentType".to_owned(), json!(plan.component_type));
            accessor.insert("type".to_owned(), json!(plan.ty));
            accessor.remove("normalized");
            match &plan.min_max {
                // the spec requires min/max on POSITION accessors; the old values were
                // in quantized space
                Some((min, max)) => {
                    accessor.insert("min".to_owned(), json!(min));
                    accessor.insert("max".to_owned(), json!(max));
                }
                None => {
                    accessor.remove("min");
                    accessor.remove("max");
                }
            }
        }

        {
            let buffers_json = root
                .get_mut("buffers")
                .and_then(Value::as_array_mut)
                .ok_or("missing buffers")?;
            if sink_is_blob {
                let buffer = buffers_json
                    .get_mut(sink_buffer)
                    .and_then(Value::as_object_mut)
                    .ok_or("buffer index out of range")?;
                buffer.insert("byteLength".to_owned(), json!(sink_bytes.len()));
            } else if !sink_bytes.is_empty() {
                buffers_json.push(json!({
                    "byteLength": sink_bytes.len(),
                    "uri": format!(
                        "data:application/octet-stream;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&sink_bytes)
                    ),
                }));
            }
        }

        {
            let referenced: HashSet<u64> = root
                .get("bufferViews")
                .and_then(Value::as_array)
                .map(|views| {
                    views
                        .iter()
                        .filter_map(|v| v.get("buffer").and_then(Value::as_u64))
                        .collect()
                })
                .unwrap_or_default();
            let buffers_json = root
                .get_mut("buffers")
                .and_then(Value::as_array_mut)
                .ok_or("missing buffers")?;
            for (i, buffer) in buffers_json.iter_mut().enumerate() {
                let Some(obj) = buffer.as_object_mut() else {
                    continue;
                };
                let empty = obj
                    .get_mut("extensions")
                    .and_then(Value::as_object_mut)
                    .map(|exts| {
                        exts.remove(EXT);
                        exts.is_empty()
                    });
                if empty == Some(true) {
                    obj.remove("extensions");
                }
                if referenced.contains(&(i as u64))
                    || obj.contains_key("uri")
                    || Some(i) == bin_buffer
                {
                    continue;
                }
                // the extension's uri-less fallback buffers are unreadable once every
                // view has been repointed; shrink them to four readable bytes
                obj.insert("byteLength".to_owned(), json!(4));
                obj.insert(
                    "uri".to_owned(),
                    json!("data:application/octet-stream;base64,AAAAAA=="),
                );
            }
        }

        strip_extension(&mut root, "extensionsUsed", EXT);
        strip_extension(&mut root, "extensionsRequired", EXT);
        // the gltf crate rejects files that *require* extensions it does not know;
        // quantized attributes that survive the rewrite are read natively by bevy
        strip_extension(&mut root, "extensionsRequired", QUANTIZATION_EXT);

        let json_bytes = serde_json::to_vec(&root).map_err(|e| e.to_string())?;
        if container.is_glb {
            let bin = if sink_is_blob { Some(sink_bytes) } else { blob };
            Ok(write_glb(json_bytes, bin))
        } else {
            Ok(json_bytes)
        }
    }

    fn read_buffers<'a>(document: &Document, blob: Option<&'a [u8]>) -> Vec<Option<Cow<'a, [u8]>>> {
        document
            .buffers()
            .map(|b| match b.source() {
                gltf::buffer::Source::Bin => blob.map(Cow::Borrowed),
                gltf::buffer::Source::Uri(uri) => decode_data_uri(uri).map(Cow::Owned),
            })
            .collect()
    }

    fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
        let rest = uri.strip_prefix("data:")?;
        let (_, data) = rest.split_once(";base64,")?;
        base64::engine::general_purpose::STANDARD.decode(data).ok()
    }

    fn append_aligned(sink: &mut Vec<u8>, bytes: &[u8]) -> usize {
        while !sink.len().is_multiple_of(4) {
            sink.push(0);
        }
        let offset = sink.len();
        sink.extend_from_slice(bytes);
        offset
    }

    fn strip_extension(root: &mut Value, key: &str, name: &str) {
        let empty = match root.get_mut(key).and_then(Value::as_array_mut) {
            Some(list) => {
                list.retain(|v| v.as_str() != Some(name));
                list.is_empty()
            }
            None => return,
        };
        if empty {
            if let Some(obj) = root.as_object_mut() {
                obj.remove(key);
            }
        }
    }

    pub(super) fn write_glb(mut json: Vec<u8>, bin: Option<Vec<u8>>) -> Vec<u8> {
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = bin;
        if let Some(bin) = &mut bin {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
        }
        let total = 12 + 8 + json.len() + bin.as_ref().map(|b| 8 + b.len()).unwrap_or(0);
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        if let Some(bin) = bin {
            out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            out.extend_from_slice(b"BIN\0");
            out.extend_from_slice(&bin);
        }
        out
    }

    struct ExtStream {
        buffer: usize,
        offset: usize,
        length: usize,
        stride: usize,
        count: usize,
        mode: String,
        filter: Option<String>,
    }

    fn parse_ext(v: &Value) -> Result<ExtStream, String> {
        let get = |k: &str| -> Result<usize, String> {
            v.get(k)
                .and_then(Value::as_u64)
                .map(|x| x as usize)
                .ok_or_else(|| format!("ext.{k} missing"))
        };
        Ok(ExtStream {
            buffer: get("buffer")?,
            offset: get("byteOffset").unwrap_or_default(),
            length: get("byteLength")?,
            stride: get("byteStride")?,
            count: get("count")?,
            mode: v
                .get("mode")
                .and_then(Value::as_str)
                .ok_or("ext.mode missing")?
                .to_owned(),
            filter: v.get("filter").and_then(Value::as_str).map(str::to_owned),
        })
    }

    struct DecodedView {
        bytes: Vec<u8>,
        stride: usize,
        attributes: bool,
    }

    // per-stream expansion bounds still let a small file claim huge outputs from many
    // views over the same source bytes; a file-wide cap keeps wasm32 (where a failed
    // memory.grow aborts the engine) out of allocation-abort territory
    const MAX_DECODE_TOTAL: usize = 512 << 20;

    fn take_budget(size: usize, budget: &mut usize) -> Result<(), String> {
        if size > *budget {
            return Err(format!(
                "decoded streams exceed the {} MiB cap",
                MAX_DECODE_TOTAL >> 20
            ));
        }
        *budget -= size;
        Ok(())
    }

    fn decode_vertex_stream(
        f: &ExtStream,
        src: &[u8],
        budget: &mut usize,
    ) -> Result<Vec<u8>, String> {
        if f.count == 0 || !(4..=256).contains(&f.stride) || f.stride % 4 != 0 {
            return Err(format!("bad attribute stride {}", f.stride));
        }
        // count/stride are attacker-controlled: a wrapping multiply would size
        // `out` short of what the C decoder writes into it (heap OOB). The
        // vertex codec expands at most 1024x (a full block's zero-mode planes
        // cost only stride/4 control bytes), so a larger claim is malformed.
        let size = f
            .count
            .checked_mul(f.stride)
            .ok_or("attribute size overflow")?;
        if size > src.len().saturating_mul(1024) {
            return Err(format!("attribute count {} exceeds stream bound", f.count));
        }
        take_budget(size, budget)?;
        let mut out = vec![0u8; size];
        let rc = unsafe {
            meshopt::ffi::meshopt_decodeVertexBuffer(
                out.as_mut_ptr().cast(),
                f.count,
                f.stride,
                src.as_ptr(),
                src.len(),
            )
        };
        if rc != 0 {
            return Err(format!("vertex decode failed rc={rc}"));
        }
        match f.filter.as_deref() {
            Some("OCTAHEDRAL") => {
                if f.stride != 4 && f.stride != 8 {
                    return Err("octahedral filter needs stride 4 or 8".into());
                }
                unsafe {
                    meshopt::ffi::meshopt_decodeFilterOct(
                        out.as_mut_ptr().cast(),
                        f.count,
                        f.stride,
                    )
                }
            }
            Some("QUATERNION") => {
                if f.stride != 8 {
                    return Err("quaternion filter needs stride 8".into());
                }
                unsafe {
                    meshopt::ffi::meshopt_decodeFilterQuat(
                        out.as_mut_ptr().cast(),
                        f.count,
                        f.stride,
                    )
                }
            }
            Some("EXPONENTIAL") => unsafe {
                meshopt::ffi::meshopt_decodeFilterExp(out.as_mut_ptr().cast(), f.count, f.stride)
            },
            None | Some("NONE") => {}
            Some(other) => return Err(format!("unsupported filter {other}")),
        }
        Ok(out)
    }

    fn decode_index_stream(
        f: &ExtStream,
        src: &[u8],
        budget: &mut usize,
    ) -> Result<Vec<u8>, String> {
        if f.count == 0 || f.count % 3 != 0 || (f.stride != 2 && f.stride != 4) {
            return Err(format!("bad index count/stride {}/{}", f.count, f.stride));
        }
        // the C decoder writes 3 indices per triangle (past `out` if count is
        // not a multiple of 3) and needs at least 1 + count/3 + 16 source
        // bytes; enforce both before allocating so an over-large or misaligned
        // count can't wrap `out` short or force a huge allocation.
        let size = f.count.checked_mul(f.stride).ok_or("index size overflow")?;
        if src.len() < 1 + f.count / 3 + 16 {
            return Err(format!("index count {} exceeds stream bound", f.count));
        }
        take_budget(size, budget)?;
        let mut out = vec![0u8; size];
        let rc = unsafe {
            meshopt::ffi::meshopt_decodeIndexBuffer(
                out.as_mut_ptr().cast(),
                f.count,
                f.stride,
                src.as_ptr(),
                src.len(),
            )
        };
        if rc != 0 {
            return Err(format!("index decode failed rc={rc}"));
        }
        Ok(out)
    }

    enum Target {
        F32 {
            comps: usize,
            ty: &'static str,
            minmax: bool,
            pad_alpha: bool,
        },
        JointsU16,
    }

    fn target_for(semantic: &Semantic) -> Option<Target> {
        match semantic {
            Semantic::Positions => Some(Target::F32 {
                comps: 3,
                ty: "VEC3",
                minmax: true,
                pad_alpha: false,
            }),
            Semantic::Normals => Some(Target::F32 {
                comps: 3,
                ty: "VEC3",
                minmax: false,
                pad_alpha: false,
            }),
            Semantic::Tangents => Some(Target::F32 {
                comps: 4,
                ty: "VEC4",
                minmax: false,
                pad_alpha: false,
            }),
            Semantic::TexCoords(0 | 1) => Some(Target::F32 {
                comps: 2,
                ty: "VEC2",
                minmax: false,
                pad_alpha: false,
            }),
            Semantic::Colors(0) => Some(Target::F32 {
                comps: 4,
                ty: "VEC4",
                minmax: false,
                pad_alpha: true,
            }),
            Semantic::Joints(0) => Some(Target::JointsU16),
            Semantic::Weights(0) => Some(Target::F32 {
                comps: 4,
                ty: "VEC4",
                minmax: false,
                pad_alpha: false,
            }),
            _ => None,
        }
    }

    struct Planned {
        accessor: usize,
        bytes: Vec<u8>,
        component_type: u64,
        ty: &'static str,
        min_max: Option<([f32; 3], [f32; 3])>,
    }

    fn accessor_source<'a>(
        accessor: &Accessor,
        decoded: &'a [Option<DecodedView>],
        buffers: &'a [Option<Cow<'a, [u8]>>],
    ) -> Result<(&'a [u8], usize, usize), String> {
        if accessor.sparse().is_some() {
            return Err("sparse accessor in a compressed primitive".into());
        }
        let view = accessor.view().ok_or("accessor without bufferView")?;
        if let Some(dv) = &decoded[view.index()] {
            Ok((&dv.bytes, accessor.offset(), dv.stride))
        } else {
            let buf = buffers
                .get(view.buffer().index())
                .and_then(|b| b.as_deref())
                .ok_or("attribute in unavailable buffer")?;
            let elem = accessor.size();
            Ok((
                buf,
                view.offset() + accessor.offset(),
                view.stride().unwrap_or(elem),
            ))
        }
    }

    fn plan_accessor(
        target: Target,
        accessor: &Accessor,
        decoded: &[Option<DecodedView>],
        buffers: &[Option<Cow<[u8]>>],
    ) -> Result<Planned, String> {
        let (data, base, stride) = accessor_source(accessor, decoded, buffers)?;
        match target {
            Target::JointsU16 => {
                let rows = rows_u16(accessor, data, base, stride)?;
                let mut bytes = Vec::with_capacity(rows.len() * 8);
                for row in &rows {
                    for v in row {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                }
                Ok(Planned {
                    accessor: accessor.index(),
                    bytes,
                    component_type: 5123,
                    ty: "VEC4",
                    min_max: None,
                })
            }
            Target::F32 {
                comps,
                ty,
                minmax,
                pad_alpha,
            } => {
                let mut rows = rows_f32(accessor, data, base, stride)?;
                if pad_alpha && dims(accessor.dimensions()) == 3 {
                    for row in &mut rows {
                        row[3] = 1.0;
                    }
                }
                let min_max = if minmax {
                    let mut min = [f32::MAX; 3];
                    let mut max = [f32::MIN; 3];
                    for row in &rows {
                        for c in 0..3 {
                            if !row[c].is_finite() {
                                return Err("non-finite position components".into());
                            }
                            min[c] = min[c].min(row[c]);
                            max[c] = max[c].max(row[c]);
                        }
                    }
                    Some((min, max))
                } else {
                    None
                };
                let mut bytes = Vec::with_capacity(rows.len() * comps * 4);
                for row in &rows {
                    for v in row.iter().take(comps) {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                }
                Ok(Planned {
                    accessor: accessor.index(),
                    bytes,
                    component_type: 5126,
                    ty,
                    min_max,
                })
            }
        }
    }

    fn check_indices(
        accessor: &Accessor,
        decoded: &[Option<DecodedView>],
        n_verts: usize,
    ) -> Result<(), String> {
        let Some(view) = accessor.view() else {
            return Ok(());
        };
        let Some(dv) = &decoded[view.index()] else {
            return Ok(());
        };
        let base = accessor.offset();
        let count = accessor.count();
        if count == 0 {
            return Ok(());
        }
        let end = base
            .checked_add(
                count
                    .checked_mul(dv.stride)
                    .ok_or("index extent overflow")?,
            )
            .ok_or("index extent overflow")?;
        if end > dv.bytes.len() {
            return Err("index accessor out of range".into());
        }
        let mut max = 0usize;
        for i in 0..count {
            let off = base + i * dv.stride;
            let v = if dv.stride == 2 {
                u16::from_le_bytes(dv.bytes[off..off + 2].try_into().unwrap()) as usize
            } else {
                u32::from_le_bytes(dv.bytes[off..off + 4].try_into().unwrap()) as usize
            };
            max = max.max(v);
        }
        if max >= n_verts {
            return Err(format!("index {max} >= vertex count {n_verts}"));
        }
        Ok(())
    }

    fn component_size(dt: DataType) -> usize {
        match dt {
            DataType::I8 | DataType::U8 => 1,
            DataType::I16 | DataType::U16 => 2,
            DataType::U32 | DataType::F32 => 4,
        }
    }

    fn dims(d: Dimensions) -> usize {
        match d {
            Dimensions::Scalar => 1,
            Dimensions::Vec2 => 2,
            Dimensions::Vec3 => 3,
            Dimensions::Vec4 => 4,
            Dimensions::Mat2 => 4,
            Dimensions::Mat3 => 9,
            Dimensions::Mat4 => 16,
        }
    }

    fn component_f32(bytes: &[u8], off: usize, dt: DataType, normalized: bool) -> f32 {
        match dt {
            DataType::F32 => f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()),
            DataType::U8 => {
                let v = bytes[off] as f32;
                if normalized {
                    v / 255.0
                } else {
                    v
                }
            }
            DataType::I8 => {
                let v = bytes[off] as i8 as f32;
                if normalized {
                    (v / 127.0).max(-1.0)
                } else {
                    v
                }
            }
            DataType::U16 => {
                let v = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as f32;
                if normalized {
                    v / 65535.0
                } else {
                    v
                }
            }
            DataType::I16 => {
                let v = i16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()) as f32;
                if normalized {
                    (v / 32767.0).max(-1.0)
                } else {
                    v
                }
            }
            DataType::U32 => u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as f32,
        }
    }

    fn row_bounds(
        accessor: &Accessor,
        data: &[u8],
        base: usize,
        stride: usize,
    ) -> Result<(usize, usize, usize), String> {
        let count = accessor.count();
        let n = dims(accessor.dimensions()).min(4);
        let csize = component_size(accessor.data_type());
        if count == 0 {
            return Err("empty accessor".into());
        }
        if stride < n * csize {
            return Err("component layout exceeds stride".into());
        }
        let end = base
            .checked_add(
                (count - 1)
                    .checked_mul(stride)
                    .ok_or("accessor extent overflow")?,
            )
            .and_then(|x| x.checked_add(n * csize))
            .ok_or("accessor extent overflow")?;
        if end > data.len() {
            return Err("accessor out of range".into());
        }
        Ok((count, n, csize))
    }

    fn rows_f32(
        accessor: &Accessor,
        data: &[u8],
        base: usize,
        stride: usize,
    ) -> Result<Vec<[f32; 4]>, String> {
        let (count, n, csize) = row_bounds(accessor, data, base, stride)?;
        let dt = accessor.data_type();
        let normalized = accessor.normalized();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let row_base = base + i * stride;
            let mut row = [0.0f32; 4];
            for (c, slot) in row.iter_mut().enumerate().take(n) {
                *slot = component_f32(data, row_base + c * csize, dt, normalized);
            }
            out.push(row);
        }
        Ok(out)
    }

    fn rows_u16(
        accessor: &Accessor,
        data: &[u8],
        base: usize,
        stride: usize,
    ) -> Result<Vec<[u16; 4]>, String> {
        let (count, n, csize) = row_bounds(accessor, data, base, stride)?;
        let dt = accessor.data_type();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let row_base = base + i * stride;
            let mut row = [0u16; 4];
            for (c, slot) in row.iter_mut().enumerate().take(n) {
                let off = row_base + c * csize;
                *slot = match dt {
                    DataType::U8 | DataType::I8 => data[off] as u16,
                    DataType::U16 | DataType::I16 => {
                        u16::from_le_bytes(data[off..off + 2].try_into().unwrap())
                    }
                    DataType::U32 | DataType::F32 => {
                        u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as u16
                    }
                };
            }
            out.push(row);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::imp::{decompress_gltf, write_glb};
    use serde_json::{json, Value};

    fn codec_vertex(data: &[u8], count: usize, stride: usize) -> Vec<u8> {
        let bound = unsafe { meshopt::ffi::meshopt_encodeVertexBufferBound(count, stride) };
        let mut out = vec![0u8; bound];
        let n = unsafe {
            meshopt::ffi::meshopt_encodeVertexBufferLevel(
                out.as_mut_ptr(),
                out.len(),
                data.as_ptr().cast(),
                count,
                stride,
                2,
                0,
            )
        };
        assert!(n > 0);
        out.truncate(n);
        out
    }

    fn codec_index(indices: &[u32]) -> Vec<u8> {
        unsafe { meshopt::ffi::meshopt_encodeIndexVersion(1) };
        let bound =
            unsafe { meshopt::ffi::meshopt_encodeIndexBufferBound(indices.len(), indices.len()) };
        let mut out = vec![0u8; bound];
        let n = unsafe {
            meshopt::ffi::meshopt_encodeIndexBuffer(
                out.as_mut_ptr(),
                out.len(),
                indices.as_ptr(),
                indices.len(),
            )
        };
        assert!(n > 0);
        out.truncate(n);
        out
    }

    fn filter_oct8(rows: &[[f32; 4]]) -> Vec<u8> {
        let mut out = vec![0u8; rows.len() * 4];
        unsafe {
            meshopt::ffi::meshopt_encodeFilterOct(
                out.as_mut_ptr().cast(),
                rows.len(),
                4,
                8,
                rows.as_ptr().cast(),
            );
        }
        out
    }

    fn filter_exp(flat: &[f32], count: usize, stride: usize) -> Vec<u8> {
        let mut out = vec![0u8; count * stride];
        unsafe {
            meshopt::ffi::meshopt_encodeFilterExp(
                out.as_mut_ptr().cast(),
                count,
                stride,
                16,
                flat.as_ptr(),
                meshopt::ffi::meshopt_EncodeExpMode_meshopt_EncodeExpSharedVector,
            );
        }
        out
    }

    struct Builder {
        bin: Vec<u8>,
        views: Vec<Value>,
        accessors: Vec<Value>,
    }

    impl Builder {
        fn compressed(
            &mut self,
            bytes: &[u8],
            stride: usize,
            count: usize,
            mode: &str,
            filter: Option<&str>,
            accessor: Value,
        ) -> usize {
            while !self.bin.len().is_multiple_of(4) {
                self.bin.push(0);
            }
            let off = self.bin.len();
            self.bin.extend_from_slice(bytes);
            let mut e = json!({
                "buffer": 0, "byteOffset": off, "byteLength": bytes.len(),
                "byteStride": stride, "count": count, "mode": mode
            });
            if let Some(f) = filter {
                e["filter"] = json!(f);
            }
            let mut view = json!({
                "buffer": 0, "byteOffset": off, "byteLength": bytes.len(),
                "extensions": {"EXT_meshopt_compression": e}
            });
            if mode == "ATTRIBUTES" {
                view["byteStride"] = json!(stride);
            }
            self.views.push(view);
            let mut accessor = accessor;
            accessor["bufferView"] = json!(self.views.len() - 1);
            accessor["count"] = json!(count);
            self.accessors.push(accessor);
            self.accessors.len() - 1
        }

        fn raw(&mut self, bytes: &[u8], count: usize, accessor: Value) -> usize {
            while !self.bin.len().is_multiple_of(4) {
                self.bin.push(0);
            }
            let off = self.bin.len();
            self.bin.extend_from_slice(bytes);
            self.views.push(json!({
                "buffer": 0, "byteOffset": off, "byteLength": bytes.len()
            }));
            let mut accessor = accessor;
            accessor["bufferView"] = json!(self.views.len() - 1);
            accessor["count"] = json!(count);
            self.accessors.push(accessor);
            self.accessors.len() - 1
        }
    }

    const POSITIONS: [[f32; 3]; 4] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    const WEIGHTS: [[f32; 4]; 4] = [
        [0.5, 0.5, 0.0, 0.0],
        [0.75, 0.25, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.25, 0.25, 0.25, 0.25],
    ];
    const INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];

    fn build_quad() -> (Vec<u8>, Vec<u8>) {
        let mut b = Builder {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        let flat: Vec<f32> = POSITIONS.iter().flatten().copied().collect();
        let pos = codec_vertex(&filter_exp(&flat, 4, 12), 4, 12);
        b.compressed(
            &pos,
            12,
            4,
            "ATTRIBUTES",
            Some("EXPONENTIAL"),
            json!({"componentType": 5126, "type": "VEC3",
                   "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]}),
        );
        let normals = [[0.0f32, 0.0, 1.0, 1.0]; 4];
        let nrm = codec_vertex(&filter_oct8(&normals), 4, 4);
        b.compressed(
            &nrm,
            4,
            4,
            "ATTRIBUTES",
            Some("OCTAHEDRAL"),
            json!({"componentType": 5120, "type": "VEC3", "normalized": true}),
        );
        let mut jbytes = Vec::new();
        for row in [[3u16, 7, 0, 0]; 4] {
            for v in row {
                jbytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        let joints = codec_vertex(&jbytes, 4, 8);
        b.compressed(
            &joints,
            8,
            4,
            "ATTRIBUTES",
            None,
            json!({"componentType": 5123, "type": "VEC4"}),
        );
        let mut wbytes = Vec::new();
        for row in WEIGHTS {
            for v in row {
                wbytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        let weights = codec_vertex(&wbytes, 4, 16);
        b.compressed(
            &weights,
            16,
            4,
            "ATTRIBUTES",
            None,
            json!({"componentType": 5126, "type": "VEC4"}),
        );
        let mut uvbytes = Vec::new();
        for row in [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            for v in row {
                uvbytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.raw(&uvbytes, 4, json!({"componentType": 5126, "type": "VEC2"}));
        let idx = codec_index(&INDICES);
        b.compressed(
            &idx,
            2,
            6,
            "TRIANGLES",
            None,
            json!({"componentType": 5123, "type": "SCALAR"}),
        );

        let gltf = json!({
            "asset": {"version": "2.0"},
            "extensionsUsed": ["EXT_meshopt_compression", "KHR_mesh_quantization"],
            "extensionsRequired": ["EXT_meshopt_compression", "KHR_mesh_quantization"],
            "buffers": [{"byteLength": b.bin.len()}],
            "bufferViews": b.views,
            "accessors": b.accessors,
            "meshes": [{"primitives": [{
                "attributes": {"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2,
                               "WEIGHTS_0": 3, "TEXCOORD_0": 4},
                "indices": 5
            }]}]
        });
        (serde_json::to_vec(&gltf).unwrap(), b.bin)
    }

    fn decompress(json_bytes: &[u8], bin: &[u8]) -> Result<Option<Vec<u8>>, String> {
        decompress_gltf(&write_glb(json_bytes.to_vec(), Some(bin.to_vec())))
    }

    #[test]
    fn compressed_primitive_decodes_all_attributes() {
        let (json_bytes, bin) = build_quad();
        let out = decompress(&json_bytes, &bin)
            .unwrap()
            .expect("compressed file must be rewritten");
        // the rewritten file must pass the gltf crate's validation, i.e. survive the
        // stock loader's parse
        let file = gltf::Gltf::from_slice(&out).expect("rewritten gltf must validate");
        let blob = file.blob.clone().expect("glb blob");
        let mesh = file.document.meshes().next().unwrap();
        let prim = mesh.primitives().next().unwrap();
        let reader = prim.reader(|buffer| (buffer.index() == 0).then_some(blob.as_slice()));

        let pos: Vec<[f32; 3]> = reader.read_positions().expect("no positions").collect();
        assert_eq!(pos.len(), 4);
        for (got, want) in pos.iter().zip(POSITIONS.iter()) {
            for c in 0..3 {
                assert!((got[c] - want[c]).abs() < 1e-3, "{got:?} vs {want:?}");
            }
        }

        let nrm: Vec<[f32; 3]> = reader.read_normals().expect("no normals").collect();
        for n in &nrm {
            assert!(
                n[2] > 0.98 && n[0].abs() < 0.05 && n[1].abs() < 0.05,
                "{n:?}"
            );
        }

        let joints: Vec<[u16; 4]> = match reader.read_joints(0).expect("no joints") {
            gltf::mesh::util::ReadJoints::U16(it) => it.collect(),
            _ => panic!("joints not u16"),
        };
        assert_eq!(joints, vec![[3u16, 7, 0, 0]; 4]);

        let weights: Vec<[f32; 4]> = match reader.read_weights(0).expect("no weights") {
            gltf::mesh::util::ReadWeights::F32(it) => it.collect(),
            _ => panic!("weights not f32"),
        };
        for (got, want) in weights.iter().zip(WEIGHTS.iter()) {
            for c in 0..4 {
                assert!((got[c] - want[c]).abs() < 1e-6, "{got:?} vs {want:?}");
            }
        }

        let uv: Vec<[f32; 2]> = reader
            .read_tex_coords(0)
            .expect("no uvs")
            .into_f32()
            .collect();
        assert_eq!(uv.len(), 4);
        assert!((uv[2][0] - 1.0).abs() < 1e-6);

        let ix: Vec<u32> = reader
            .read_indices()
            .expect("no indices")
            .into_u32()
            .collect();
        assert_eq!(ix, INDICES);
    }

    #[test]
    fn plain_primitive_is_left_untouched() {
        let mut b = Builder {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        let mut pbytes = Vec::new();
        for row in POSITIONS {
            for v in row {
                pbytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.raw(&pbytes, 4, json!({"componentType": 5126, "type": "VEC3"}));
        let gltf = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": b.bin.len()}],
            "bufferViews": b.views,
            "accessors": b.accessors,
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]
        });
        let json_bytes = serde_json::to_vec(&gltf).unwrap();
        assert!(decompress(&json_bytes, &b.bin).unwrap().is_none());
    }

    #[test]
    fn corrupt_stream_is_an_error_not_a_fallthrough() {
        let (json_bytes, mut bin) = build_quad();
        let n = bin.len();
        bin.truncate(n / 2);
        assert!(decompress(&json_bytes, &bin).is_err());
    }

    fn vertex_stream_gltf(count: usize) -> (Vec<u8>, Vec<u8>) {
        let mut b = Builder {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        b.compressed(
            &[0xa1u8; 40],
            4,
            count,
            "ATTRIBUTES",
            None,
            json!({"componentType": 5126, "type": "VEC3",
                   "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]}),
        );
        let gltf = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": b.bin.len()}],
            "bufferViews": b.views,
            "accessors": b.accessors,
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]
        });
        (serde_json::to_vec(&gltf).unwrap(), b.bin)
    }

    fn index_stream_gltf(count: usize, stride: usize, component: i64) -> (Vec<u8>, Vec<u8>) {
        let mut b = Builder {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        let mut pbytes = Vec::new();
        for row in POSITIONS {
            for v in row {
                pbytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.raw(
            &pbytes,
            4,
            json!({"componentType": 5126, "type": "VEC3",
                   "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]}),
        );
        b.compressed(
            &[0xe1u8; 40],
            stride,
            count,
            "TRIANGLES",
            None,
            json!({"componentType": component, "type": "SCALAR"}),
        );
        let gltf = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": b.bin.len()}],
            "bufferViews": b.views,
            "accessors": b.accessors,
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}]
        });
        (serde_json::to_vec(&gltf).unwrap(), b.bin)
    }

    // count*stride wraps to a short allocation the C decoder would write past.
    #[test]
    fn vertex_count_overflow_is_rejected_not_oob() {
        let (json_bytes, bin) = vertex_stream_gltf(0x4000_0000_0000_0000);
        assert!(decompress(&json_bytes, &bin).is_err());
    }

    // ~4 MB of vertices claimed from a 40-byte stream (beyond the 1024x max).
    #[test]
    fn vertex_count_beyond_compressed_bound_is_rejected() {
        let (json_bytes, bin) = vertex_stream_gltf(1_000_002);
        assert!(decompress(&json_bytes, &bin).is_err());
    }

    // count not a multiple of 3 would let the decoder write a triangle past out.
    #[test]
    fn misaligned_index_count_is_rejected() {
        let (json_bytes, bin) = index_stream_gltf(7, 2, 5123);
        assert!(decompress(&json_bytes, &bin).is_err());
    }

    // 3M indices need > 1 MB of source; a 40-byte stream cannot hold them.
    #[test]
    fn oversized_index_count_is_rejected() {
        let (json_bytes, bin) = index_stream_gltf(3_000_000, 4, 5125);
        assert!(decompress(&json_bytes, &bin).is_err());
    }

    // dangling cross-references hit panicking unwraps in the gltf crate's document
    // walk (and a direct index in its validator); they must surface as errors
    #[test]
    fn dangling_refs_are_errors_not_panics() {
        let dangling_attribute = json!({
            "asset": {"version": "2.0"},
            "extensionsUsed": ["EXT_meshopt_compression"],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 9}}]}]
        });
        assert!(decompress_gltf(&serde_json::to_vec(&dangling_attribute).unwrap()).is_err());
        let dangling_view = json!({
            "asset": {"version": "2.0"},
            "extensionsUsed": ["EXT_meshopt_compression"],
            "accessors": [{"bufferView": 7, "componentType": 5126, "type": "VEC3", "count": 1,
                           "min": [0.0, 0.0, 0.0], "max": [0.0, 0.0, 0.0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]
        });
        assert!(decompress_gltf(&serde_json::to_vec(&dangling_view).unwrap()).is_err());
    }

    // the validation gate must fail on anything beyond the two tolerated extension
    // markers, keeping unwalkable json away from the transform
    #[test]
    fn non_extension_validation_errors_fail_the_file() {
        let (json_bytes, bin) = build_quad();
        let mut root: Value = serde_json::from_slice(&json_bytes).unwrap();
        root["accessors"][0]["componentType"] = json!(9999);
        assert!(decompress(&serde_json::to_vec(&root).unwrap(), &bin).is_err());
    }

    // 600 MB of claimed output from a 600 KB stream passes the 1024x expansion bound
    // but must trip the absolute cap before allocating
    #[test]
    fn decoded_total_over_cap_is_rejected() {
        let mut b = Builder {
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        };
        let src = vec![0xa1u8; 600_000];
        b.compressed(
            &src,
            4,
            150_000_000,
            "ATTRIBUTES",
            None,
            json!({"componentType": 5126, "type": "VEC3",
                   "min": [0.0, 0.0, 0.0], "max": [0.0, 0.0, 0.0]}),
        );
        let gltf = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": b.bin.len()}],
            "bufferViews": b.views,
            "accessors": b.accessors,
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]
        });
        let json_bytes = serde_json::to_vec(&gltf).unwrap();
        let err = decompress(&json_bytes, &b.bin).unwrap_err();
        assert!(err.contains("cap"), "{err}");
    }

    // native smoke against a real meshopt-compressed file (e.g. gltfpack -cc output):
    // MESHOPT_SMOKE_GLB=/path/to.glb cargo test --lib smoke_decode -- --ignored
    #[test]
    #[ignore]
    fn smoke_decode_real_glb() {
        let path = std::env::var("MESHOPT_SMOKE_GLB").expect("MESHOPT_SMOKE_GLB not set");
        let bytes = std::fs::read(&path).unwrap();
        let out = decompress_gltf(&bytes)
            .unwrap()
            .expect("no meshopt content in input");
        let file = gltf::Gltf::from_slice(&out).expect("rewritten gltf must validate");
        let blob = file.blob.clone().expect("glb blob");
        let mut prims = 0usize;
        let mut verts = 0usize;
        for mesh in file.document.meshes() {
            for prim in mesh.primitives() {
                let reader = prim.reader(|buffer| (buffer.index() == 0).then_some(blob.as_slice()));
                let n = reader.read_positions().map(Iterator::count).unwrap_or(0);
                assert!(n > 0, "empty primitive {}/{}", mesh.index(), prim.index());
                verts += n;
                prims += 1;
            }
        }
        println!("smoke: {prims} primitives, {verts} vertices");
    }
}
