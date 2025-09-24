use avatar::mask_material::MaskMaterial;
use bevy::{
    diagnostic::FrameCount,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    math::Vec3Swizzles,
    platform::{collections::HashSet, hash::FixedHasher},
    prelude::*,
    render::mesh::Indices,
    text::JustifyText,
    ui::FocusPolicy,
};

use bevy_console::ConsoleCommand;
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps, DuiRegistry};
use common::{
    sets::{SceneSets, SetupSets},
    structs::{
        AppConfig, CursorLocks, PreviewCommand, PrimaryUser, SettingsTab, ShowSettingsEvent,
        SystemScene, Version, ZOrder,
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
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(48.5),
                    top: Val::Percent(48.5),
                    right: Val::Percent(48.5),
                    bottom: Val::Percent(48.5),
                    align_content: AlignContent::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                ZOrder::Crosshair.default(),
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        width: Val::VMin(3.0),
                        height: Val::VMin(3.0),
                        ..Default::default()
                    },
                    ImageNode::new(asset_server.load("images/crosshair.png"))
                        .with_color(Color::srgba(1.0, 1.0, 1.0, 0.7)),
                    CrossHair,
                ));
            });

        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::VMin(2.0),
                bottom: Val::VMin(2.0),
                ..Default::default()
            },
            Text::new(format!("Version: {}", version.0)),
            BODY_TEXT_STYLE.get().unwrap().clone(),
        ));
    });

    commands.entity(root.0).with_children(|commands| {
        commands
            .spawn((
                Node {
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
                BackgroundColor(Color::srgba(0.8, 0.8, 1.0, 0.8)),
                FocusPolicy::Block,
                SysInfoContainer,
            ))
            .with_children(|commands| {
                commands.spawn((
                    Text::new("System Info"),
                    TITLE_TEXT_STYLE.get().unwrap().clone(),
                ));

                commands
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            ..Default::default()
                        },
                        SysInfoMarker,
                    ))
                    .with_children(|commands| {
                        let mut info_node = |label: String| {
                            commands.spawn(Node::default()).with_children(|commands| {
                                commands.spawn((
                                    Node {
                                        width: Val::Px(150.0),
                                        ..Default::default()
                                    },
                                    Text::new(label),
                                    TextLayout::new_with_justify(JustifyText::Right),
                                    BODY_TEXT_STYLE.get().unwrap().clone(),
                                ));
                                commands.spawn((
                                    Node {
                                        width: Val::Px(250.0),
                                        ..Default::default()
                                    },
                                    Text::default(),
                                    BODY_TEXT_STYLE.get().unwrap().clone(),
                                ));
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
    mut style: Query<&mut Node>,
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
    let tick = (time.elapsed_secs() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    let Ok((player, pos)) = player.single() else {
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

    if let Ok(sysinfo) = q.single() {
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
            text.0 = value;
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

        set_child(format!("{loading}"));
        set_child(format!("{running}"));
        set_child(format!("{blocked}"));
        set_child(format!("{broken}"));

        set_child(format!("{transports}"));
        set_child(format!("{players}"));

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

    commands
        .entity(components.root)
        .insert((Minimap, ZOrder::Minimap.default()));
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
                        let Ok(player) = player.single() else {
                            return;
                        };
                        let Some(scene) = containing_scene.get_parcel_oow(player) else {
                            return;
                        };
                        let Ok(scene) = scenes.get(scene) else {
                            return;
                        };
                        test_data.inspect_hash = Some(scene.hash.clone());
                        reload.write(PreviewCommand::ReloadScene { hash: scene.hash.clone() });
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
    let Ok((player, gt)) = player.single() else {
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

    if let Ok(components) = q.single() {
        if let Ok(mut map) = maps.get_mut(components.named("map-node")) {
            map.center = map_center;
        }

        if let Ok(mut text) = text.get_mut(components.named("title")) {
            text.0 = title;
        }

        if let Ok(mut text) = text.get_mut(components.named("position")) {
            text.0 = format!("({},{})   {sdk}   {state}", parcel.x, parcel.y);
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
    mesh_handles: Query<(&Mesh3d, &ContainerEntity, &Visibility)>,
    material_handles: Query<(
        &MeshMaterial3d<SceneMaterial>,
        &ContainerEntity,
        &Visibility,
    )>,
    scene_entities: Query<&ContainerEntity>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<SceneMaterial>>,
    diagnostics: Res<DiagnosticsStore>,
    images: Res<Assets<Image>>,
) {
    let Ok((tracker, entities)) = q.single_mut() else {
        return;
    };

    if !f.0.is_multiple_of(100) && !tracker.is_changed() {
        return;
    }

    commands
        .entity(entities.named("content"))
        .despawn_related::<Children>();

    if !tracker.0 {
        return;
    }

    let Ok(player) = player.single() else {
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
            .meshes_by_hash
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
        .map(|(h, ..)| &h.0)
        .collect::<HashSet<_, FixedHasher>>();

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
        .flat_map(|h| images.get(h.id()).and_then(|t| t.data.as_ref()))
        .map(|data| data.len())
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
                    .with_prop("value", format!("{value}")),
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
            .entity(q.single().unwrap())
            .modify_component(move |style: &mut Node| {
                style.display = if on { Display::Flex } else { Display::None };
            });
        input.reply_ok("");
    }
}

fn update_map_visibilty(
    realm: Res<CurrentRealm>,
    map: Query<&DuiEntities, With<Minimap>>,
    mut style: Query<&mut Node>,
    mut init: Local<bool>,
) {
    if !*init || realm.is_changed() {
        let Ok(nodes) = map.single() else {
            return;
        };
        let Ok(mut style) = style.get_mut(nodes.named("map-container")) else {
            return;
        };
        *init = true;
        // todo this is really bad
        if realm.about_url.ends_with("decentraland.org/main/about") {
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
    ui_nodes: Query<(), With<ComputedNode>>,
    scene_mats: Query<&MeshMaterial3d<SceneMaterial>>,
    std_mats: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
    mask_mats: Query<(), With<MeshMaterial3d<MaskMaterial>>>,
    uv_mats: Query<(), With<MaterialNode<StretchUvMaterial>>>,
    bound_mats: Query<(), With<MaterialNode<BoundedImageMaterial>>>,
    textshape_mats: Query<(), With<MeshMaterial3d<TextShapeMaterial>>>,
    mats: Res<Assets<SceneMaterial>>,
) {
    if f.0.is_multiple_of(100) {
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

    if !track.0 || f.0.is_multiple_of(100) {
        return;
    }

    let Ok(player) = player.single() else {
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
    locks: Res<CursorLocks>,
    mut prev: Local<Option<bool>>,
    mut crosshair: Query<&mut ImageNode, With<CrossHair>>,
) {
    let locked = locks.0.contains("Camera");
    if Some(locked) != *prev {
        let Ok(mut img) = crosshair.single_mut() else {
            return;
        };
        *prev = Some(locked);
        if locked {
            img.color.set_alpha(0.7);
        } else {
            img.color.set_alpha(0.2);
        }
    }
}
