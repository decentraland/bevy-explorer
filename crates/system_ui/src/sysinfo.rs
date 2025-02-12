use avatar::mask_material::MaskMaterial;
use bevy::{
    core::FrameCount,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    math::Vec3Swizzles,
    prelude::*,
    render::mesh::Indices,
    text::JustifyText,
    ui::FocusPolicy,
    utils::hashbrown::HashSet,
};

use bevy_console::ConsoleCommand;
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    sets::{SceneSets, SetupSets},
    structs::{
        AppConfig, CursorLocked, PreviewCommand, PrimaryUser, SettingsTab, ShowSettingsEvent,
        SystemScene, Version,
    },
    util::ModifyComponentExt,
};
use comms::{global_crdt::ForeignPlayer, preview::PreviewMode, Transport};
use console::DoAddConsoleCommand;
use ipfs::CurrentRealm;
use scene_material::{SceneMaterial, SCENE_MATERIAL_OUTLINE};
use scene_runner::{
    initialize_scene::{SceneLoading, TestingData, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    update_world::{
        gltf_container::{GltfLoadingCount, SceneResourceLookup},
        ComponentTracker, TrackComponents,
    },
    ContainerEntity, ContainingScene, DebugInfo, Toaster,
};
use ui_core::{
    bound_node::BoundedImageMaterial,
    stretch_uvs_image::StretchUvMaterial,
    text_size::update_fontsize,
    ui_actions::{Click, EventCloneExt, On},
    BODY_TEXT_STYLE, TITLE_TEXT_STYLE,
};
use world_ui::TextShapeMaterial;

use crate::map::MapTexture;

use super::SystemUiRoot;

pub struct SysInfoPanelPlugin;

impl Plugin for SysInfoPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorLocked>();
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Main).after(SetupSets::Init),
        );
        app.add_systems(
            Update,
            (
                update_scene_load_state,
                update_minimap,
                update_tracker,
                update_map_visibilty,
                update_crosshair,
            )
                .before(update_fontsize)
                .after(SceneSets::PostLoop),
        );
        app.add_systems(
            OnEnter::<ui_core::State>(ui_core::State::Ready),
            setup_minimap,
        );
        app.add_console_command::<SysinfoCommand, _>(set_sysinfo);

        app.add_systems(First, (entity_count, display_tracked_components));
        app.add_console_command::<TrackComponentCommand, _>(set_track_components);
    }
}

#[derive(Component)]
struct SysInfoMarker;

#[derive(Component)]
struct SysInfoContainer;

#[derive(Component)]
struct CrossHair;

pub(crate) fn setup(
    mut commands: Commands,
    root: Res<SystemUiRoot>,
    config: Res<AppConfig>,
    asset_server: Res<AssetServer>,
    version: Res<Version>,
) {
    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(48.5),
                    top: Val::Percent(48.5),
                    right: Val::Percent(48.5),
                    bottom: Val::Percent(48.5),
                    align_content: AlignContent::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                z_index: ZIndex::Global(i16::MIN as i32 - 1),
                ..Default::default()
            })
            .with_children(|c| {
                c.spawn((
                    ImageBundle {
                        style: Style {
                            width: Val::VMin(3.0),
                            height: Val::VMin(3.0),
                            ..Default::default()
                        },
                        image: UiImage {
                            color: Color::srgba(1.0, 1.0, 1.0, 0.7),
                            texture: asset_server.load("images/crosshair.png"),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    CrossHair,
                ));
            });
        commands.spawn(TextBundle {
            style: Style {
                position_type: PositionType::Absolute,
                right: Val::VMin(2.0),
                bottom: Val::VMin(2.0),
                ..Default::default()
            },
            text: Text::from_section(
                format!("Version: {}", version.0),
                BODY_TEXT_STYLE.get().unwrap().clone(),
            ),
            ..Default::default()
        });
    });

    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        right: Val::Px(40.0),
                        top: Val::Px(0.0),
                        display: if config.sysinfo_visible {
                            Display::Flex
                        } else {
                            Display::None
                        },
                        flex_direction: FlexDirection::Column,
                        align_self: AlignSelf::FlexStart,
                        border: UiRect::all(Val::Px(5.)),
                        ..default()
                    },
                    background_color: Color::srgba(0.8, 0.8, 1.0, 0.8).into(),
                    focus_policy: FocusPolicy::Block,
                    z_index: ZIndex::Global(i16::MAX as i32 + 3),
                    ..default()
                },
                SysInfoContainer,
            ))
            .with_children(|commands| {
                commands.spawn(TextBundle::from_section(
                    "System Info",
                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                ));

                commands
                    .spawn((
                        NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        SysInfoMarker,
                    ))
                    .with_children(|commands| {
                        let mut info_node = |label: String| {
                            commands
                                .spawn(NodeBundle::default())
                                .with_children(|commands| {
                                    commands.spawn(TextBundle {
                                        style: Style {
                                            width: Val::Px(150.0),
                                            ..Default::default()
                                        },
                                        text: Text::from_section(
                                            label,
                                            BODY_TEXT_STYLE.get().unwrap().clone(),
                                        )
                                        .with_justify(JustifyText::Right),
                                        ..Default::default()
                                    });
                                    commands.spawn(TextBundle {
                                        style: Style {
                                            width: Val::Px(250.0),
                                            ..Default::default()
                                        },
                                        text: Text::from_section(
                                            "",
                                            BODY_TEXT_STYLE.get().unwrap().clone(),
                                        ),
                                        ..Default::default()
                                    });
                                });
                        };

                        if config.graphics.log_fps {
                            info_node("FPS :".to_owned());
                        }

                        info_node("Current Parcel :".to_owned());
                        info_node("Current Scene :".to_owned());
                        info_node("Scene State :".to_owned());
                        info_node("Loading Scenes :".to_owned());
                        info_node("Running Scenes :".to_owned());
                        info_node("Blocked Scenes :".to_owned());
                        info_node("Broken Scenes :".to_owned());
                        info_node("Transports :".to_owned());
                        info_node("Players :".to_owned());
                        info_node("Debug info :".to_owned());
                    });
            });
    });
}

#[derive(Component)]
pub struct Minimap;

#[derive(Component)]
pub struct Tracker(bool);

#[allow(clippy::too_many_arguments)]
fn update_scene_load_state(
    q: Query<Entity, With<SysInfoMarker>>,
    q_children: Query<&Children>,
    mut text: Query<&mut Text>,
    mut style: Query<&mut Style>,
    loading_scenes: Query<&SceneLoading>,
    running_scenes: Query<&RendererSceneContext, Without<SceneLoading>>,
    transports: Query<&Transport>,
    players: Query<&ForeignPlayer>,
    config: Res<AppConfig>,
    mut last_update: Local<u32>,
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    containing_scene: ContainingScene,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    debug_info: Res<DebugInfo>,
) {
    let tick = (time.elapsed_seconds() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    let Ok((player, pos)) = player.get_single() else {
        return;
    };
    let scene = containing_scene.get_parcel(player);
    let parcel = (pos.translation().xz() * Vec2::new(1.0, -1.0) / PARCEL_SIZE)
        .floor()
        .as_ivec2();
    let title = scene
        .and_then(|scene| running_scenes.get(scene).ok())
        .map(|context| context.title.clone())
        .unwrap_or("???".to_owned());

    if let Ok(sysinfo) = q.get_single() {
        let mut ix = 0;
        let children = q_children.get(sysinfo).unwrap();
        let mut set_child = |value: String| {
            if value.is_empty() {
                style.get_mut(children[ix]).unwrap().display = Display::None;
            } else {
                style.get_mut(children[ix]).unwrap().display = Display::Flex;
            }
            let container = q_children.get(children[ix]).unwrap();
            let mut text = text.get_mut(container[1]).unwrap();
            text.sections[0].value = value;
            ix += 1;
        };

        let state = scene.map_or("-", |scene| {
            if loading_scenes.get(scene).is_ok() {
                "Loading"
            } else if let Ok(scene) = running_scenes.get(scene) {
                if scene.broken {
                    "Broken"
                } else if !scene.blocked.is_empty() {
                    "Blocked"
                } else {
                    "Running"
                }
            } else {
                "Unknown?!"
            }
        });

        let loading = loading_scenes.iter().count();
        let running = running_scenes
            .iter()
            .filter(|context| !context.broken && context.blocked.is_empty())
            .count();
        let blocked = running_scenes
            .iter()
            .filter(|context| !context.broken && !context.blocked.is_empty())
            .count();
        let broken = running_scenes
            .iter()
            .filter(|context| context.broken)
            .count();
        let transports = transports.iter().count();
        let players = players.iter().count() + 1;

        if config.graphics.log_fps {
            if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
                let fps = fps.smoothed().unwrap_or_default();
                set_child(format!("{fps:.0}"));
            } else {
                set_child("-".to_owned());
            }
        }

        set_child(format!("({},{})", parcel.x, parcel.y));
        set_child(title);
        set_child(state.to_owned());

        set_child(format!("{}", loading));
        set_child(format!("{}", running));
        set_child(format!("{}", blocked));
        set_child(format!("{}", broken));

        set_child(format!("{}", transports));
        set_child(format!("{}", players));

        let debug_info = debug_info
            .info
            .iter()
            .fold(String::default(), |msg, (key, info)| {
                format!("{msg}\n{key}: {info}")
            });
        set_child(debug_info);
    }
}

fn setup_minimap(
    mut commands: Commands,
    root: Res<SystemUiRoot>,
    dui: Res<DuiRegistry>,
    preview: Res<PreviewMode>,
    system_scene: Option<Res<SystemScene>>,
) {
    let components = commands
        .spawn_template(&dui, "minimap", Default::default())
        .unwrap();
    commands
        .entity(root.0)
        .insert_children(0, &[components.root]);

    commands.entity(components.root).insert(Minimap);
    commands.entity(components.named("map-node")).insert((
        MapTexture {
            center: Default::default(),
            parcels_per_vmin: 100.0,
            icon_min_size_vmin: 0.03,
        },
        Interaction::default(),
        ShowSettingsEvent(SettingsTab::Map).send_value_on::<Click>(),
    ));

    if preview.server.is_some() || system_scene.as_ref().is_some_and(|ss| ss.preview) {
        let tracker = commands
            .entity(components.root)
            .spawn_template(
                &dui,
                "tracker",
                DuiProps::new().with_prop(
                    "toggle",
                    On::<Click>::new(|mut trackers: Query<&mut Tracker>| {
                        for mut tracker in trackers.iter_mut() {
                            tracker.0 = !tracker.0;
                        }
                    })
                ).with_prop(
                    "inspect",
                    On::<Click>::new(|
                        mut reload: EventWriter<PreviewCommand>,
                        mut test_data: ResMut<TestingData>,
                        containing_scene: ContainingScene,
                        scenes: Query<&RendererSceneContext>,
                        player: Query<Entity, With<PrimaryUser>>,
                        system_scene: Option<Res<SystemScene>>,
                        mut toaster: Toaster,
                    | {
                        if system_scene.as_ref().is_some_and(|ss| ss.hot_reload.is_some()) {
                            let ss = system_scene.unwrap();
                            if let Some(hash) = ss.hash.clone() {
                                test_data.inspect_hash = Some(hash.clone());
                                let _ = ss.hot_reload.as_ref().unwrap().send(PreviewCommand::ReloadScene { hash });
                                toaster.add_toast("inspector", "Please open chrome and navigate to \"chrome://inspect\" to attach a debugger to the UI scene");
                            }
                            return;
                        }
                        let Ok(player) = player.get_single() else {
                            return;
                        };
                        let Some(scene) = containing_scene.get_parcel_oow(player) else {
                            return;
                        };
                        let Ok(scene) = scenes.get(scene) else {
                            return;
                        };
                        test_data.inspect_hash = Some(scene.hash.clone());
                        reload.send(PreviewCommand::ReloadScene { hash: scene.hash.clone() });
                        toaster.add_toast("inspector", "Please open chrome and navigate to \"chrome://inspect\" to attach a debugger");
                    })
                )
                .with_prop("inspect-enabled", cfg!(feature = "inspect")),
            )
            .unwrap();
        commands.entity(tracker.root).insert(Tracker(true));
    }
}

fn update_minimap(
    q: Query<&DuiEntities, With<Minimap>>,
    mut maps: Query<&mut MapTexture>,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    scenes: Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
    mut text: Query<&mut Text>,
    preview: Res<PreviewMode>,
) {
    let Ok((player, gt)) = player.get_single() else {
        return;
    };

    let player_translation = (gt.translation().xz() * Vec2::new(1.0, -1.0)) / PARCEL_SIZE;
    let map_center = player_translation - Vec2::Y;

    let scene = containing_scene
        .get_parcel_oow(player)
        .and_then(|scene| scenes.get(scene).ok());
    let parcel = player_translation.floor().as_ivec2();
    let title = scene
        .map(|(context, _)| context.title.clone())
        .unwrap_or("???".to_owned());
    let title = if preview.is_preview {
        format!("[ Preview Mode ]\n{title}")
    } else {
        title
    };
    let sdk = scene.map(|(context, _)| context.sdk_version).unwrap_or("");
    let state = scene
        .map(|(context, gltf_count)| {
            if context.broken {
                "Broken".to_owned()
            } else if !context.blocked.is_empty() {
                format!("Loading [{}]", gltf_count.map(|c| c.0).unwrap_or_default())
            } else {
                "Ok ".to_owned()
            }
        })
        .unwrap_or("No scene".to_owned());

    if let Ok(components) = q.get_single() {
        if let Ok(mut map) = maps.get_mut(components.named("map-node")) {
            map.center = map_center;
        }

        if let Ok(mut text) = text.get_mut(components.named("title")) {
            text.sections[0].value = title;
        }

        if let Ok(mut text) = text.get_mut(components.named("position")) {
            text.sections[0].value = format!("({},{})   {sdk}   {state}", parcel.x, parcel.y);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_tracker(
    mut commands: Commands,
    mut q: Query<(Ref<Tracker>, &DuiEntities)>,
    stats: Query<&SceneResourceLookup>,
    f: Res<FrameCount>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    dui: Res<DuiRegistry>,
    mesh_handles: Query<(&Handle<Mesh>, &ContainerEntity, &Visibility)>,
    material_handles: Query<(&Handle<SceneMaterial>, &ContainerEntity, &Visibility)>,
    scene_entities: Query<&ContainerEntity>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<SceneMaterial>>,
    diagnostics: Res<DiagnosticsStore>,
    images: Res<Assets<Image>>,
) {
    let Ok((tracker, entities)) = q.get_single_mut() else {
        return;
    };

    if f.0 % 100 != 0 && !tracker.is_changed() {
        return;
    }

    commands
        .entity(entities.named("content"))
        .despawn_descendants();

    if !tracker.0 {
        return;
    }

    let Ok(player) = player.get_single() else {
        return;
    };

    let scenes = containing_scene.get(player);
    let Some(scene) = scenes.iter().next() else {
        return;
    };

    let Ok(resource_lookup) = stats.get(*scene) else {
        return;
    };

    let mut display_data = Vec::default();

    if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
        display_data.push(("FPS", fps.smoothed().unwrap_or_default() as usize));
    }

    display_data.push((
        "Unique Gltf Meshes",
        resource_lookup
            .meshes
            .values()
            .filter(|c| meshes.get(c.mesh_id).is_some())
            .count(),
    ));
    display_data.push((
        "Visible Mesh Count",
        mesh_handles
            .iter()
            .filter(|(_, c, v)| &c.root == scene && !matches!(v, Visibility::Hidden))
            .count(),
    ));
    display_data.push((
        "Visible Triangle Count",
        mesh_handles
            .iter()
            .filter(|(_, c, v)| &c.root == scene && !matches!(v, Visibility::Hidden))
            .flat_map(|(h, _, _)| meshes.get(h.id()))
            .map(|mesh| {
                mesh.indices()
                    .map(Indices::len)
                    .unwrap_or_else(|| mesh.attributes().next().unwrap().1.len())
                    / 3
            })
            .sum(),
    ));

    display_data.push((
        "Unique Gltf Materials",
        resource_lookup
            .materials
            .values()
            .filter(|h| materials.get(h.id()).is_some())
            .count(),
    ));

    let visible_mats = material_handles
        .iter()
        .filter(|(_, c, v)| &c.root == scene && !matches!(v, Visibility::Hidden))
        .map(|(h, ..)| h)
        .collect::<HashSet<_>>();

    display_data.push(("Visible Material Count", visible_mats.len()));

    let textures = visible_mats
        .iter()
        .flat_map(|h| materials.get(h.id()))
        .flat_map(|mat| {
            mat.base
                .base_color_texture
                .iter()
                .chain(mat.base.emissive_texture.as_ref())
                .chain(mat.base.normal_map_texture.as_ref())
        })
        .collect::<HashSet<_>>();

    display_data.push(("Total Texture Count", textures.len()));

    let total_memory = textures
        .iter()
        .flat_map(|h| images.get(h.id()))
        .map(|t| t.data.len())
        .sum::<usize>();
    let total_mb = (total_memory as f32 / 1024.0 / 1024.0).round() as usize;

    display_data.push(("Total Texture Memory (mb)", total_mb));

    display_data.push((
        "Total Entities",
        scene_entities.iter().filter(|c| &c.root == scene).count(),
    ));

    for (key, value) in display_data {
        commands
            .entity(entities.named("content"))
            .spawn_template(
                &dui,
                "tracker-item",
                DuiProps::new()
                    .with_prop("label", key.to_string())
                    .with_prop("value", format!("{}", value)),
            )
            .unwrap();
    }
}

// set fps
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/sysinfo")]
struct SysinfoCommand {
    on: Option<bool>,
}

fn set_sysinfo(
    mut commands: Commands,
    mut input: ConsoleCommand<SysinfoCommand>,
    q: Query<Entity, With<SysInfoContainer>>,
) {
    if let Some(Ok(command)) = input.take() {
        let on = command.on.unwrap_or(true);

        commands
            .entity(q.single())
            .modify_component(move |style: &mut Style| {
                style.display = if on { Display::Flex } else { Display::None };
            });
        input.reply_ok("");
    }
}

fn update_map_visibilty(
    realm: Res<CurrentRealm>,
    map: Query<&DuiEntities, With<Minimap>>,
    mut style: Query<&mut Style>,
    mut init: Local<bool>,
) {
    if !*init || realm.is_changed() {
        let Ok(nodes) = map.get_single() else {
            return;
        };
        let Ok(mut style) = style.get_mut(nodes.named("map-container")) else {
            return;
        };
        *init = true;
        // todo this is really bad
        if realm.address == "https://realm-provider.decentraland.org/main" {
            style.display = Display::Flex;
        } else {
            style.display = Display::None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn entity_count(
    q: Query<Entity>,
    f: Res<FrameCount>,
    meshes: Res<Assets<Mesh>>,
    textures: Res<Assets<Image>>,
    ui_nodes: Query<(), With<Node>>,
    scene_mats: Query<&Handle<SceneMaterial>>,
    std_mats: Query<(), With<Handle<StandardMaterial>>>,
    mask_mats: Query<(), With<Handle<MaskMaterial>>>,
    uv_mats: Query<(), With<Handle<StretchUvMaterial>>>,
    bound_mats: Query<(), With<Handle<BoundedImageMaterial>>>,
    textshape_mats: Query<(), With<Handle<TextShapeMaterial>>>,
    mats: Res<Assets<SceneMaterial>>,
) {
    if f.0 % 100 == 0 {
        let entities = q.iter().count();
        let meshes = meshes.iter().count();
        let textures = textures.iter().count();
        let ui_nodes = ui_nodes.iter().count();
        debug!("{entities} ents, {meshes} meshes, {textures} textures, {ui_nodes} ui nodes");

        let outlined = scene_mats
            .iter()
            .filter(|m| {
                mats.get(m.id())
                    .map(|m| (m.extension.data.flags & SCENE_MATERIAL_OUTLINE) != 0)
                    .unwrap_or(false)
            })
            .count();
        let scene_mats = scene_mats.iter().count();
        let std_mats = std_mats.iter().count();
        let mask_mats = mask_mats.iter().count();
        let uv_mats = uv_mats.iter().count();
        let bound_mats = bound_mats.iter().count();
        let textshape_mats = textshape_mats.iter().count();
        debug!("scene {scene_mats} ({outlined} outlined), std {std_mats}, mask: {mask_mats}, uv: {uv_mats}, bound: {bound_mats}, text: {textshape_mats}");
    }
}

fn display_tracked_components(
    track: Option<Res<TrackComponents>>,
    f: Res<FrameCount>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    components: Query<&ComponentTracker>,
) {
    let Some(track) = track else {
        return;
    };

    if !track.0 || f.0 % 100 != 0 {
        return;
    }

    let Ok(player) = player.get_single() else {
        return;
    };

    let scenes = containing_scene.get(player);
    for scene in scenes {
        println!("scene {:?}\n{:#?}", scene, components.get(scene));
    }
}

// set fps
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/track_components")]
struct TrackComponentCommand {
    on: Option<bool>,
}

fn set_track_components(
    mut input: ConsoleCommand<TrackComponentCommand>,
    track: Option<ResMut<TrackComponents>>,
) {
    let Some(mut track) = track else {
        return;
    };
    if let Some(Ok(command)) = input.take() {
        let on = command.on.unwrap_or(true);
        track.0 = on;
        input.reply_ok("");
    }
}

fn update_crosshair(
    locked: Res<CursorLocked>,
    mut prev: Local<Option<bool>>,
    mut crosshair: Query<&mut UiImage, With<CrossHair>>,
) {
    if Some(locked.0) != *prev {
        let Ok(mut img) = crosshair.get_single_mut() else {
            return;
        };
        *prev = Some(locked.0);
        if locked.0 {
            img.color.set_alpha(0.7);
        } else {
            img.color.set_alpha(0.2);
        }
    }
}
