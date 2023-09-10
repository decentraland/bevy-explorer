use bevy::{
    core::cast_slice,
    math::DVec3,
    prelude::{debug, warn, Vec3},
    render::mesh::Indices,
    utils::HashMap,
};
use rapier3d_f64::{
    parry::transformation::{vhacd::VHACDParameters, ConvexHullError},
    prelude::{Point, SharedShape},
};

pub async fn calculate_mesh_collider(
    positions: Vec<[f32; 3]>,
    maybe_indices: Option<Indices>,
    size_hint: Vec3,
    label: String,
) -> Result<SharedShape, ConvexHullError> {
    let start = std::time::Instant::now();
    debug!(
        "[{label}] positions [{}]: {positions:?}\n[{label}] indices [{:?}]: {maybe_indices:?}",
        positions.len(),
        maybe_indices.as_ref().map(|i| i.len())
    );
    // calculate a unique index per vertex using the bit pattern
    let mut vertex_ids = Vec::with_capacity(positions.len());
    let mut positions_vec3 = Vec::with_capacity(positions.len());
    {
        let mut vertex_lookup: HashMap<_, usize> = HashMap::default();

        for pos in positions.iter() {
            let id = vertex_lookup
                .entry(cast_slice::<f32, u8>(pos))
                .or_insert_with(|| {
                    positions_vec3.push(Vec3::from_slice(pos).as_dvec3());
                    positions_vec3.len() - 1
                });
            vertex_ids.push(*id);
        }
    }

    debug!(
        "[{label}] vec3s [{}]: {positions_vec3:?}",
        positions_vec3.len()
    );
    debug!("[{label}]ids [{}]: {vertex_ids:?}", vertex_ids.len());

    // normalize indices
    let indices: Vec<usize> = match &maybe_indices {
        None => (0..positions.len()).collect(),
        Some(Indices::U16(ixs)) => ixs.iter().map(|ix| *ix as usize).collect(),
        Some(Indices::U32(ixs)) => ixs.iter().map(|ix| *ix as usize).collect(),
    };
    // map through to vertex ids to ensure uniqueness of vertices
    let indices: Vec<usize> = indices.into_iter().map(|ix| vertex_ids[ix]).collect();

    debug!(
        "[{label}] normalized indices [{}]: {indices:?}",
        indices.len()
    );

    // calculate extents
    let (min, max) = indices
        .iter()
        .fold((DVec3::MAX, DVec3::MIN), |(cmin, cmax), ix| {
            (cmin.min(positions_vec3[*ix]), cmax.max(positions_vec3[*ix]))
        });

    let size = max - min;

    if size.min_element() < 1e-5 {
        warn!("[{label}] skipping collider for flat mesh");
        return Err(ConvexHullError::Unreachable);
    }

    // make parry-shaped data
    let positions_parry: Vec<_> = indices
        .iter()
        .map(|ix| {
            let pos = positions_vec3[*ix];

            Point::from([pos[0], pos[1], pos[2]])
        })
        .collect();

    let tris = indices.len() / 3;
    assert_eq!(indices.len() % 3, 0);
    let indices_parry: Vec<_> = (0..tris)
        .map(|ix| {
            let ix = ix as u32;
            [ix * 3, ix * 3 + 1, ix * 3 + 2]
        })
        .collect();

    let global_size = size * size_hint.as_dvec3();
    let resolution_unclamped = global_size.max_element() * 3.0;
    let resolution = resolution_unclamped.clamp(32.0, 512.0) as u32;

    debug!("[{label}] going to vhacd. tris: {tris}, size: {global_size}, resolution: {resolution} (clamped from {resolution_unclamped})");

    let shape = SharedShape::convex_decomposition_with_params(
        &positions_parry,
        &indices_parry,
        &VHACDParameters {
            concavity: 1e-8,
            resolution,
            max_convex_hulls: 1 << 15,
            ..Default::default()
        },
    );

    if shape.is_err() {
        debug!("[{label}] nothing out for ");
        debug!(
            "[{label}] positions [{}]: {positions:?}\nindices [{:?}]: {maybe_indices:?}",
            positions.len(),
            maybe_indices.as_ref().map(|i| i.len())
        );
    }

    let end = std::time::Instant::now();
    let duration = end.checked_duration_since(start);
    debug!("[{label}] done ! {duration:?}");
    shape
}
