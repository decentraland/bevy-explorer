//! Windowed debug viewer for the authoritative scene server (`--viewer`).
//!
//! The headless binary already links wgpu and winit — the `bevy` dependency carries a
//! fixed feature list including `bevy_render`/`bevy_winit`, and the `headless` cargo
//! feature only adds `configurable_error_handler`. Headless simply never *adds*
//! `RenderPlugin`/`WinitPlugin`. So "see what the server sees" needs no snapshot
//! protocol and no second process: swap the plugin set and the server draws its own ECS.
//!
//! This is deliberately NOT an avatar renderer. The server holds a transform, a name and
//! an address per player — nothing else — so players are drawn as capsules, the way
//! hammurabi's browser viewer draws them. Adding `AvatarPlugin` would invent detail the
//! server does not have (and would double-add `PlayerMovementPlugin`).
//!
//! Server semantics are untouched: `set_server_mode()` is latched before the app is
//! built, so all the `server_mode()` gates still apply — no position broadcast, no Pulse,
//! no local profile, server gatekeeper URL, and the fake player stays hidden from
//! `getConnectedPlayers`. Flying the camera cannot produce a ghost player, because
//! `broadcast_position` early-returns in server mode regardless of where the camera is.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, Propagate},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    log::LogPlugin,
    picking::Pickable,
    prelude::*,
    render::view::RenderLayers,
    window::{PresentMode, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use common::{
    sets::SetupSets,
    structs::{PrimaryCamera, PrimaryCameraRes, PrimaryUser, GROUND_RENDERLAYER},
    util::TryPushChildrenEx,
};
use comms::{global_crdt::ForeignPlayer, profile::UserProfile};
use ipfs::{map_realm_name, IpfsIoPlugin};
use scene_runner::renderer_context::RendererSceneContext;

/// Window title. Deliberately distinct from the client's "Decentraland Web Explorer" so
/// a viewer window is never mistaken for a client window in a screenshot or a bug report.
const WINDOW_TITLE: &str = "Decentraland Server Viewer";

const CAPSULE_RADIUS: f32 = 0.3;
const CAPSULE_HALF_LENGTH: f32 = 0.6;
/// Player transforms are feet-anchored; the capsule mesh is centred on its own origin.
const CAPSULE_Y_OFFSET: f32 = CAPSULE_RADIUS + CAPSULE_HALF_LENGTH;

const LOOK_SENSITIVITY: f32 = 0.003;
/// Free-camera cruise speed, m/s. Scenes are parcel-scale (16m) and often hundreds of
/// metres across, so this is tuned for crossing one, not for inspecting a prop — use ctrl
/// to slow down for close work.
const BASE_SPEED: f32 = 36.0;
/// Shift, on top of `BASE_SPEED`: crossing a large scene end to end.
const BOOST_MULTIPLIER: f32 = 8.0;
/// Ctrl: precision nudging.
const SLOW_MULTIPLIER: f32 = 0.25;
const PITCH_LIMIT: f32 = 1.54;

/// `DCL_SERVER_VIEWER` — the trigger `sdk-commands start` forwards (it spreads the whole
/// `process.env` into the spawned server, so no toolchain change is needed).
///
/// Leniency matches hammurabi's `parseViewerPort` so muscle memory carries over: a port
/// number like the old `HAMMURABI_DEBUG_VIEWER=8080` enables the viewer rather than being
/// rejected. There is no port to bind — the value is only ever truthy/falsy here.
pub fn env_enabled() -> bool {
    let Ok(raw) = std::env::var("DCL_SERVER_VIEWER") else {
        return false;
    };
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "0" | "false" | "no" | "off" => false,
        "1" | "true" | "yes" | "on" => true,
        // any other number (e.g. a leftover port) is truthy unless it is zero
        other => other.parse::<i64>().map(|n| n != 0).unwrap_or(true),
    }
}

/// The windowed counterpart of headless's hand-assembled render-free plugin set.
/// Mirrors `desktop_default_plugins` in `src/lib.rs`, minus the client-only pieces.
pub fn default_plugins(realm: &str, preview: bool, num_slots: usize) -> PluginGroupBuilder {
    DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: WINDOW_TITLE.to_owned(),
                // Deliberately NOT derived from AppConfig.graphics.vsync. `run_scene_loop`
                // reads that field to size the scene budget, and headless pins it to
                // `false` + `fps_target = tick_hz`; letting the window presentation mode
                // ride the same field would change the server's tick pacing just because
                // someone opened a window. Present on vblank, budget scenes at tick_hz.
                present_mode: PresentMode::AutoVsync,
                resolution: WindowResolution::new(1280.0, 720.0),
                ..Default::default()
            }),
            ..Default::default()
        })
        // main() installs LogPlugin itself, before the plugin set is chosen
        .disable::<LogPlugin>()
        .set(bevy::asset::AssetPlugin {
            // asset loads go through the ipfs module, which does its own validation
            unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
            ..Default::default()
        })
        .build()
        .add_before::<bevy::asset::AssetPlugin>(IpfsIoPlugin {
            preview,
            starting_realm: Some(map_realm_name(realm)),
            content_server_override: None,
            assets_root: None,
            num_slots,
        })
}

pub struct ViewerPlugin {
    /// Scene tick rate, used to pace app frames so the windowed server keeps headless's
    /// cadence instead of inheriting the monitor's.
    pub tick_hz: u32,
}

impl Plugin for ViewerPlugin {
    fn build(&self, app: &mut App) {
        // Fixed cadence. Every app frame runs `run_scene_loop`, so leaving winit reactive
        // would make the server tick faster whenever the mouse moved — a debug viewer must
        // not change what it is measuring. `..` in the pattern is deliberate: it keeps this
        // compiling if bevy adds another reactivity flag.
        let frame_period = Duration::from_secs_f64(1.0 / self.tick_hz.max(1) as f64);
        let paced = || {
            let mut mode = UpdateMode::reactive(frame_period);
            if let UpdateMode::Reactive {
                react_to_device_events,
                react_to_user_events,
                react_to_window_events,
                ..
            } = &mut mode
            {
                *react_to_device_events = false;
                *react_to_user_events = false;
                *react_to_window_events = false;
            }
            mode
        };
        app.insert_resource(WinitSettings {
            focused_mode: paced(),
            unfocused_mode: paced(),
        });

        // headless has no AmbientLight (nothing reads it without a render app)
        app.insert_resource(AmbientLight {
            color: Color::srgb(0.85, 0.85, 1.0),
            brightness: 575.0,
            ..Default::default()
        });

        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        app.init_resource::<ViewerState>();
        app.init_resource::<ViewerAssets>();

        // SetupSets::Init holds headless's own `setup`, which creates the player and the
        // (render-less) camera entity. Main runs after, so the entities exist here.
        app.add_systems(Startup, setup_viewer.in_set(SetupSets::Main));
        app.add_systems(
            Update,
            (
                sync_player_bodies,
                auto_follow_first_player,
                handle_viewer_keys,
                handle_row_clicks,
                rebuild_player_list,
                update_hud,
            ),
        );
        // Player transforms are written by `PlayerMovementPlugin` in Update, so the camera
        // has to run afterwards or a followed player is always one frame ahead of the
        // camera looking at them (the bug fixed for cinematic cameras in #1114).
        app.add_systems(
            PostUpdate,
            drive_camera.before(TransformSystem::TransformPropagate),
        );
        // ...and anything *reading* GlobalTransform has to run after propagation, or the
        // nametags trail the camera by a frame for the same reason.
        app.add_systems(
            PostUpdate,
            (position_labels, update_row_positions).after(TransformSystem::TransformPropagate),
        );
    }
}

// ---------------------------------------------------------------- state

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    #[default]
    Free,
    Follow(Entity),
}

#[derive(Resource)]
pub struct ViewerState {
    pub mode: CameraMode,
    yaw: f32,
    pitch: f32,
    distance: f32,
    position: Vec3,
    /// persistent fly-speed multiplier (free camera wheel)
    speed_scale: f32,
    /// one-shot: jump to the first player that connects, so the viewer does not open
    /// staring at empty space (the fake player sits at the base parcel centre, which for
    /// a large scene is nowhere near where players actually are)
    auto_followed: bool,
    roster_hash: u64,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            mode: CameraMode::Free,
            yaw: 0.0,
            pitch: -0.35,
            distance: 8.0,
            position: Vec3::new(8.0, 20.0, 8.0),
            speed_scale: 1.0,
            auto_followed: false,
            roster_hash: u64::MAX,
        }
    }
}

#[derive(Resource, Default)]
struct ViewerAssets {
    capsule: Handle<Mesh>,
    server: Handle<StandardMaterial>,
    peer: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
}

/// On a player entity: its debug body has been spawned.
#[derive(Component)]
struct HasViewerBody;

/// Players — remote or the server's own fake one — that still need a debug body.
type BodylessPlayers = (
    Or<(With<ForeignPlayer>, With<PrimaryUser>)>,
    Without<HasViewerBody>,
);

/// On the capsule child: which player it belongs to, and the colour it reverts to when it
/// is not the current selection. Carrying the base handle here (rather than re-deriving it
/// from the live material) keeps deselection correct for the green server player too.
#[derive(Component)]
struct ViewerBody {
    player: Entity,
    base: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PlayerListRoot;

#[derive(Component)]
struct PlayerRow(Entity);

#[derive(Component)]
struct HudText;

#[derive(Component)]
struct LabelRoot;

#[derive(Component)]
struct WorldLabel(Entity);

/// The second line of a player row. The roster is only rebuilt when its *membership*
/// changes, so positions — the thing you actually came here to read — are refreshed in
/// place by `update_row_positions` rather than by respawning the list every frame.
#[derive(Component)]
struct RowPosition {
    player: Entity,
    prefix: String,
}

// ---------------------------------------------------------------- setup

fn setup_viewer(
    mut commands: Commands,
    mut assets: ResMut<ViewerAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<ViewerState>,
    cam_res: Res<PrimaryCameraRes>,
    player: Single<(Entity, &Transform), With<PrimaryUser>>,
) {
    let (player_ent, player_transform) = player.into_inner();

    assets.capsule = meshes.add(Mesh::from(Capsule3d::new(
        CAPSULE_RADIUS,
        CAPSULE_HALF_LENGTH * 2.0,
    )));
    assets.server = materials.add(unlit(Color::srgb(0.2, 0.9, 0.35)));
    assets.peer = materials.add(unlit(Color::srgb(0.25, 0.55, 1.0)));
    assets.selected = materials.add(unlit(Color::srgb(1.0, 0.85, 0.1)));

    // Start looking at the server's fake player rather than the world origin.
    state.position = player_transform.translation + Vec3::new(0.0, 12.0, 12.0);

    // headless spawns the player without render-layer propagation (nothing renders); the
    // client does, and the scene's own entities expect it.
    commands
        .entity(player_ent)
        .insert(Propagate(RenderLayers::default()));

    // headless's `setup` already made this entity with `PrimaryCamera` + Transform, so the
    // scene runtime's camera plumbing (ParentPositionSync targets, push_camera_fov_to_crdt)
    // keeps working — we only add what the render app needs. Same bundle as src/lib.rs,
    // minus its `NewCameraEvent`: that event is registered by SettingBridgePlugin, which
    // `SystemBridgePlugin { bare: true }` deliberately skips, so sending it here would fire
    // an unregistered event. It only applies user graphics settings, which a debug viewer
    // has no business reading anyway.
    commands.entity(cam_res.0).insert((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..Default::default()
        },
        platform::default_camera_components(),
        Projection::from(PerspectiveProjection {
            far: 100000.0,
            ..Default::default()
        }),
        GROUND_RENDERLAYER.with(0),
    ));

    spawn_ui(&mut commands);
}

/// Debug bodies are markers, not lighting studies — unlit keeps them legible against any
/// scene and at any time of day.
fn unlit(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        unlit: true,
        ..Default::default()
    }
}

fn spawn_ui(commands: &mut Commands) {
    commands.spawn((
        LabelRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..Default::default()
        },
        Pickable::IGNORE,
    ));

    commands.spawn((
        HudText,
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..Default::default()
        },
        TextColor(Color::srgb(0.9, 0.9, 0.9)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            bottom: Val::Px(8.0),
            ..Default::default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
    ));

    commands.spawn((
        PlayerListRoot,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            width: Val::Px(280.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(6.0)),
            row_gap: Val::Px(2.0),
            ..Default::default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
    ));
}

// ---------------------------------------------------------------- bodies

fn sync_player_bodies(
    mut commands: Commands,
    assets: Res<ViewerAssets>,
    state: Res<ViewerState>,
    new_players: Query<(Entity, Option<&ForeignPlayer>), BodylessPlayers>,
    mut bodies: Query<(&ViewerBody, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (player, foreign) in &new_players {
        let base = if foreign.is_some() {
            assets.peer.clone()
        } else {
            assets.server.clone()
        };
        // try_ variant: a player can disconnect between this command being queued and
        // flushed, and the plain builder panics on a missing parent
        commands
            .entity(player)
            .try_insert(HasViewerBody)
            .try_with_children(|p| {
                p.spawn((
                    ViewerBody {
                        player,
                        base: base.clone(),
                    },
                    Mesh3d(assets.capsule.clone()),
                    MeshMaterial3d(base),
                    Transform::from_xyz(0.0, CAPSULE_Y_OFFSET, 0.0),
                ));
            });
    }

    // Selection highlight. Cheap enough to reassert every frame for the handful of players
    // a preview ever has, and it self-heals if a body is respawned.
    let selected = match state.mode {
        CameraMode::Follow(e) => Some(e),
        CameraMode::Free => None,
    };
    for (body, mut material) in &mut bodies {
        let wanted = if Some(body.player) == selected {
            &assets.selected
        } else {
            &body.base
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

// ---------------------------------------------------------------- camera

fn auto_follow_first_player(
    mut state: ResMut<ViewerState>,
    players: Query<Entity, With<ForeignPlayer>>,
) {
    if state.auto_followed {
        return;
    }
    if let Some(first) = players.iter().next() {
        state.auto_followed = true;
        state.mode = CameraMode::Follow(first);
        info!("[viewer] following {first:?} (first player to connect)");
    }
}

fn handle_viewer_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ViewerState>,
    players: Query<(Entity, &ForeignPlayer)>,
    transforms: Query<&Transform>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        state.mode = CameraMode::Free;
    }

    if keys.just_pressed(KeyCode::Tab) {
        // Sorted by address, exactly like the on-screen list: query iteration order is
        // archetype order, so cycling would otherwise not match what the user is reading.
        let mut roster: Vec<(Entity, String)> = players
            .iter()
            .map(|(entity, foreign)| (entity, format!("{:#x}", foreign.address)))
            .collect();
        roster.sort_by(|a, b| a.1.cmp(&b.1));
        let roster: Vec<Entity> = roster.into_iter().map(|(entity, _)| entity).collect();
        if !roster.is_empty() {
            let next = match state.mode {
                CameraMode::Follow(current) => roster
                    .iter()
                    .position(|e| *e == current)
                    .map(|ix| roster[(ix + 1) % roster.len()])
                    .unwrap_or(roster[0]),
                CameraMode::Free => roster[0],
            };
            state.mode = CameraMode::Follow(next);
        }
    }

    // F: frame every known player, so "where did everyone go" is one keypress
    if keys.just_pressed(KeyCode::KeyF) {
        let points: Vec<Vec3> = players
            .iter()
            .filter_map(|(entity, _)| transforms.get(entity).ok())
            .map(|t| t.translation)
            .collect();
        if !points.is_empty() {
            let centre = points.iter().fold(Vec3::ZERO, |acc, p| acc + *p) / points.len() as f32;
            let spread = points
                .iter()
                .map(|p| p.distance(centre))
                .fold(0.0f32, f32::max);
            state.mode = CameraMode::Free;
            state.position = centre + Vec3::new(0.0, spread.max(8.0), spread.max(8.0));
            state.pitch = -0.6;
            state.yaw = 0.0;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut state: ResMut<ViewerState>,
    cam_res: Res<PrimaryCameraRes>,
    // Local Transform, not GlobalTransform: this runs before propagation (it has to — the
    // camera's own GlobalTransform is computed from what it writes), so GlobalTransform
    // here would still hold last frame's value. Foreign players are spawned at the world
    // root with no parent, so their local transform is already world space.
    targets: Query<&Transform, Without<PrimaryCamera>>,
    mut cameras: Query<&mut Transform, With<PrimaryCamera>>,
) {
    // Drained first, and unconditionally: a frame spent with no button held (or before the
    // camera exists) must not bank motion that snaps the view on the next drag.
    let drag: Vec2 = motion.read().map(|e| e.delta).sum();
    let zoom: f32 = wheel
        .read()
        .map(|e| match e.unit {
            // matches input_manager's line/pixel normalisation
            MouseScrollUnit::Line => e.y,
            MouseScrollUnit::Pixel => e.y / 16.0,
        })
        .sum();

    let Ok(mut camera) = cameras.get_mut(cam_res.0) else {
        return;
    };

    // Right button only. Left stays free for the player list — orbiting on left-drag would
    // spin the camera every time you clicked a row.
    if buttons.pressed(MouseButton::Right) {
        state.yaw -= drag.x * LOOK_SENSITIVITY;
        state.pitch = (state.pitch - drag.y * LOOK_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    let rotation = Quat::from_euler(EulerRot::YXZ, state.yaw, state.pitch, 0.0);

    match state.mode {
        CameraMode::Follow(target) => {
            let Ok(target_transform) = targets.get(target) else {
                // followed player disconnected
                state.mode = CameraMode::Free;
                return;
            };
            state.distance = (state.distance - zoom).clamp(1.5, 200.0);
            let focus = target_transform.translation + Vec3::Y * CAPSULE_Y_OFFSET;
            let eye = focus + rotation * (Vec3::Z * state.distance);
            camera.translation = eye;
            camera.look_at(focus, Vec3::Y);
            // leaving follow mode should not teleport the camera
            state.position = eye;
        }
        CameraMode::Free => {
            let mut direction = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW) {
                direction += rotation * Vec3::NEG_Z;
            }
            if keys.pressed(KeyCode::KeyS) {
                direction += rotation * Vec3::Z;
            }
            if keys.pressed(KeyCode::KeyA) {
                direction += rotation * Vec3::NEG_X;
            }
            if keys.pressed(KeyCode::KeyD) {
                direction += rotation * Vec3::X;
            }
            if keys.pressed(KeyCode::KeyE) {
                direction += Vec3::Y;
            }
            if keys.pressed(KeyCode::KeyQ) {
                direction += Vec3::NEG_Y;
            }

            // Free-camera wheel trims a persistent speed multiplier. Applying the raw
            // wheel delta to this frame's speed instead would make the camera lurch for
            // one frame and then forget, which reads as a bug.
            if zoom != 0.0 {
                state.speed_scale = (state.speed_scale * (1.0 + zoom * 0.1)).clamp(0.05, 20.0);
            }

            let mut speed = BASE_SPEED * state.speed_scale;
            if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                speed *= BOOST_MULTIPLIER;
            }
            if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
                speed *= SLOW_MULTIPLIER;
            }

            state.position += direction.normalize_or_zero() * speed * time.delta_secs();
            camera.translation = state.position;
            camera.rotation = rotation;
        }
    }
}

// ---------------------------------------------------------------- player list

/// The roster changes rarely, so the list is rebuilt only when the set of players (or the
/// selection) actually changes rather than every frame.
fn roster_hash(entries: &[RosterEntry], selected: Option<Entity>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry in entries {
        entry.entity.hash(&mut hasher);
        entry.name.hash(&mut hasher);
    }
    selected.hash(&mut hasher);
    hasher.finish()
}

struct RosterEntry {
    entity: Entity,
    name: String,
    address: String,
}

fn collect_roster(
    players: &Query<(Entity, &ForeignPlayer, Option<&UserProfile>)>,
) -> Vec<RosterEntry> {
    let mut entries: Vec<RosterEntry> = players
        .iter()
        .map(|(entity, foreign, profile)| {
            let address = format!("{:#x}", foreign.address);
            RosterEntry {
                entity,
                name: profile
                    .map(|p| p.content.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "(no profile)".to_owned()),
                address,
            }
        })
        .collect();
    // stable ordering, so Tab cycles predictably and rows do not jump around
    entries.sort_by(|a, b| a.address.cmp(&b.address));
    entries
}

fn rebuild_player_list(
    mut commands: Commands,
    mut state: ResMut<ViewerState>,
    players: Query<(Entity, &ForeignPlayer, Option<&UserProfile>)>,
    list: Single<Entity, With<PlayerListRoot>>,
    labels: Single<Entity, With<LabelRoot>>,
) {
    let (list, labels) = (list.into_inner(), labels.into_inner());
    let entries = collect_roster(&players);
    let selected = match state.mode {
        CameraMode::Follow(e) => Some(e),
        CameraMode::Free => None,
    };
    let hash = roster_hash(&entries, selected);
    if hash == state.roster_hash {
        return;
    }
    state.roster_hash = hash;

    commands.entity(list).despawn_related::<Children>();
    commands.entity(labels).despawn_related::<Children>();

    commands.entity(list).with_children(|parent| {
        parent.spawn((
            Text::new(if entries.is_empty() {
                "no players connected".to_owned()
            } else {
                format!("players ({})", entries.len())
            }),
            TextFont {
                font_size: 13.0,
                ..Default::default()
            },
            TextColor(Color::srgb(0.7, 0.7, 0.7)),
        ));

        for entry in &entries {
            let is_selected = selected == Some(entry.entity);
            parent
                .spawn((
                    PlayerRow(entry.entity),
                    Button,
                    Node {
                        padding: UiRect::all(Val::Px(3.0)),
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    BackgroundColor(if is_selected {
                        Color::srgba(1.0, 0.85, 0.1, 0.25)
                    } else {
                        Color::NONE
                    }),
                ))
                .with_children(|row| {
                    row.spawn((
                        Text::new(entry.name.clone()),
                        TextFont {
                            font_size: 13.0,
                            ..Default::default()
                        },
                        TextColor(if is_selected {
                            Color::srgb(1.0, 0.85, 0.1)
                        } else {
                            Color::srgb(0.25, 0.55, 1.0)
                        }),
                        Pickable::IGNORE,
                    ));
                    let prefix = format!(
                        "{}…{}",
                        &entry.address[..8.min(entry.address.len())],
                        &entry.address[entry.address.len().saturating_sub(4)..]
                    );
                    row.spawn((
                        RowPosition {
                            player: entry.entity,
                            prefix: prefix.clone(),
                        },
                        Text::new(prefix),
                        TextFont {
                            font_size: 10.0,
                            ..Default::default()
                        },
                        TextColor(Color::srgb(0.6, 0.6, 0.6)),
                        Pickable::IGNORE,
                    ));
                });
        }
    });

    // screen-space nametags, positioned each frame by `position_labels`
    commands.entity(labels).with_children(|parent| {
        for entry in &entries {
            parent.spawn((
                WorldLabel(entry.entity),
                Text::new(entry.name.clone()),
                TextFont {
                    font_size: 11.0,
                    ..Default::default()
                },
                TextColor(Color::srgb(0.95, 0.95, 0.95)),
                Node {
                    position_type: PositionType::Absolute,
                    ..Default::default()
                },
                Pickable::IGNORE,
            ));
        }
    });
}

/// Live position readout for each row. Runs every frame; the list structure does not.
fn update_row_positions(
    transforms: Query<&GlobalTransform>,
    mut rows: Query<(&RowPosition, &mut Text)>,
) {
    for (row, mut text) in &mut rows {
        let updated = match transforms.get(row.player) {
            Ok(transform) => {
                let p = transform.translation();
                format!("{}  ({:.1}, {:.1}, {:.1})", row.prefix, p.x, p.y, p.z)
            }
            Err(_) => format!("{}  (gone)", row.prefix),
        };
        if text.0 != updated {
            text.0 = updated;
        }
    }
}

fn handle_row_clicks(
    mut state: ResMut<ViewerState>,
    rows: Query<(&Interaction, &PlayerRow), Changed<Interaction>>,
) {
    for (interaction, row) in &rows {
        if *interaction == Interaction::Pressed {
            state.mode = CameraMode::Follow(row.0);
        }
    }
}

fn position_labels(
    cam_res: Res<PrimaryCameraRes>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    targets: Query<&GlobalTransform, Without<Camera>>,
    mut labels: Query<(&WorldLabel, &mut Node)>,
) {
    let Ok((camera, camera_transform)) = cameras.get(cam_res.0) else {
        return;
    };
    for (label, mut node) in &mut labels {
        let Ok(target) = targets.get(label.0) else {
            node.display = Display::None;
            continue;
        };
        let world = target.translation() + Vec3::Y * (CAPSULE_Y_OFFSET * 2.0 + 0.2);
        let Ok(projected) = camera.world_to_viewport_with_depth(camera_transform, world) else {
            node.display = Display::None;
            continue;
        };
        // z is the ndc depth: <= 0 means the point is behind the camera
        if projected.z <= 0.0 {
            node.display = Display::None;
            continue;
        }
        node.display = Display::Block;
        node.left = Val::Px(projected.x);
        node.top = Val::Px(projected.y);
    }
}

// ---------------------------------------------------------------- hud

#[allow(clippy::too_many_arguments)]
fn update_hud(
    diagnostics: Res<DiagnosticsStore>,
    state: Res<ViewerState>,
    scenes: Query<&RendererSceneContext>,
    players: Query<&ForeignPlayer>,
    cam_res: Res<PrimaryCameraRes>,
    transforms: Query<&GlobalTransform>,
    hud: Single<&mut Text, With<HudText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or_default();

    let camera_position = transforms
        .get(cam_res.0)
        .map(|t| t.translation())
        .unwrap_or_default();

    let mode = match state.mode {
        CameraMode::Free => "free".to_owned(),
        CameraMode::Follow(e) => format!("follow {e:?}"),
    };

    // Tick number is the honest check that a window has not changed the server's cadence.
    let scene_summary = scenes
        .iter()
        .map(|context| {
            format!(
                "{} @ {:?} tick {} ({:?})",
                &context.hash[..12.min(context.hash.len())],
                context.base,
                context.tick_number,
                context.state
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");

    let mut hud = hud.into_inner();
    hud.0 = format!(
        "VIEWER  {mode}  players {}  cam ({:.1}, {:.1}, {:.1})  {:.0} fps\n{}\n\
         WASD/QE fly (shift faster, ctrl slower) · right-drag look · wheel zoom+speed · \
         Tab cycle · F frame all · Esc free",
        players.iter().count(),
        camera_position.x,
        camera_position.y,
        camera_position.z,
        fps,
        if scene_summary.is_empty() {
            "no scene loaded".to_owned()
        } else {
            scene_summary
        },
    );
}
