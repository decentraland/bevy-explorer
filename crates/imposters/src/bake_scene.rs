// scenes are saved by entity id
// cache/imposters/scenes (/specs | /textures)

use bevy::{
    core::FrameCount,
    math::FloatOrd,
    prelude::*,
    render::{primitives::Aabb, view::RenderLayers},
    utils::HashSet,
};
use boimp::{
    bake::{BakeState, ImposterBakeBundle, ImposterBakeCamera},
    GridMode, ImposterBakePlugin,
};
use common::structs::PrimaryUser;
use ipfs::{CurrentRealm, IpfsAssetServer};
use scene_material::{BoundRegion, SceneBound, SceneMaterial};

use scene_runner::{
    initialize_scene::{
        CurrentImposterScene, LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    DebugInfo,
};

use crate::{
    imposter_spec::{floor_path, texture_path, write_imposter, BakedScene, ImposterSpec},
    render::{BakingIngredients, ImposterLookup, ImposterMissing, ImposterReady, SceneImposter},
};
pub struct DclImposterBakeScenePlugin;

const GRID_SIZE: u32 = 8;
const TILE_SIZE: u32 = 128;

const IMPOSTERCEPTION_LAYER: RenderLayers = RenderLayers::layer(5);

impl Plugin for DclImposterBakeScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ImposterBakePlugin)
            .init_resource::<ImposterBakeList>()
            .init_resource::<CurrentImposterImposter>()
            .add_systems(
                Update,
                (
                    make_scene_oven,
                    bake_scene_imposters,
                    bake_imposter_imposter,
                    check_bake_state,
                    pick_imposter_to_bake,
                )
                    .chain()
                    .before(crate::render::spawn_imposters),
            );
    }
}

#[derive(Component)]
pub struct ImposterOven {
    start_tick: u32,
    hash: String,
    unbaked_parcels: Vec<BoundRegion>,
    all_parcels: HashSet<IVec2>,
    baked_scene: BakedScene,
}

fn make_scene_oven(
    current_imposter: Res<CurrentImposterScene>,
    mut commands: Commands,
    baking: Query<&ImposterOven>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    tick: Res<FrameCount>,
    mut start_tick: Local<Option<(String, u32)>>,
    bake_list: Res<ImposterBakeList>,
) {
    if !baking.is_empty() {
        return;
    }

    let Some(PointerResult::Exists { hash, .. }) = &current_imposter.0 else {
        return;
    };

    if start_tick
        .as_ref()
        .map_or(true, |(start_tick_hash, _)| hash != start_tick_hash)
    {
        *start_tick = Some((hash.clone(), tick.0));
    }

    if let Some(entity) = live_scenes.0.get(hash) {
        let Ok((mut context, mut transform)) = scenes.get_mut(*entity) else {
            if tick.0 > start_tick.as_ref().unwrap().1 + 1000 {
                warn!("scene load failed, spawning dummy oven");
                let Some(ImposterToBake::Scene(parcel, _)) = bake_list.0.last() else {
                    error!("bake list doesn't have a scene, scene entity is missing, but CurrentImposterScene is set?!");
                    return;
                };
                commands.spawn(ImposterOven {
                    start_tick: tick.0,
                    hash: hash.clone(),
                    unbaked_parcels: vec![BoundRegion::new(*parcel, *parcel, 1)],
                    all_parcels: HashSet::from_iter([*parcel]),
                    baked_scene: Default::default(),
                });
            }
            return;
        };

        transform.translation.y = -2000.0;

        if context.tick_number < 10
            && !context.broken
            && tick.0 < start_tick.as_ref().unwrap().1 + 1000
        {
            return;
        }

        context.blocked.insert("imposter_baking");

        warn!("baking scene {:?}", hash);

        // gather regions
        let unbaked_parcels = context
            .bounds
            .iter()
            .flat_map(|r| {
                (r.parcel_min().x..=r.parcel_max().x).map(move |x| {
                    (r.parcel_min().y..=r.parcel_max().y).map(move |y| {
                        BoundRegion::new(IVec2::new(x, y), IVec2::new(x, y), r.parcel_count)
                    })
                })
            })
            .flatten()
            .collect::<Vec<_>>();

        // spawn oven
        commands.spawn(ImposterOven {
            start_tick: tick.0,
            hash: hash.clone(),
            unbaked_parcels,
            all_parcels: context.parcels.clone(),
            baked_scene: Default::default(),
        });
    }
}

fn bake_scene_imposters(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterScene>,
    mut baking: Query<(Entity, &mut ImposterOven)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    mut scenes: Query<(&mut RendererSceneContext, &mut Transform)>,
    live_scenes: Res<LiveScenes>,
    ipfas: IpfsAssetServer,
    tick: Res<FrameCount>,
    children: Query<&Children>,
    meshes: Query<(&GlobalTransform, &Aabb), With<Handle<Mesh>>>,
    bound_materials: Query<&Handle<SceneMaterial>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    lookup: Res<ImposterLookup>,
) {
    if let Ok((baking_ent, mut oven)) = baking.get_single_mut() {
        let current_scene_ent = {
            let Some(PointerResult::Exists { hash, .. }) = &current_imposter.0 else {
                return;
            };
            let Some(entity) = live_scenes.0.get(hash) else {
                return;
            };
            *entity
        };

        let any_baking_cams = !all_baking_cams.is_empty();
        let all_cams_finished = all_baking_cams
            .iter()
            .all(|(_, cam)| cam.state == BakeState::Finished);

        if !any_baking_cams {
            let Some(region) = oven.unbaked_parcels.pop() else {
                warn!("no regions left");
                write_imposter(&ipfas, &oven.hash, IVec2::MAX, 0, &oven.baked_scene);

                if let Ok((mut context, _)) = scenes.get_mut(current_scene_ent) {
                    context.blocked.remove("imposter_baking");
                }

                for parcel in std::mem::take(&mut oven.all_parcels).drain() {
                    if let Some(entity) = lookup.0.get(&(parcel, 0)) {
                        if let Some(mut commands) = commands.get_entity(*entity) {
                            commands.remove::<ImposterMissing>();

                            if let Some(spec) = oven.baked_scene.imposters.get(&parcel) {
                                commands.try_insert(spec.clone());
                            }
                            commands.try_insert(ImposterReady(Some(oven.hash.clone())));
                        }
                    }
                }

                current_imposter.0 = None;
                commands.entity(baking_ent).despawn_recursive();
                return;
            };

            warn!("baking region: {:?}", region);

            // update materials
            for h_mat in children
                .iter_descendants(current_scene_ent)
                .filter_map(|c| bound_materials.get(c).ok())
            {
                let Some(mat) = materials.get_mut(h_mat) else {
                    continue;
                };
                mat.extension.data = SceneBound::new(vec![region], 1.0).data;
            }

            // region bounds
            let rmin = region.world_min();
            let rmax = region.world_max();

            // content bounds
            let mut points = Vec::default();
            for (gt, aabb) in children
                .iter_descendants(current_scene_ent)
                .filter_map(|e| meshes.get(e).ok())
            {
                let corners = [
                    Vec3::new(-1.0, -1.0, -1.0),
                    Vec3::new(-1.0, -1.0, 1.0),
                    Vec3::new(-1.0, 1.0, -1.0),
                    Vec3::new(-1.0, 1.0, 1.0),
                    Vec3::new(1.0, -1.0, -1.0),
                    Vec3::new(1.0, -1.0, 1.0),
                    Vec3::new(1.0, 1.0, -1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ];
                let new_points = corners
                    .iter()
                    .map(|c| {
                        gt.transform_point(
                            Vec3::from(aabb.center) + (Vec3::from(aabb.half_extents) * *c),
                        ) + Vec3::Y
                    })
                    .collect::<Vec<_>>();

                // skip low things (max y < 1)
                // skip flat things that won't get rendered
                // skip things that don't intersect our rendered region
                let min = new_points.iter().fold(Vec3::MAX, |m, p| m.min(*p));
                let max = new_points.iter().fold(Vec3::MIN, |m, p| m.max(*p));
                let intersects = max.cmpge(rmin).all() || min.cmple(rmax).all();
                if max.y > -1999.0 && max.y > min.y && intersects {
                    points.extend(new_points);
                }
            }

            // skip empty
            if points.is_empty() {
                warn!("skipping empty bake");
                // oven.baked_scene.imposters.insert(region.parcel_min(), None);
            } else {
                let mut content_aabb = Aabb::enclosing(points).unwrap();
                content_aabb.center.y += 2000.0;
                let aabb = Aabb::from_min_max(
                    rmin.max(content_aabb.min().into()),
                    rmax.min(content_aabb.max().into()),
                );
                let center = Vec3::from(aabb.center);
                let radius = aabb.half_extents.length();

                warn!("region: {rmin}-{rmax}, snap: {}-{}", aabb.min(), aabb.max());
                let tile_size = (TILE_SIZE as f32
                    * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                    / 16.0)
                    .clamp(2.0, 256.0) as u32;
                warn!("tile size: {tile_size}");

                let mut camera = ImposterBakeCamera {
                    radius,
                    grid_size: GRID_SIZE,
                    tile_size,
                    grid_mode: GridMode::Horizontal,
                    // max_tiles_per_frame: 5,
                    ..Default::default()
                };

                let path = texture_path(ipfas.ipfs(), &oven.hash, region.parcel_min(), 0);
                let _ = std::fs::create_dir_all(path.parent().unwrap());
                let callback = camera.save_asset_callback(path, true, true);
                camera.set_callback(callback);

                commands.spawn(ImposterBakeBundle {
                    camera,
                    transform: Transform::from_translation(center + Vec3::Y * -2000.0),
                    ..Default::default()
                });

                oven.baked_scene.imposters.insert(
                    region.parcel_min(),
                    ImposterSpec {
                        scale: radius,
                        region_min: aabb.min().into(),
                        region_max: aabb.max().into(),
                    },
                );
            }

            // always generate top-down
            let tile_mid = (rmin + rmax) * 0.5;
            let mut top_down = ImposterBakeCamera {
                grid_size: 1,
                radius: (rmax - rmin).xz().max_element() * 0.5 + 1.0,
                tile_size: (rmax - rmin).xz().max_element() as u32 + 2,
                order: -98,
                manual_camera_transforms: Some(vec![Transform::from_translation(Vec3::new(
                    tile_mid.x,
                    -2000.0 + 2.0,
                    tile_mid.z,
                ))
                .looking_at(Vec3::new(tile_mid.x, -2000.0, tile_mid.z), Vec3::NEG_Z)
                .into()]),
                ..Default::default()
            };

            let path = floor_path(ipfas.ipfs(), &oven.hash, region.parcel_min(), 0);
            let callback = top_down.save_asset_callback(path, true, true);
            top_down.set_callback(callback);

            commands.spawn(ImposterBakeBundle {
                transform: Transform::from_translation(Vec3::new(tile_mid.x, -2000.0, tile_mid.z)),
                camera: top_down,
                ..Default::default()
            });
        } else {
            if tick.0 % 200 == 0 {
                warn!("waiting for bake ...");
            }

            // force failed
            if oven.start_tick + 100 < tick.0 {
                warn!("forcing failed bake ...");
                for (_, mut cam) in all_baking_cams.iter_mut() {
                    cam.wait_for_render = false;
                }
            }

            // despawn on complete
            if all_cams_finished {
                warn!("finished baking");

                for (cam_ent, _) in all_baking_cams.iter() {
                    commands.entity(cam_ent).despawn_recursive();
                }
            }
        }
    }
}

fn bake_imposter_imposter(
    mut commands: Commands,
    mut current_imposter: ResMut<CurrentImposterImposter>,
    existing_imposters: Query<(Option<&ImposterSpec>, &Children)>,
    mut all_baking_cams: Query<(Entity, &mut ImposterBakeCamera)>,
    lookup: Res<ImposterLookup>,
    mut baking: Local<Option<(u32, BakedScene)>>,
    ipfas: IpfsAssetServer,
    current_realm: Res<CurrentRealm>,
    mut layers: Query<&mut RenderLayers>,
    tick: Res<FrameCount>,
) {
    if baking.is_some() {
        let all_cams_finished = all_baking_cams
            .iter()
            .all(|(_, cam)| cam.state == BakeState::Finished);

        if all_cams_finished {
            let (_, baking) = baking.take().unwrap();
            warn!("all cams done");
            let Some((parcel, level)) = current_imposter.0 else {
                return;
            };
            write_imposter(&ipfas, &current_realm.address, parcel, level, &baking);

            if let Some(entity) = lookup.0.get(&(parcel, level)) {
                if let Some(mut commands) = commands.get_entity(*entity) {
                    commands.remove::<ImposterMissing>();

                    if let Some(spec) = baking.imposters.get(&parcel) {
                        commands.try_insert(spec.clone());
                    }
                    commands.try_insert(ImposterReady(None));
                }
            }

            for (ent, _) in all_baking_cams.iter() {
                if let Some(commands) = commands.get_entity(ent) {
                    commands.despawn_recursive();
                }
            }
            current_imposter.0 = None;
        } else if tick.0 > baking.as_ref().unwrap().0 + 100 {
            for (_, mut cam) in all_baking_cams.iter_mut() {
                warn!("forcing failed imposter bake ...");
                cam.wait_for_render = false;
            }
        }

        return;
    }

    if let Some((parcel, level)) = current_imposter.0.as_ref() {
        warn!("baking mip: {:?}-{}", parcel, level);
        let size = 1 << level;
        let next_size = 1 << (level - 1);
        let mut baked_scene = BakedScene::default();

        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
            let key = (*parcel + offset * next_size, level - 1);

            if let Some(ingredient) = lookup.0.get(&key) {
                // content bounds
                let Ok((maybe_spec, children)) = existing_imposters.get(*ingredient) else {
                    warn!("missing children for imposter ingredient, don't know why, gonna bail");
                    for (ent, _) in all_baking_cams.iter() {
                        if let Some(commands) = commands.get_entity(ent) {
                            commands.despawn_recursive();
                        }
                    }        
                    *baking = None;
                    current_imposter.0 = None;
                    return;
                };
                assert_eq!(
                    children.iter().count(),
                    if maybe_spec.is_some() { 2 } else { 1 }
                );
                if let Some(spec) = maybe_spec {
                    min = min.min(spec.region_min);
                    max = max.max(spec.region_max);
                }

                // add layer to children
                for child in children.iter() {
                    let mut layer = layers.get_mut(*child).unwrap();
                    *layer = layer.union(&IMPOSTERCEPTION_LAYER);
                }
            }
        }

        // skip empty
        if min.cmpgt(max).any() {
            warn!("skipping empty imposterception bake");
            // oven.baked_scene.imposters.insert(region.parcel_min(), None);
        } else {
            let aabb = Aabb::from_min_max(min, max);
            let center = Vec3::from(aabb.center);
            let radius = aabb.half_extents.length();

            let tile_size = ((TILE_SIZE / size) as f32
                * aabb.half_extents.xz().length().max(aabb.half_extents.y)
                / 16.0)
                .clamp(2.0, 256.0) as u32;
            warn!("tile size: {tile_size}");

            let mut camera = ImposterBakeCamera {
                radius,
                grid_size: GRID_SIZE,
                tile_size,
                grid_mode: GridMode::Horizontal,
                // max_tiles_per_frame: 5,
                multisample: 1,
                ..Default::default()
            };

            let path = texture_path(ipfas.ipfs(), &current_realm.address, *parcel, *level);
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let callback = camera.save_asset_callback(path, true, true);
            camera.set_callback(callback);

            commands.spawn((
                ImposterBakeBundle {
                    camera,
                    transform: Transform::from_translation(center),
                    ..Default::default()
                },
                IMPOSTERCEPTION_LAYER,
            ));

            baked_scene.imposters.insert(
                *parcel,
                ImposterSpec {
                    scale: radius,
                    region_min: aabb.min().into(),
                    region_max: aabb.max().into(),
                },
            );
        }

        // always generate top-down
        let mid = (*parcel * IVec2::new(1, -1) * 16).as_vec2()
            + Vec2::new(size as f32, -(size as f32)) * 16.0 * 0.5;
        let mut top_down = ImposterBakeCamera {
            grid_size: 1,
            radius: (size as f32) * PARCEL_SIZE * 0.5 + size as f32,
            tile_size: 16 + 2,
            order: -98,
            multisample: 1,
            manual_camera_transforms: Some(vec![Transform::from_translation(Vec3::new(
                mid.x, 2.0, mid.y,
            ))
            .looking_at(Vec3::new(mid.x, 0.0, mid.y), Vec3::NEG_Z)
            .into()]),
            ..Default::default()
        };

        let path = floor_path(ipfas.ipfs(), &current_realm.address, *parcel, *level);
        let callback = top_down.save_asset_callback(path, true, true);
        top_down.set_callback(callback);

        commands.spawn((
            ImposterBakeBundle {
                transform: Transform::from_translation(Vec3::new(mid.x, 0.0, mid.y)),
                camera: top_down,
                ..Default::default()
            },
            IMPOSTERCEPTION_LAYER,
        ));

        *baking = Some((tick.0, baked_scene));
    }
}

#[derive(Debug)]
pub enum ImposterToBake {
    Scene(IVec2, PointerResult),
    Mip(IVec2, usize),
}

#[derive(Resource, Default, Debug)]
pub struct ImposterBakeList(Vec<ImposterToBake>);

fn pick_imposter_to_bake(
    q: Query<(&SceneImposter, &ImposterMissing)>,
    focus: Query<&GlobalTransform, With<PrimaryUser>>,
    scene_pointers: Res<ScenePointers>,
    live_scenes: Res<LiveScenes>,
    mut baking: ResMut<ImposterBakeList>,
    current_realm: Res<CurrentRealm>,
) {
    if current_realm.address.is_empty() {
        return;
    }

    if !baking.0.is_empty() {
        return;
    }

    let Ok(focus) = focus.get_single().map(|gt| gt.translation()) else {
        return;
    };

    let mut missing = q
        .iter()
        .map(|(imposter, missing)| {
            let midpoint = (imposter.parcel * IVec2::new(1, -1)).as_vec2() * PARCEL_SIZE
                + ((1 << imposter.level) as f32 * PARCEL_SIZE) * 0.5;
            ((midpoint - focus.xz()).length_squared(), imposter, missing)
        })
        .filter(|(_, _, missing)| {
            missing
                .0
                .as_ref()
                .map_or(true, |m| !live_scenes.0.contains_key(m))
        })
        .collect::<Vec<_>>();

    missing.sort_by_key(|(dist, ..)| FloatOrd(*dist));

    for (_, imposter, _) in missing.into_iter() {
        if imposter.level == 0 {
            if let Some(pointer) = scene_pointers.get(imposter.parcel).cloned() {
                if matches!(pointer, PointerResult::Exists { .. }) {
                    println!("picked {imposter:?}");
                    baking
                        .0
                        .push(ImposterToBake::Scene(imposter.parcel, pointer));
                    break;
                }
            }
        } else {
            println!("picked {imposter:?}");
            baking
                .0
                .push(ImposterToBake::Mip(imposter.parcel, imposter.level));
            break;
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct CurrentImposterImposter(Option<(IVec2, usize)>);

fn check_bake_state(
    mut baking: ResMut<ImposterBakeList>,
    mut current_imposter_scene: ResMut<CurrentImposterScene>,
    mut current_imposter_imposter: ResMut<CurrentImposterImposter>,
    mut ingredients: ResMut<BakingIngredients>,
    lookup: Res<ImposterLookup>,
    imposter_state: Query<
        (Option<&ImposterReady>, Option<&ImposterMissing>),
        Or<(With<ImposterReady>, With<ImposterMissing>)>,
    >,
    scene_pointers: Res<ScenePointers>,
    mut debug_info: ResMut<DebugInfo>,
) {
    #[derive(PartialEq, Debug)]
    enum State {
        Ready,
        Missing,
        Pending,
    }

    let check_state = |(parcel, level): &(IVec2, usize)| -> State {
        match lookup
            .0
            .get(&(*parcel, *level))
            .and_then(|e| imposter_state.get(*e).ok())
        {
            Some((Some(_), None)) => State::Ready,
            Some((None, Some(_))) => State::Missing,
            _ => State::Pending,
        }
    };

    ingredients.0.clear();

    if !baking.0.is_empty() {
        debug_info.info.insert(
            "Imposter Generation",
            format!(
                "{:?}",
                baking
                    .0
                    .iter()
                    .map(|p| format!("{p:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        );
    } else {
        debug_info.info.remove("Imposter Generation");
    }

    if let Some(imposter) = baking.0.last() {
        println!("bake: {:?}", imposter);
        match imposter {
            ImposterToBake::Scene(parcel, pointer_result) => {
                let key = (*parcel, 0);
                if check_state(&key) == State::Ready {
                    // done
                    println!("scene done!");
                    baking.0.pop();
                } else {
                    // don't need to check for constituents, just go
                    println!("scene running!");
                    current_imposter_scene.0 = Some(pointer_result.clone());
                    ingredients.0.insert((*parcel, 0));
                }
            }
            ImposterToBake::Mip(parcel, level) => {
                if check_state(&(*parcel, *level)) == State::Ready {
                    current_imposter_imposter.0 = None;
                    baking.0.pop();
                    ingredients.0.clear();
                    println!("mip done!");
                    return;
                } else {
                    println!("mip bake state: {:?}", check_state(&(*parcel, *level)));
                }
                ingredients.0.insert((*parcel, *level));

                let next_size = 1 << (level - 1);
                let mut any_pending = false;
                for offset in [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE] {
                    let key = (*parcel + offset * next_size, level - 1);
                    let mut pointer = None;
                    if *level == 1 {
                        match scene_pointers.get(key.0) {
                            Some(p @ PointerResult::Exists { .. }) => pointer = Some(p),
                            Some(PointerResult::Nothing) => continue,
                            None => {
                                error!("missing scene pointer for {}, bailing", &key.0);
                                return;
                            }
                        }
                    }
                    ingredients.0.insert(key);
                    match check_state(&key) {
                        State::Ready => (),
                        State::Missing => {
                            if *level == 1 {
                                baking
                                    .0
                                    .push(ImposterToBake::Scene(key.0, pointer.unwrap().clone()));
                            } else {
                                baking.0.push(ImposterToBake::Mip(key.0, key.1));
                            }
                            return;
                        }
                        State::Pending => {
                            println!("waiting for pending {key:?}");
                            any_pending = true;
                        }
                    }
                }

                if any_pending {
                    return;
                }

                // run the bake
                current_imposter_imposter.0 = Some((*parcel, *level));
            }
        }
    }
}
