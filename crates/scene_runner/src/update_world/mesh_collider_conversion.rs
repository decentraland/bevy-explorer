// asyncronously convert meshes into colliders
// decompose the mesh into disjoint submeshes
// use the vec3 bits to hash vertices
// scale the submeshes where needed for vhacd tolerance
// run vhacd convex hull per submesh
// rescale the result if required
// generate a single compound shape from the resulting set of compound shapes

// use std::{collections::BTreeMap, cell::OnceCell};

// use bevy::{render::mesh::Indices, utils::{HashMap, HashSet}, core::cast_slice, prelude::Vec3, tasks::AsyncComputeTaskPool, math::DVec3};
// use rapier3d::{prelude::{SharedShape, Isometry, Real, Point}, parry::transformation::{ConvexHullError, vhacd::VHACDParameters, voxelization::FillMode}};

/*
pub async fn calculate_mesh_collider(positions: Vec<[f32; 3]>, maybe_indices: Option<Indices>, size_hint: Vec3, dbg: bool) -> Result<SharedShape, ConvexHullError> {
    let indices: Vec<u32> = match &maybe_indices {
        None => (0..positions.len() as u32).collect(),
        Some(Indices::U16(ixs)) => ixs.iter().map(|ix| *ix as u32).collect(),
        Some(Indices::U32(ixs)) => ixs.iter().map(|ix| *ix as u32).collect(),
    };

    let mut vhacd = vhacd_rs::VHACD::new();
    let mut shapes = Vec::default();
    let hulls = vhacd.compute(&positions, &indices);
    println!("{} hulls", hulls.len());
    for hull in hulls {
        let points = hull.points.into_iter().map(|f32s| Point::from(f32s)).collect();
        let indices: Vec<_> = hull.indices.chunks_exact(3).map(|chunk| chunk.try_into().unwrap()).collect();
        let shape = SharedShape::convex_mesh(points, &indices).unwrap();
        shapes.push((Isometry::default(), shape))
    }

    Ok(SharedShape::compound(shapes))
}

 */

//  use std::collections::BTreeMap;

use bevy::{
    core::cast_slice,
    prelude::{warn, Vec3},
    render::mesh::Indices,
    utils::HashMap,
};
use rapier3d::{
    parry::transformation::ConvexHullError,
    prelude::{Isometry, Point, Real, SharedShape},
};
use vhacd_rs::VHACDWrapperParams;

pub async fn calculate_mesh_collider(
    positions: Vec<[f32; 3]>,
    maybe_indices: Option<Indices>,
    size_hint: Vec3,
    dbg: bool,
    label: String,
) -> Result<SharedShape, ConvexHullError> {
    let start = std::time::Instant::now();
    if dbg {
        println!(
            "positions [{}]: {positions:?}\nindices [{:?}]: {maybe_indices:?}",
            positions.len(),
            maybe_indices.as_ref().map(|i| i.len())
        );
    }
    // calculate a unique index per vertex using the bit pattern
    let mut vertex_ids = Vec::with_capacity(positions.len());
    let mut positions_vec3 = Vec::with_capacity(positions.len());
    {
        let mut vertex_lookup: HashMap<_, usize> = HashMap::default();

        for pos in positions.iter() {
            let id = vertex_lookup
                .entry(cast_slice::<f32, u8>(pos))
                .or_insert_with(|| {
                    positions_vec3.push(Vec3::from_slice(pos));
                    positions_vec3.len() - 1
                });
            vertex_ids.push(*id);
        }
    }

    if dbg {
        println!("vec3s [{}]: {positions_vec3:?}", positions_vec3.len());
        println!("ids [{}]: {vertex_ids:?}", vertex_ids.len());
    }

    // normalize indices
    let indices: Vec<usize> = match &maybe_indices {
        None => (0..positions.len()).collect(),
        Some(Indices::U16(ixs)) => ixs.iter().map(|ix| *ix as usize).collect(),
        Some(Indices::U32(ixs)) => ixs.iter().map(|ix| *ix as usize).collect(),
    };
    // map through to vertex ids to ensure uniqueness of vertices
    let indices: Vec<usize> = indices.into_iter().map(|ix| vertex_ids[ix]).collect();

    if dbg {
        println!("normalized indices [{}]: {indices:?}", indices.len());
    }

    // lookup from vertex id -> group
    //  let mut vertex_group: Vec<Option<usize>> = vec![Some(0); positions_vec3.len()];
    //  let mut vertex_group: Vec<Option<usize>> = vec![None; positions_vec3.len()];
    //  let mut next_group = 0;

    // lookup from group -> joined groups
    //  let mut group_joins: BTreeMap<usize, HashSet<usize>> = BTreeMap::default();
    //  group_joins.insert(0, HashSet::from_iter(std::iter::once(0)));

    // per triangle / set of 3 vertex indexes
    //  for tri_indices in indices.chunks_exact(3) {
    //      let mut tri_indices: [usize;3] = tri_indices.try_into().unwrap();
    //      tri_indices.sort();

    //      // find any groups our vertices belong to
    //      let mut existing_groups: Vec<usize> = tri_indices.iter().flat_map(|ix| vertex_group[*ix]).collect();
    //      existing_groups.sort();
    //      // find the target group
    //      let target_group = existing_groups.get(0).copied().unwrap_or_else(|| { let group = next_group; next_group += 1; group });

    //      // add the target group to any new indices
    //      for ix in tri_indices {
    //          vertex_group[ix] = Some(target_group);
    //      }

    //      // join any groups that need it
    //      for group in existing_groups.iter() {
    //          group_joins.entry(target_group).or_insert_with(HashSet::default).insert(*group);
    //          group_joins.entry(*group).or_insert_with(HashSet::default).insert(target_group);
    //      }
    //  }

    if dbg {
        //  println!("vertex groups [{}]: {vertex_group:?}", vertex_group.len());
        //  println!("group joins [{}]: {group_joins:?}", group_joins.len());
    }

    let mut group_count = 0;
    // let mut tasks = Vec::default();
    let mut shapes: Vec<(Isometry<Real>, SharedShape)> = Vec::default();

    // let (sx, mut rx) = tokio::sync::mpsc::channel(10);

    // generate a compound for each supergroup
    let mut vhacd = vhacd_rs::VHACD::new();
    //  while let Some(key) = group_joins.keys().next().copied() {
    // collate the connected groups
    //  let mut target_groups = HashSet::default();
    //  target_groups.insert(key);

    //  loop {
    //      let initial_len = group_joins.len();
    //      group_joins.retain(|k, joined_groups| {
    //          if target_groups.contains(k) {
    //              target_groups.extend(joined_groups.iter());
    //              false
    //          } else {
    //              true
    //          }
    //      });
    //      if group_joins.len() == initial_len {
    //          break;
    //      }
    //  }

    // take the vertices that match
    //  let matching_indices: Vec<usize> = indices.iter().filter(|ix| {
    //      target_groups.contains(&vertex_group[**ix].unwrap())
    //  }).copied().collect();
    let matching_indices = indices;

    if dbg {
        //  println!("group from {key}: {target_groups:?}");
        println!(
            "includes [{}]: {matching_indices:?}",
            matching_indices.len()
        );
    }

    // calculate extents
    let (min, max) = matching_indices
        .iter()
        .fold((Vec3::MAX, Vec3::MIN), |(cmin, cmax), ix| {
            (cmin.min(positions_vec3[*ix]), cmax.max(positions_vec3[*ix]))
        });

    // rescale if not flat
    let size = max - min;

    // make parry-shaped data
    //  let positions_parry: Vec<_> = matching_indices.iter().map(|ix| {
    //      let pos = positions_vec3[*ix];

    //      Point::from([
    //          pos[0] * scale.x,
    //          pos[1] * scale.y,
    //          pos[2] * scale.z,
    //      ])
    //  }).collect();
    let positions_vhacd: Vec<_> = matching_indices
        .iter()
        .map(|ix| {
            let pos = positions_vec3[*ix];
            [pos[0], pos[1], pos[2]]
        })
        .collect();

    let tris = matching_indices.len() / 3;
    assert_eq!(matching_indices.len() % 3, 0);
    let indices_parry: Vec<_> = (0..tris)
        .map(|ix| {
            let ix = ix as u32;
            [ix * 3, ix * 3 + 1, ix * 3 + 2]
        })
        .collect();

    // let sx = sx.clone();
    // AsyncComputeTaskPool::get().spawn(async move {
    // let resolution = std::cmp::max(
    //     tris as u32 / 16,
    //     (size * size_hint.as_dvec3()).length_squared() as u32 / 8
    // ).clamp(64, 64);

    let size = size * size_hint;
    // let min_error = (0.01 / max_dimension).clamp(0.000001, 0.1);
    let resolution_unclamped = (size.x * size.y * size.z * 4.0 * 4.0 * 4.0) as u32;
    let resolution = resolution_unclamped.clamp(10_000, 4_000_000);

    let max_hulls = 100_000;
    let error = 0.005;
    let depth = 12;

    let params = VHACDWrapperParams {
        resolution,
        error,
        max_hulls,
        depth,
    };

    if dbg {
        println!("[{label}] group {group_count} going to vhacd. tris: {tris}, size: {size}, resolution: {resolution} (clamped from {resolution_unclamped}), hulls: {max_hulls}");
    }

    let indices: Vec<_> = indices_parry.into_iter().flatten().collect();
    let hulls = vhacd.compute(&positions_vhacd, &indices, &params);
    // let mut shapes = Vec::default();

    if dbg {
        println!("[{label}] got {} hulls", hulls.len());
    }

    // for hull in hulls.take(1) {
    for hull in hulls {
        let mid = Vec3::ZERO; //(Vec3::from(hull.min_bound) + Vec3::from(hull.max_bound)) / 2.0;
        let points: Vec<_> = hull
            .points
            .into_iter()
            .map(|f32s| Point::from(Point::from(f32s) - Point::from(mid)))
            .collect();
        // let indices: Vec<_> = hull.indices.chunks_exact(3).map(|chunk| chunk.try_into().unwrap()).collect();

        // println!("mid: {mid:?}");
        // println!("points: {points:?}");
        // println!("indices: {indices:?}");

        let Some(shape) = SharedShape::convex_hull(&points) else {
            warn!("failed on shape ...");
            continue;
        };
        // let shape = SharedShape::convex_mesh(points, &indices).unwrap();
        let iso = Isometry {
            rotation: Default::default(),
            translation: nalgebra::Translation {
                vector: (mid * -1.0).into(),
            },
        };
        shapes.push((iso, shape))
    }

    group_count += 1;

    // if shapes.len() > 0 {
    //     let _ = sx.send((Ok(SharedShape::compound(shapes)), scale)).await;
    // } else {
    //     let _ = sx.send((Err(ConvexHullError::Unreachable), scale)).await;
    // }

    // let _ = sx.send((SharedShape::convex_decomposition(&positions_parry, &indices_parry), scale)).await;
    // let _ = sx.send((SharedShape::convex_decomposition_with_params(&positions_parry, &indices_parry, &VHACDParameters { resolution, max_convex_hulls: 32, fill_mode: FillMode::SurfaceOnly, convex_hull_approximation: true, ..Default::default() }), scale)).await;
    // }).detach();
    //     group_count += 1;
    // }

    // while tasks.len() < group_count {
    //     let res = rx.recv().await.unwrap();
    //     tasks.push(res);
    //     println!("{}/{}", tasks.len(), group_count);
    // }

    // println!("tasks: {}", tasks.len());
    // for task in tasks {
    // get the compound shape for this disjoint part
    // let compound = match task.0 {

    // //     Ok(c) => c,
    // //     Err(e) => {
    // //         continue;
    // //     }
    // // };
    // let compound = compound.as_compound().unwrap();
    // let scale = task.1;

    // pull it to pieces and add them all to the big list
    // for (iso, shape) in compound.shapes().iter() {
    // for (iso, shape) in shapes {
    //     shapes.push((iso, shape));
    //     // shapes.push((*iso, shape.scale_ext(scale.recip())));
    // }
    //  }

    if shapes.is_empty() {
        println!("[{label}] nothing out for ");
        println!(
            "positions [{}]: {positions:?}\nindices [{:?}]: {maybe_indices:?}",
            positions.len(),
            maybe_indices.as_ref().map(|i| i.len())
        );
        return Err(ConvexHullError::Unreachable);
    }

    let end = std::time::Instant::now();
    let duration = end.checked_duration_since(start);
    if dbg {
        println!(
            "[{label}] done ! {group_count} groups, {} shapes, {duration:?}",
            shapes.len()
        );
    }
    Ok(SharedShape::compound(shapes))
}
