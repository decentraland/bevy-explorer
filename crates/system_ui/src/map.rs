use std::path::PathBuf;

use anyhow::anyhow;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::{hashbrown::hash_map::Entry, HashMap},
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{structs::PrimaryUser, util::{TaskExt, TryPushChildrenEx}};
use ipfs::ipfs_path::IpfsPath;
use isahc::AsyncReadResponseExt;
use scene_runner::vec3_to_parcel;
use ui_core::{
    ui_actions::{ClickNoDrag, DragData, Dragged, MouseWheelData, MouseWheeled, On, UiCaller},
    ModifyComponentExt,
};

use crate::{
    discover::{spawn_discover_popup, DiscoverPage, DiscoverPages},
    profile::{SettingsDialog, SettingsTab},
};

#[derive(Component)]
pub struct MapTexture {
    pub center: Vec2,
    pub parcels_per_vw: f32,
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                set_map_content,
                render_map,
                touch_map,
                update_hover,
                handle_map_task,
            ),
        );
    }
}

#[derive(Component)]
struct MapData {
    cursor: Entity,
    pixels_per_parcel: f32,
    top_left_offset: Vec2,
    min_parcel: IVec2,
    max_parcel: IVec2,
    tile_entities: HashMap<(usize, i32, i32), Entity>,
}

#[derive(Component, Default)]
pub struct MapSettings {
    task: Option<(IVec2, Task<Result<DiscoverPages, anyhow::Error>>)>,
}

fn set_map_content(
    mut commands: Commands,
    dialog: Query<(Entity, Ref<SettingsDialog>)>,
    mut q: Query<(Entity, &SettingsTab, Option<&mut MapSettings>), Changed<SettingsTab>>,
    mut prev_tab: Local<Option<SettingsTab>>,
    dui: Res<DuiRegistry>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab, maybe_map_settings) in q.iter_mut() {
        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Map {
            return;
        }

        commands.entity(ent).despawn_descendants();

        if maybe_map_settings.is_none() {
            commands.entity(ent).insert(MapSettings::default());
        }

        let components = commands
            .entity(ent)
            .apply_template(&dui, "map", DuiProps::default())
            .unwrap();

        let center = player
            .get_single()
            .ok()
            .map(|gt| vec3_to_parcel(gt.translation()).as_vec2())
            .unwrap_or(Vec2::ZERO);

        debug!("using parcel {}", center);

        commands.entity(components.named("map-node")).insert((
            MapTexture {
                center,
                parcels_per_vw: 20.0,
            },
            Interaction::default(),
            On::<Dragged>::new(
                |caller: Res<UiCaller>,
                 mut map: Query<(&DragData, &mut MapTexture)>,
                 window: Query<&Window, With<PrimaryWindow>>| {
                    let Ok((drag, mut map)) = map.get_mut(caller.0) else {
                        warn!("no data");
                        return;
                    };

                    let Ok(window) = window.get_single() else {
                        return;
                    };

                    let parcel_delta_x = drag.delta_viewport.x * map.parcels_per_vw;
                    let parcel_delta_y =
                        -drag.delta_viewport.y * map.parcels_per_vw * window.height()
                            / window.width();

                    map.center -= Vec2::new(parcel_delta_x, parcel_delta_y);
                    map.center = map.center.clamp(Vec2::splat(-152.0), Vec2::splat(152.0));
                },
            ),
            On::<MouseWheeled>::new(
                |caller: Res<UiCaller>,
                 mut map: Query<(&GlobalTransform, &MouseWheelData, &mut MapTexture, &MapData)>,
                 window: Query<&Window, With<PrimaryWindow>>| {
                    let Ok(window) = window.get_single() else {
                        return;
                    };

                    let Ok((gt, wheel, mut map, data)) = map.get_mut(caller.0) else {
                        warn!("no data");
                        return;
                    };

                    let cursor_position = window.cursor_position().unwrap_or_default();
                    let cursor_rel_position = cursor_position - gt.translation().truncate();
                    let cursor_rel_position = cursor_rel_position * Vec2::new(1.0, -1.0);
                    let cursor_parcel = map.center + cursor_rel_position / data.pixels_per_parcel;

                    let adj = 1.1f32.powf(-wheel.wheel);
                    map.parcels_per_vw = (map.parcels_per_vw * adj).clamp(10.0, 500.0);
                    let pixels_per_parcel = (window.width() / map.parcels_per_vw).round();

                    map.center = cursor_parcel - cursor_rel_position / pixels_per_parcel;
                    map.center = map.center.clamp(Vec2::splat(-152.0), Vec2::splat(152.0));
                },
            ),
            On::<ClickNoDrag>::new(
                |caller: Res<UiCaller>,
                 mut q: Query<&mut MapSettings>,
                 window: Query<&Window, With<PrimaryWindow>>,
                 map: Query<(&GlobalTransform, &MapTexture, &MapData)>| {
                    let Ok(mut settings) = q.get_single_mut() else {
                        warn!("no settings");
                        return;
                    };
                    let Ok(window) = window.get_single() else {
                        warn!("no window");
                        return;
                    };
                    let Ok((gt, map, data)) = map.get(caller.0) else {
                        warn!("no map");
                        return;
                    };
                    // so grim
                    let cursor_position = window.cursor_position().unwrap_or_default();
                    let cursor_rel_position = cursor_position - gt.translation().truncate();
                    let cursor_rel_position = cursor_rel_position * Vec2::new(1.0, -1.0);
                    let cursor_parcel = map.center + cursor_rel_position / data.pixels_per_parcel;
                    let parcel = cursor_parcel.floor().as_ivec2() + IVec2::Y;
                    debug!("click parcel {}", parcel);

                    let url = format!(
                        "https://places.decentraland.org/api/places?positions={},{}",
                        parcel.x, parcel.y
                    );

                    settings.task = Some((
                        parcel,
                        IoTaskPool::get().spawn(async move {
                            debug!("url: {url}");
                            let mut response = isahc::get_async(url).await?;
                            response
                                .json::<DiscoverPages>()
                                .await
                                .map_err(|e| anyhow!(e))
                        })
                    ));
                },
            ),
        ));
    }
}

fn update_hover(
    mut commands: Commands,
    map: Query<(&GlobalTransform, &MapTexture, &MapData, &Interaction)>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    for (gt, map, data, interaction) in map.iter() {
        if interaction == &Interaction::None {
            commands.entity(data.cursor).try_insert(Visibility::Hidden);
            continue;
        }
        let cursor_position = window.cursor_position().unwrap_or_default();
        let cursor_rel_position = cursor_position - gt.translation().truncate();
        let cursor_parcel = map.center * Vec2::new(1.0, -1.0) + cursor_rel_position / data.pixels_per_parcel;
        let parcel = cursor_parcel.floor().as_ivec2();

        let topleft_pixel =
            data.top_left_offset + data.pixels_per_parcel * (parcel - data.min_parcel).as_vec2();
        let ppp = data.pixels_per_parcel;
        commands
            .entity(data.cursor)
            .modify_component(move |style: &mut Style| {
                style.left = Val::Px(topleft_pixel.x);
                style.top = Val::Px(topleft_pixel.y);
                style.width = Val::Px(ppp);
                style.height = Val::Px(ppp);
            });
        commands.entity(data.cursor).try_insert(Visibility::Visible);
    }
}

const TILE_PARCELS: [i32; 6] = [160, 80, 40, 20, 10, 5];
const PIXELS_PER_PARCEL: [f32; 6] = [
    512.0 / 160.0,
    512.0 / 80.0,
    512.0 / 40.0,
    512.0 / 20.0,
    512.0 / 10.0,
    512.0 / 5.0,
];

fn touch_map(mut e: EventReader<WindowResized>, mut q: Query<&mut MapTexture>) {
    if e.read().last().is_some() {
        for mut mt in q.iter_mut() {
            mt.set_changed();
        }
    }
}

fn render_map(
    mut commands: Commands,
    mut q: Query<
        (Entity, &Node, &MapTexture, Option<&mut MapData>),
        Or<(Changed<MapTexture>, Changed<Node>)>,
    >,
    window: Query<&Window, With<PrimaryWindow>>,
    mut styles: Query<(&mut Style, &mut Visibility)>,
    asset_server: Res<AssetServer>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };

    for (map_entity, node, map, maybe_data) in q.iter_mut() {
        let mut new_data = None;
        let data = match maybe_data {
            Some(data) => data.into_inner(),
            None => {
                let cursor = commands
                    .spawn(ImageBundle {
                        z_index: ZIndex::Local(7),
                        image: UiImage::new(asset_server.load("images/cursor.png")),
                        ..Default::default()
                    })
                    .id();
                commands.entity(map_entity).try_push_children(&[cursor]);
                new_data = Some(MapData {
                    cursor,
                    pixels_per_parcel: 0.0,
                    tile_entities: Default::default(),
                    top_left_offset: Default::default(),
                    min_parcel: Default::default(),
                    max_parcel: Default::default(),
                });
                new_data.as_mut().unwrap()
            }
        };

        let pixels_per_parcel = (window.width() / map.parcels_per_vw).round();
        data.pixels_per_parcel = pixels_per_parcel;

        let max_level = PIXELS_PER_PARCEL
            .iter()
            .position(|ppp| *ppp > pixels_per_parcel)
            .unwrap_or(5);

        debug!(
            "center: {}, ppp: {pixels_per_parcel}, node size: {}/{}, running 0..{max_level}",
            map.center,
            node.unrounded_size(),
            node.size()
        );

        let center = map.center * Vec2::new(1.0, -1.0);

        data.min_parcel = (center - node.size() / 2.0 / pixels_per_parcel)
            .floor()
            .as_ivec2();
        data.max_parcel = (center + node.size() / 2.0 / pixels_per_parcel)
            .ceil()
            .as_ivec2();

        data.min_parcel = data.min_parcel.max(IVec2::splat(-152));
        data.max_parcel = data.max_parcel.min(IVec2::splat(152));

        debug!("parcels ({}..{})", data.min_parcel, data.max_parcel);

        for entity in data.tile_entities.values() {
            if let Ok(mut vis) = styles.get_component_mut::<Visibility>(*entity) {
                *vis = Visibility::Hidden;
            }
        }

        let parcels_to_topleft = data.min_parcel.as_vec2() - center;
        let pixels_to_topleft = parcels_to_topleft * pixels_per_parcel;
        data.top_left_offset = (node.size() * 0.5 + pixels_to_topleft).floor();

        debug!(
            "parcels to tl: {}, pixels to tl: {}, tl: {}",
            parcels_to_topleft, pixels_to_topleft, data.top_left_offset
        );

        for level in 0..=max_level {
            for x in ((data.min_parcel.x + 152) / TILE_PARCELS[level])
                ..=((data.max_parcel.x + 152) / TILE_PARCELS[level])
            {
                for y in ((data.min_parcel.y + 152) / TILE_PARCELS[level])
                    ..=((data.max_parcel.y + 152) / TILE_PARCELS[level])
                {
                    let topleft_parcel = IVec2::new(x, y) * TILE_PARCELS[level] - 152;
                    let bottomright_parcel = topleft_parcel + TILE_PARCELS[level];

                    let topleft_pixel = data.top_left_offset
                        + (topleft_parcel - data.min_parcel).as_vec2() * pixels_per_parcel;
                    let bottomright_pixel = data.top_left_offset
                        + (bottomright_parcel - data.min_parcel).as_vec2() * pixels_per_parcel;

                    let left = topleft_pixel.x;
                    let right = node.size().x - bottomright_pixel.x;
                    let top = topleft_pixel.y;
                    let bottom = node.size().y - bottomright_pixel.y;

                    debug!("level: {level}, parcels: ({topleft_parcel}..{bottomright_parcel}) -> t l b r ({top}px, {left}px, {bottom}px, {right}px)");
                    debug!(
                        "tl pixel: {}, br pixel: {}",
                        topleft_pixel, bottomright_pixel
                    );

                    match data.tile_entities.entry((level, x, y)) {
                        Entry::Occupied(o) => {
                            debug!("update");
                            let Ok((mut style, mut vis)) = styles.get_mut(*o.get()) else {
                                warn!("missing tile entity");
                                continue;
                            };

                            style.left = Val::Px(left);
                            style.right = Val::Px(right);
                            style.top = Val::Px(top);
                            style.bottom = Val::Px(bottom);
                            *vis = Visibility::Visible;
                        }
                        Entry::Vacant(v) => {
                            debug!("new");
                            let image_path = IpfsPath::new_from_url(
                                &format!(
                                    "https://genesis.city/map/latest/{}/{},{}.jpg",
                                    level + 1,
                                    x,
                                    y
                                ),
                                "jpg",
                            );
                            let h_image = asset_server.load::<Image>(PathBuf::from(&image_path));

                            let tile_entity = commands
                                .spawn(ImageBundle {
                                    style: Style {
                                        position_type: PositionType::Absolute,
                                        left: Val::Px(left),
                                        right: Val::Px(right),
                                        top: Val::Px(top),
                                        bottom: Val::Px(bottom),
                                        ..Default::default()
                                    },
                                    image: UiImage::new(h_image),
                                    z_index: ZIndex::Local(level as i32 + 1),
                                    ..Default::default()
                                })
                                .id();

                            commands.entity(map_entity).try_push_children(&[tile_entity]);

                            v.insert(tile_entity);
                        }
                    };
                }
            }
        }

        if let Some(new_data) = new_data {
            commands.entity(map_entity).try_insert(new_data);
        }
    }
}

fn handle_map_task(
    mut q: Query<&mut MapSettings>,
    mut commands: Commands,
    dui: Res<DuiRegistry>,
    asset_server: Res<AssetServer>,
) {
    for mut settings in q.iter_mut() {
        if let Some((coords, mut task)) = settings.task.take() {
            match task.complete() {
                Some(Ok(mut pages)) => {
                    let page = pages.data.pop().unwrap_or_else(|| DiscoverPage::dummy(coords));
                    spawn_discover_popup(&mut commands, &dui, &asset_server, &page);
                }
                Some(Err(e)) => warn!("places task error: {e}"),
                None => settings.task = Some((coords, task)),
            }
        }
    }
}
