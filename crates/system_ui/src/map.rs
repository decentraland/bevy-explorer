use std::path::PathBuf;

use anyhow::anyhow;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    platform::collections::{hash_map::Entry, HashMap},
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    structs::{PrimaryUser, SettingsTab},
    util::{ModifyComponentExt, TaskCompat, TaskExt, TryPushChildrenEx},
};
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};
use scene_runner::{initialize_scene::PARCEL_SIZE, vec3_to_parcel};
use ui_core::{
    bound_node::{BoundedNode, BoundedNodeBundle},
    text_size::FontSize,
    ui_actions::{ClickNoDrag, DragData, Dragged, MouseWheelData, MouseWheeled, On, UiCaller},
};

use crate::{
    discover::{spawn_discover_popup, DiscoverPage, DiscoverPages},
    profile::SettingsDialog,
};

#[derive(Component)]
pub struct MapTexture {
    pub center: Vec2,
    pub parcels_per_vmin: f32,
    pub icon_min_size_vmin: f32,
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                set_map_content,
                touch_map,
                render_map,
                update_map_data,
                handle_map_task,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct MapData {
    cursor: Entity,
    you_are_here: Entity,
    pub pixels_per_parcel: f32,
    bottom_left_offset: Vec2,
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
            .unwrap_or(Vec2::ZERO)
            - Vec2::Y; // no idea why i have to subtract one :(

        debug!("using parcel {}", center);

        commands.entity(components.named("map-node")).insert((
            MapTexture {
                center,
                parcels_per_vmin: 20.0,
                icon_min_size_vmin: 0.05,
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
                    let window = Vec2::new(window.width(), window.height());

                    let vw = window.x / window.min_element();
                    let vh = window.y / window.min_element();

                    let parcel_delta_x = drag.delta_viewport.x * map.parcels_per_vmin * vw;
                    let parcel_delta_y = -drag.delta_viewport.y * map.parcels_per_vmin * vh;

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

                    let adj = 1.01f32.powf(-wheel.wheel);
                    map.parcels_per_vmin = (map.parcels_per_vmin * adj).clamp(10.0, 500.0);
                    let pixels_per_parcel =
                        (window.width().min(window.height()) / map.parcels_per_vmin).round();

                    map.center = cursor_parcel - cursor_rel_position / pixels_per_parcel;
                    map.center = map.center.clamp(Vec2::splat(-152.0), Vec2::splat(152.0));
                },
            ),
            On::<ClickNoDrag>::new(
                |caller: Res<UiCaller>,
                 mut q: Query<&mut MapSettings>,
                 window: Query<&Window, With<PrimaryWindow>>,
                 map: Query<(&GlobalTransform, &MapTexture, &MapData)>,
                 ipfas: IpfsAssetServer| {
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

                    let client = ipfas.ipfs().client();
                    settings.task = Some((
                        parcel,
                        IoTaskPool::get().spawn_compat(async move {
                            debug!("url: {url}");
                            let response = client.get(url).send().await?;
                            response
                                .json::<DiscoverPages>()
                                .await
                                .map_err(|e| anyhow!(e))
                        }),
                    ));
                },
            ),
        ));
    }
}

fn update_map_data(
    mut commands: Commands,
    map: Query<(&GlobalTransform, &MapTexture, &MapData, &Interaction)>,
    window: Query<&Window, With<PrimaryWindow>>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    children: Query<&Children>,
    mut text: Query<&mut Text>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    for (gt, map, data, interaction) in map.iter() {
        // update you are here
        if let Ok(gt) = player.get_single() {
            let icon_pos = data.bottom_left_offset
                + (((gt.translation().xz() * Vec2::new(1.0, -1.0)) / PARCEL_SIZE)
                    - data.min_parcel.as_vec2())
                    * data.pixels_per_parcel;

            let icon_size = (window.width().min(window.height()) * map.icon_min_size_vmin)
                .max(data.pixels_per_parcel);

            commands.entity(data.you_are_here).try_insert((
                Visibility::Inherited,
                Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(icon_pos.x - icon_size * 0.5),
                    bottom: Val::Px(icon_pos.y - data.pixels_per_parcel),
                    width: Val::Px(icon_size),
                    height: Val::Px(icon_size),
                    ..Default::default()
                },
            ));
        }

        // update cursor
        if interaction == &Interaction::None {
            commands.entity(data.cursor).try_insert(Visibility::Hidden);
            continue;
        }
        let cursor_position = window.cursor_position().unwrap_or_default();
        let cursor_rel_position =
            (cursor_position - gt.translation().truncate()) * Vec2::new(1.0, -1.0);
        let cursor_parcel = map.center + cursor_rel_position / data.pixels_per_parcel;
        let parcel = cursor_parcel.floor().as_ivec2();

        let bottomleft_pixel =
            data.bottom_left_offset + data.pixels_per_parcel * (parcel - data.min_parcel).as_vec2();
        let ppp = data.pixels_per_parcel;

        debug!("center: {}, min parcel: {}, cursor rel position: {}, cursor parcel: {}, blo: {}, blp: {}", map.center, data.min_parcel, cursor_rel_position, parcel, data.bottom_left_offset, bottomleft_pixel);

        commands
            .entity(data.cursor)
            .modify_component(move |style: &mut Style| {
                style.position_type = PositionType::Absolute;
                style.left = Val::Px(bottomleft_pixel.x);
                style.bottom = Val::Px(bottomleft_pixel.y);
                style.width = Val::Px(ppp);
                style.height = Val::Px(ppp);
            })
            .try_insert(Visibility::Inherited);

        if let Some(mut text) = children
            .get(data.cursor)
            .ok()
            .and_then(|c| c.first())
            .and_then(|c| text.get_mut(*c).ok())
        {
            text.sections[0].value = format!("({},{})", parcel.x, parcel.y + 1);
        }
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

#[allow(clippy::type_complexity)]
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
                    .spawn(BoundedNodeBundle {
                        z_index: ZIndex::Local(8),
                        bounded: BoundedNode {
                            image: Some(asset_server.load("images/cursor.png")),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            TextBundle {
                                style: Style {
                                    position_type: PositionType::Absolute,
                                    bottom: Val::Percent(100.0),
                                    left: Val::Percent(100.0),
                                    ..Default::default()
                                },
                                text: Text::from_section("Hello!", Default::default()),
                                ..Default::default()
                            },
                            FontSize(0.03),
                        ));
                    })
                    .id();
                let you_are_here = commands
                    .spawn(BoundedNodeBundle {
                        z_index: ZIndex::Local(7),
                        bounded: BoundedNode {
                            image: Some(asset_server.load("images/you_are_here.png")),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .id();

                commands
                    .entity(map_entity)
                    .try_push_children(&[cursor, you_are_here]);
                new_data = Some(MapData {
                    cursor,
                    you_are_here,
                    pixels_per_parcel: 0.0,
                    tile_entities: Default::default(),
                    bottom_left_offset: Default::default(),
                    min_parcel: Default::default(),
                    max_parcel: Default::default(),
                });
                new_data.as_mut().unwrap()
            }
        };

        let pixels_per_parcel =
            (window.width().min(window.height()) / map.parcels_per_vmin).round();
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

        let center = map.center;

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
            if let Ok((_, mut vis)) = styles.get_mut(*entity) {
                *vis = Visibility::Hidden;
            }
        }

        let parcels_to_bottomleft = data.min_parcel.as_vec2() - center;
        let pixels_to_bottomleft = parcels_to_bottomleft * pixels_per_parcel;
        data.bottom_left_offset = (node.size() * 0.5 + pixels_to_bottomleft).floor();

        debug!(
            "parcels to bl: {}, pixels to bl: {}, bl: {}",
            parcels_to_bottomleft, pixels_to_bottomleft, data.bottom_left_offset
        );

        for (level, &tile_parcels) in TILE_PARCELS.iter().enumerate().take(max_level + 1) {
            // .take(max_level + 1) {
            // in x, the tile images start at -152 and go up by tile_parcels per step
            // eg. level=0, x=0 -> parcels[-152, 8]
            // eg. level=1, x=1 -> parcels[-72, 8]
            for tile_img_x in ((data.min_parcel.x + 152) / tile_parcels)
                ..=((data.max_parcel.x + 152) / tile_parcels)
            {
                // in y, tile images start at 152 and go down by tile_parcels per step
                // eg. level=0 y=0 -> parcels[-8, 152]
                // eg. level=1 y=1 -> parcels[-8, 72]
                for tile_img_y in ((152 - data.max_parcel.y) / tile_parcels)
                    ..=((152 - data.min_parcel.y) / tile_parcels)
                {
                    let bottomleft_parcel = IVec2::new(
                        tile_img_x * tile_parcels - 152,
                        152 - (tile_img_y + 1) * tile_parcels,
                    );

                    let bottomleft_pixel = data.bottom_left_offset
                        + (bottomleft_parcel - data.min_parcel).as_vec2() * pixels_per_parcel;

                    let left = bottomleft_pixel.x;
                    let bottom = bottomleft_pixel.y;

                    debug!("level: {level}, img({tile_img_x},{tile_img_y}) bl parcel: ({bottomleft_parcel}) -> bl ({left}px, {bottom}px), sz: {}", pixels_per_parcel * tile_parcels as f32);

                    match data.tile_entities.entry((level, tile_img_x, tile_img_y)) {
                        Entry::Occupied(o) => {
                            debug!("update");
                            let Ok((mut style, mut vis)) = styles.get_mut(*o.get()) else {
                                warn!("missing tile entity");
                                continue;
                            };

                            style.left = Val::Px(left);
                            style.bottom = Val::Px(bottom);
                            style.width = Val::Px(pixels_per_parcel * tile_parcels as f32);
                            style.height = Val::Px(pixels_per_parcel * tile_parcels as f32);
                            *vis = Visibility::Inherited;
                        }
                        Entry::Vacant(v) => {
                            debug!("new");
                            let image_path = IpfsPath::new_from_url_uncached(
                                &format!(
                                    "https://genesis.city/map/latest/{}/{},{}.jpg",
                                    level + 1,
                                    tile_img_x,
                                    tile_img_y
                                ),
                                "jpg",
                            );
                            let h_image = asset_server.load::<Image>(PathBuf::from(&image_path));

                            let tile_entity = commands
                                .spawn(BoundedNodeBundle {
                                    style: Style {
                                        position_type: PositionType::Absolute,
                                        left: Val::Px(left),
                                        bottom: Val::Px(bottom),
                                        width: Val::Px(pixels_per_parcel * tile_parcels as f32),
                                        height: Val::Px(pixels_per_parcel * tile_parcels as f32),
                                        ..Default::default()
                                    },
                                    bounded: BoundedNode {
                                        image: Some(h_image),
                                        color: None,
                                    },
                                    z_index: ZIndex::Local(level as i32 + 1),
                                    ..Default::default()
                                })
                                .id();

                            commands
                                .entity(map_entity)
                                .try_push_children(&[tile_entity]);

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
                    let page = pages
                        .data
                        .pop()
                        .unwrap_or_else(|| DiscoverPage::dummy(coords));
                    spawn_discover_popup(&mut commands, &dui, &asset_server, &page);
                }
                Some(Err(e)) => warn!("places task error: {e}"),
                None => settings.task = Some((coords, task)),
            }
        }
    }
}
