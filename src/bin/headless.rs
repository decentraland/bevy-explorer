//! Headless authoritative-server scene runner.
//!
//! Runs Decentraland SDK7 scenes with no window / render / GPU — the render-free
//! plugin set proven by `scene_runner`'s integration tests (`TestPlugins` +
//! `init_test_app`), plus the realm/preview loading and comms the real explorer
//! uses. `--server-mode` makes `isServer()` return true to scene JS, so a scene's
//! authoritative-server branch runs. Intended to replace hammurabi-headless.

use std::{sync::OnceLock, time::Duration};

use bevy::tasks::IoTaskPool;
use bevy::{
    app::ScheduleRunnerPlugin,
    diagnostic::{DiagnosticsPlugin, FrameCountPlugin},
    gizmos::GizmoPlugin,
    gltf::{Gltf, GltfPlugin},
    input::InputPlugin,
    log::LogPlugin,
    prelude::*,
    render::mesh::MeshPlugin,
    scene::ScenePlugin,
    state::app::StatesPlugin,
    time::TimePlugin,
};
use bevy_dui::DuiPlugin;
use collectibles::base_wearables;
use common::{
    inputs::InputMap,
    profile::SerializedProfile,
    rpc::RpcCall,
    sets::SetupSets,
    structs::{
        AppConfig, AppError, AvatarDynamicState, CursorLocks, EngineMovementControl,
        GraphicsSettings, HeadSync, IsServer, PermissionType, PermissionUsed, PermissionValue,
        PointAtSync, PreviewMode, PrimaryCamera, PrimaryCameraRes, PrimaryPlayerRes, PrimaryUser,
        SceneGlobalLight, SceneLoadDistance, SystemAudio, TimeOfDay, ToolTips,
    },
    util::{TaskCompat, TaskExt, UtilsPlugin},
};
use comms::{
    global_crdt::{ForeignPlayer, GlobalCrdtState},
    profile::{CurrentUserProfile, ProfileCache, UserProfile},
    AdapterManager, CommsPlugin, ServerSceneRooms,
};
use console::ConsolePlugin;
use dcl::interface::CrdtType;
use dcl::SceneLogMessage;
use dcl_component::{
    proto_components::sdk::components::{PbAvatarBase, PbAvatarEquippedData},
    SceneComponentId, SceneEntityId,
};
use dcl_deno_ipc::init_runtime;
use input_manager::{CumulativeAxisData, InputPriorities};
use ipfs::{map_realm_name, IpfsAssetServer, IpfsIoPlugin};
use nft::asset_source::Nft;
use restricted_actions::RestrictedActionsPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    initialize_scene::{PortableScenes, PortableSource, SceneHash, SceneLoading, PARCEL_SIZE},
    permissions::PermissionManager,
    renderer_context::RendererSceneContext,
    SceneRunnerPlugin,
};
use system_bridge::SystemBridgePlugin;
use ui_core::{scrollable::ScrollTargetEvent, stretch_uvs_image::StretchUvMaterial};
use user_input::avatar_movement::{
    ActivePlayerComponent, AvatarMovement, AvatarMovementInfo, FromConfig, GroundCollider,
};
use wallet::{
    delegation::{StorageDelegation, StorageDelegations},
    sign_request, Wallet, WalletPlugin,
};

static SESSION_LOG: OnceLock<String> = OnceLock::new();

struct Args {
    realm: String,
    location: IVec2,
    preview: bool,
    server_mode: bool,
    orchestrated: bool,
    timeout: Option<f32>,
    scene_threads: usize,
    tick_hz: u32,
    /// base64 world-storage delegation (hammurabi envelope); single-scene runs only —
    /// orchestrated scenes receive theirs via the control channel
    storage_delegation: Option<String>,
}

fn parse_args() -> Args {
    let mut args = pico_args::Arguments::from_env();
    let realm: String = args
        .value_from_str("--realm")
        .unwrap_or_else(|_| "http://localhost:8000".to_owned());
    let location = args
        .value_from_str::<_, common::structs::IVec2Arg>("--location")
        .ok()
        .map(|va| va.0)
        .unwrap_or(IVec2::ZERO);
    let preview = args.contains("--preview");
    let orchestrated = args.contains("--orchestrated");
    // orchestrated mode is always a server
    let server_mode = args.contains("--server-mode") || orchestrated;
    let timeout: Option<f32> = args.value_from_str("--timeout").ok();
    let scene_threads: usize = args
        .value_from_str("--scene-threads")
        .unwrap_or(if orchestrated { 16 } else { 4 });
    let tick_hz: u32 = args.value_from_str("--tick-hz").unwrap_or(30);
    // mirror hammurabi's worker env contract (PROCESS_STORAGE_DELEGATION)
    let storage_delegation: Option<String> = args
        .value_from_str("--storage-delegation")
        .ok()
        .or_else(|| std::env::var("PROCESS_STORAGE_DELEGATION").ok());
    Args {
        realm,
        location,
        preview,
        server_mode,
        orchestrated,
        timeout,
        scene_threads,
        tick_hz,
        storage_delegation,
    }
}

// ---------------- orchestrator control protocol ----------------
// stdin: one JSON command per line. stdout: control events as single lines with a
// reserved `@bevy-ctl ` prefix; scene logs as `@scene-log ` lines keyed by scene hash.
// The parent (multiplayer-server) never shares its authoritative key: adapters and
// delegations are minted parent-side and passed per scene, mirroring hammurabi workers.

#[derive(serde::Deserialize, Debug)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ControlCommand {
    /// Pin a scene and (optionally) join its pre-minted room.
    /// `urn` must be a full entity urn incl. baseUrl: urn:decentraland:entity:<hash>?=&baseUrl=<content>/contents/
    AddScene {
        #[serde(rename = "sceneId")]
        scene_id: String,
        urn: String,
        /// pre-minted comms adapter (livekit:wss://host?access_token=T). If absent in
        /// --preview mode, the engine mints one from the local gatekeeper (smoke tests only).
        adapter: Option<String>,
        /// base64 world-storage delegation minted by the orchestrator for this scene
        #[serde(rename = "storageDelegation")]
        storage_delegation: Option<String>,
    },
    RemoveScene {
        #[serde(rename = "sceneId")]
        scene_id: String,
    },
    Status,
    /// Storage-delegation renewal (mirrors hammurabi's storage-delegation:response IPC):
    /// the engine emits `storage-delegation-request` when a delegation nears expiry and
    /// the parent replies with a fresh one for that scene.
    #[serde(rename = "storage-delegation-response")]
    StorageDelegationResponse {
        #[serde(rename = "sceneId")]
        scene_id: Option<String>,
        delegation: Option<String>,
    },
}

#[derive(Resource)]
struct ControlChannel(std::sync::Mutex<std::sync::mpsc::Receiver<ControlCommand>>);

/// scenes the orchestrator asked for: hash -> pending adapter (taken when connected)
#[derive(Resource, Default)]
struct OrchestratedScenes {
    wanted: std::collections::HashMap<String, Option<String>>,
}

fn ctl_emit(event: &serde_json::Value) {
    println!("@bevy-ctl {event}");
}

fn spawn_stdin_reader() -> std::sync::mpsc::Receiver<ControlCommand> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<ControlCommand>(line) {
                Ok(cmd) => {
                    if tx.send(cmd).is_err() {
                        break;
                    }
                }
                Err(e) => ctl_emit(&serde_json::json!({
                    "type": "error", "error": format!("bad command: {e}")
                })),
            }
        }
        // stdin closed: parent died or finished — exit like hammurabi's disconnect handler
        ctl_emit(&serde_json::json!({"type": "stdin-closed"}));
        std::process::exit(0);
    });
    rx
}

fn main() {
    let session_time: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp_millis(
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    )
    .unwrap();
    let dirs = platform::project_directories().unwrap();
    let log_dir = dirs.data_local_dir();
    std::fs::create_dir_all(log_dir).unwrap();
    let session_log = log_dir.join(format!(
        "headless-{}.log",
        session_time.format("%Y%m%d-%H%M%S")
    ));
    SESSION_LOG
        .set(session_log.to_string_lossy().into_owned())
        .unwrap();

    // v8 runtime must init on the main thread before the App is built (matches the tests).
    // Headless is always a server: a lost JS sidecar must restart the whole engine (the
    // desktop client and the scene_runner tests leave this false).
    dcl_deno_ipc::EXIT_ON_SIDECAR_LOSS.store(true, std::sync::atomic::Ordering::SeqCst);
    init_runtime().unwrap();

    let args = parse_args();
    TIMEOUT.set(args.timeout).ok();

    println!(
        "[headless] realm={} location={} preview={} server_mode={} tick_hz={}",
        args.realm, args.location, args.preview, args.server_mode, args.tick_hz
    );

    let config = AppConfig {
        server: args.realm.clone(),
        location: args.location,
        graphics: GraphicsSettings {
            vsync: false,
            log_fps: false,
            fps_target: args.tick_hz as usize,
            ..Default::default()
        },
        scene_threads: args.scene_threads,
        // load everything around the fake player generously; unload never.
        scene_load_distance: 100.0,
        scene_unload_extra_distance: 0.0,
        scene_log_to_console: true,
        // headless permission policy (hammurabi parity): network APIs allowed, everything
        // user-facing denied. Without an explicit value these resolve to Ask, and the Ask
        // queue has no consumer headless — the scene promise would hang forever.
        default_permissions: [
            (PermissionType::Fetch, PermissionValue::Allow),
            (PermissionType::Websocket, PermissionValue::Allow),
            (PermissionType::Teleport, PermissionValue::Deny),
            (PermissionType::ChangeRealm, PermissionValue::Deny),
            (PermissionType::SpawnPortable, PermissionValue::Deny),
            (PermissionType::KillPortables, PermissionValue::Deny),
            (PermissionType::Web3, PermissionValue::Deny),
            (PermissionType::CopyToClipboard, PermissionValue::Deny),
            (PermissionType::OpenUrl, PermissionValue::Deny),
        ]
        .into(),
        ..Default::default()
    };

    let mut app = App::new();

    app.add_plugins(LogPlugin {
        filter: "wgpu=error,naga=error,bevy_animation=error,matrix=error,symphonia=warn"
            .to_string(),
        ..default()
    });

    // ---- render-free plugin set (mirrors scene_runner TestPlugins) ----
    app.add_plugins(TaskPoolPlugin::default())
        .add_plugins(FrameCountPlugin)
        .add_plugins(TimePlugin)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins(TransformPlugin)
        .add_plugins(DiagnosticsPlugin)
        .add_plugins(IpfsIoPlugin {
            preview: args.preview,
            assets_root: None,
            starting_realm: Some(map_realm_name(&args.realm)),
            content_server_override: None,
            num_slots: config.max_concurrent_remotes,
        })
        .add_plugins(AssetPlugin::default())
        .add_plugins(MeshPlugin)
        .add_plugins(GltfPlugin::default())
        .add_plugins(AnimationPlugin)
        .add_plugins(InputPlugin)
        .add_plugins(ScenePlugin)
        .add_plugins(StatesPlugin)
        .add_plugins(ConsolePlugin {
            add_bevy_console: false,
        })
        .add_plugins(WalletPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(DuiPlugin)
        .add_plugins(SystemBridgePlugin { bare: true });

    // manual asset/text inits update_text_shapes and material processing need (from init_test_app)
    app.init_asset::<Shader>()
        .init_asset::<AnimationClip>()
        .init_asset::<Image>()
        .init_asset::<StretchUvMaterial>()
        .init_asset::<bevy::text::Font>()
        .init_resource::<bevy::text::TextPipeline>()
        .init_resource::<bevy::text::CosmicFontSystem>();

    app.add_plugins(MaterialPlugin::<StandardMaterial>::default())
        .add_plugins(GizmoPlugin)
        .add_plugins(UtilsPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(SceneBoundPlugin)
        .add_plugins(RestrictedActionsPlugin);

    register_gltf_scene_types(&mut app);

    // scene text processing needs the SDK fonts; embedded assets provide them
    app.add_plugins(assets::EmbedAssetsPlugin);
    app.add_systems(Startup, |asset_server: Res<AssetServer>| {
        ui_core::init_fonts(&asset_server)
    });

    // world-storage delegations (per scene; CLI/env value is the single-scene fallback)
    let mut delegations = StorageDelegations::default();
    if args.orchestrated {
        // KEY-3 style structural gate: a process-level credential in multi-scene mode
        // would be a wildcard — delegations must arrive per scene over the control channel
        if args.storage_delegation.is_some() {
            println!("[headless] refusing --storage-delegation/PROCESS_STORAGE_DELEGATION in orchestrated mode; pass storageDelegation per add-scene");
        }
    } else if let Some(encoded) = &args.storage_delegation {
        match StorageDelegation::parse(encoded, &map_realm_name(&args.realm)) {
            Ok(delegation) => {
                println!(
                    "[headless] storage delegation loaded: world={} scene={} parcel={}",
                    delegation.world, delegation.scene_id, delegation.parcel
                );
                delegations.fallback = Some(delegation);
            }
            Err(e) => println!("[headless] ignoring storage delegation: {e}"),
        }
    }

    // ---- resources & events the scene runtime needs (no render/UI plugins) ----
    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();
    // KEY-3: the orchestrated engine must only ever hold a throwaway identity — the
    // authoritative key lives in the orchestrator and never reaches this process.
    assert!(
        wallet.is_guest(),
        "orchestrated engine must run with a guest wallet"
    );

    app.insert_resource(config)
        .insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .insert_resource(wallet)
        .init_resource::<ProfileCache>()
        .init_resource::<PermissionManager>()
        .init_resource::<InputMap>()
        .init_resource::<InputPriorities>()
        .init_resource::<CumulativeAxisData>()
        .init_resource::<ToolTips>()
        .init_resource::<SceneGlobalLight>()
        .init_resource::<CursorLocks>()
        .init_resource::<EngineMovementControl>()
        .init_resource::<AvatarMovementInfo>()
        .init_asset::<Nft>()
        .insert_resource(TimeOfDay {
            time: 10.0 * 3600.0,
        })
        .insert_resource(SceneLoadDistance {
            // orchestrated: never load by player position — scenes come ONLY from the
            // control channel (portables are exempt from distance loading).
            load: if args.orchestrated { -1.0 } else { 100.0 },
            unload: 0.0,
            load_imposter: 0.0,
        })
        .insert_resource(PreviewMode {
            server: args.preview.then(|| map_realm_name(&args.realm)),
            is_preview: args.preview,
            preview_parcel: None,
        })
        .insert_resource(IsServer(args.server_mode))
        // never join realm-wide comms (archipelago / world room): a server would be a
        // ghost participant. Scene rooms use per-scene adapters and are unaffected.
        .insert_resource(comms::DisableRealmComms(true))
        .insert_resource(delegations)
        .add_event::<RpcCall>()
        .add_event::<SystemAudio>()
        .add_event::<ScrollTargetEvent>()
        .add_event::<PermissionUsed>();

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));
    app.add_systems(Startup, setup.in_set(SetupSets::Init));
    app.add_systems(PreUpdate, supervisor);
    app.add_systems(
        Update,
        (
            drain_permissions,
            replicate_avatar_info,
            free_gltf_textures,
            reap_terminal_scene_rooms,
        ),
    );

    if args.orchestrated {
        app.insert_resource(ControlChannel(std::sync::Mutex::new(spawn_stdin_reader())))
            .init_resource::<OrchestratedScenes>()
            // structural gate: the engine must never sign gatekeeper handshakes in
            // orchestrated mode — adapters are always minted by the trusted parent
            .insert_resource(comms::DisableSceneRoomGatekeeper(true))
            .add_systems(
                Update,
                (
                    drain_control_commands,
                    demux_scene_logs,
                    request_delegation_renewals,
                ),
            )
            .add_systems(PostUpdate, (emit_scene_status, emit_failed_scene_status));
        ctl_emit(&serde_json::json!({"type": "starting", "realm": args.realm}));
    }

    log_panics::init();

    // Warn-and-continue on fallible-system / command / observer errors instead of the
    // Bevy default (panic). A raw panic in a scene-data system still aborts the shared
    // engine and takes every co-tenant down, so this is a safety net, not a full fix —
    // see HEADLESS_SECURITY_ISOLATION.md (S3).
    bevy::ecs::error::GLOBAL_ERROR_HANDLER
        .set(bevy::ecs::error::warn)
        .ok();

    app.run();
}

/// GLTF scenes are written into the world via bevy's SceneSpawner, which reflects
/// every component the loader emits and panics on any unregistered type.
/// RenderPlugin/VisibilityPlugin/PbrPlugin normally register these; we omit those
/// plugins (no GPU), so register the full GLTF-scene component set by hand.
fn register_gltf_scene_types(app: &mut App) {
    use bevy::pbr::{
        CascadeShadowConfig, Cascades, DirectionalLight, NotShadowCaster, NotShadowReceiver,
        PointLight, SpotLight,
    };
    use bevy::render::{
        mesh::{
            morph::{MeshMorphWeights, MorphWeights},
            skinning::SkinnedMesh,
            Mesh3d,
        },
        primitives::Aabb,
        view::{InheritedVisibility, ViewVisibility, Visibility, VisibilityClass, VisibilityRange},
    };
    app.register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<VisibilityClass>()
        .register_type::<VisibilityRange>()
        .register_type::<Aabb>()
        .register_type::<Mesh3d>()
        .register_type::<SkinnedMesh>()
        .register_type::<MorphWeights>()
        .register_type::<MeshMorphWeights>()
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .register_type::<Name>()
        .register_type::<PointLight>()
        .register_type::<SpotLight>()
        .register_type::<DirectionalLight>()
        .register_type::<Cascades>()
        .register_type::<CascadeShadowConfig>()
        .register_type::<NotShadowCaster>()
        .register_type::<NotShadowReceiver>()
        .register_type::<bevy::gltf::GltfExtras>()
        .register_type::<bevy::gltf::GltfMaterialExtras>()
        .register_type::<bevy::gltf::GltfMaterialName>()
        .register_type::<bevy::gltf::GltfMeshExtras>()
        .register_type::<bevy::gltf::GltfSceneExtras>();
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
) {
    // fake player: process_scene_lifecycle early-returns without a PrimaryUser,
    // and PrimaryEntities::player() panics without the marker. Placed at the scene
    // location so position-based loading picks up the parcel scene.
    let player_pos = Vec3::new(
        8.0 + PARCEL_SIZE * config.location.x as f32,
        0.0,
        -8.0 + -PARCEL_SIZE * config.location.y as f32,
    );
    // NOT OutOfWorld: the player must count as "inside" the scene parcel so
    // update_scene_room fires the authoritative scene-room connection.
    let player_id = commands
        .spawn((
            Transform::from_translation(player_pos),
            Visibility::default(),
            PrimaryUser::default(),
            AvatarDynamicState::default(),
            HeadSync::default(),
            PointAtSync::default(),
            GroundCollider::default(),
            // movePlayerTo-with-duration / walkTo park their response here; without it
            // (inserted by the omitted user_input plugin) the scene promise never settles
            ActivePlayerComponent::<AvatarMovement>::from_config(&config),
        ))
        .id();

    // Visibility included: scenes may ParentPositionSync entities to the camera, and
    // the sync system reads InheritedVisibility from the sync target.
    let camera_id = commands
        .spawn((
            PrimaryCamera::default(),
            Transform::from_translation(player_pos),
            Visibility::default(),
        ))
        .id();

    player_resource.0 = player_id;
    cam_resource.0 = camera_id;

    wallet.finalize_as_guest();
    current_profile.profile = Some(UserProfile {
        version: 0,
        content: SerializedProfile {
            eth_address: format!("{:#x}", wallet.address().unwrap()),
            user_id: Some(format!("{:#x}", wallet.address().unwrap())),
            ..Default::default()
        },
        base_url: Default::default(),
    });
    // guard against the auto-deploy of the guest profile (login.rs pattern)
    current_profile.is_deployed = true;
}

/// Liveness supervisor. Logs when the scene first ticks; exits non-zero on a
/// broken scene or the optional wall-clock timeout, zero on graceful conditions.
#[allow(clippy::too_many_arguments)]
fn supervisor(
    time: Res<Time>,
    scenes: Query<&RendererSceneContext>,
    mut errors: EventReader<AppError>,
    mut exit: EventWriter<AppExit>,
    mut announced: Local<bool>,
    mut last_report: Local<f32>,
    orchestrated: Option<Res<OrchestratedScenes>>,
) {
    let elapsed = time.elapsed_secs();

    for e in errors.read() {
        error!("[headless] scene error: {e:?}");
    }

    let mut any_broken = false;
    let mut max_tick = 0u32;
    let mut count = 0usize;
    for ctx in scenes.iter() {
        count += 1;
        max_tick = max_tick.max(ctx.tick_number);
        if ctx.broken {
            any_broken = true;
            error!("[headless] scene {} is broken", ctx.hash);
        }
    }

    if count > 0 && max_tick >= 1 && !*announced {
        *announced = true;
        println!("[headless] {count} scene(s) live, first tick reached");
    }

    if *announced && elapsed - *last_report > 5.0 {
        *last_report = elapsed;
        println!("[headless] alive: {count} scene(s), max_tick={max_tick}, t={elapsed:.0}s");
    }

    // Orchestrated: a broken scene is reported (scene-broken event) and removed by the
    // orchestrator — it must NOT take the other scenes down with an engine exit.
    if any_broken && orchestrated.is_none() {
        exit.write(AppExit::from_code(1));
    }

    // wall-clock timeout: graceful success exit for smoke tests
    // (checked in main via arg-injected resource below)
    if let Some(limit) = TIMEOUT.get().copied().flatten() {
        if elapsed > limit {
            println!("[headless] timeout {limit}s reached, exiting");
            exit.write_default();
        }
    }
}

// timeout is read in the supervisor; stored globally to avoid threading it as a resource
static TIMEOUT: OnceLock<Option<f32>> = OnceLock::new();

/// Answer queued permission requests by the headless policy. The Ask queue's only
/// consumer is the omitted system_ui crate, so anything that lands here (a permission
/// type missing from default_permissions) would otherwise hang its scene promise forever.
fn drain_permissions(mut manager: ResMut<PermissionManager>, config: Res<AppConfig>) {
    while let Some(req) = manager.pending.pop_front() {
        let allow = matches!(
            config.default_permissions.get(&req.ty),
            Some(PermissionValue::Allow)
        );
        req.sender.send(allow);
    }
}

/// Forward player profile data into scene CRDTs as AVATAR_BASE / AVATAR_EQUIPPED_DATA.
/// Copy of avatar::update_avatar_info — the AvatarPlugin that normally registers it is
/// render-bound and omitted headless; without this, SDK getPlayer()/onEnterScene never
/// see names or wearables.
fn replicate_avatar_info(
    updated_players: Query<(Option<&ForeignPlayer>, &UserProfile), Changed<UserProfile>>,
    mut global_state: ResMut<GlobalCrdtState>,
) {
    for (player, profile) in &updated_players {
        let avatar = &profile.content.avatar;
        global_state.update_crdt(
            SceneComponentId::AVATAR_BASE,
            CrdtType::LWW_ANY,
            player.map(|p| p.scene_id).unwrap_or(SceneEntityId::PLAYER),
            &PbAvatarBase {
                name: profile.content.name.clone(),
                skin_color: avatar.skin.map(|c| c.color),
                eyes_color: avatar.eyes.map(|c| c.color),
                hair_color: avatar.hair.map(|c| c.color),
                body_shape_urn: avatar
                    .body_shape
                    .as_deref()
                    .map(ToString::to_string)
                    .unwrap_or(base_wearables::default_bodyshape_urn().to_string()),
            },
        );
        global_state.update_crdt(
            SceneComponentId::AVATAR_EQUIPPED_DATA,
            CrdtType::LWW_ANY,
            player.map(|p| p.scene_id).unwrap_or(SceneEntityId::PLAYER),
            &PbAvatarEquippedData {
                wearable_urns: avatar.wearables.to_vec(),
                emote_urns: (0..10)
                    .map(|ix| {
                        avatar
                            .emotes
                            .as_ref()
                            .unwrap_or(&Vec::default())
                            .iter()
                            .find(|emote| emote.slot == ix)
                            .map(|emote| emote.urn.clone())
                            .unwrap_or_default()
                    })
                    .collect(),
                force_render: avatar.force_render.clone().unwrap_or_default(),
            },
        )
    }
}

/// Free decoded glTF textures once a glTF finishes loading. The fork's glTF loader
/// decodes every embedded texture into raw RGBA unconditionally; with no render app
/// there is nothing to hand the pixels to, so they would stay resident in
/// Assets<Image> for the life of the process — the dominant headless memory cost
/// (texture-heavy worlds hold ~700 MB each). Nothing observable depends on them:
/// colliders/raycasts run on mesh geometry, and no headless system reads pixel data.
/// Stripping the handles from the glTF's own materials drops the last strong
/// references; dangling handles are harmless without a renderer. The decode CPU is
/// still paid at load — skipping the decode itself needs a loader gate in the bevy
/// fork (tracked for a follow-up there).
fn free_gltf_textures(
    mut events: EventReader<AssetEvent<Gltf>>,
    gltfs: Res<Assets<Gltf>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else {
            continue;
        };
        let Some(gltf) = gltfs.get(*id) else { continue };
        let mut freed = 0usize;
        for h_material in gltf.materials.iter().chain(gltf.named_materials.values()) {
            let Some(material) = materials.get_mut(h_material) else {
                continue;
            };
            for texture in [
                &mut material.base_color_texture,
                &mut material.emissive_texture,
                &mut material.metallic_roughness_texture,
                &mut material.normal_map_texture,
                &mut material.occlusion_texture,
                &mut material.depth_map,
            ] {
                if let Some(handle) = texture.take() {
                    if images.remove(&handle).is_some() {
                        freed += 1;
                    }
                }
            }
        }
        if freed > 0 {
            debug!("freed {freed} decoded gltf textures");
        }
    }
}

/// Tear down scene-room transports whose LiveKit room hit a terminal disconnect
/// (DuplicateIdentity / kicked): the access token is spent, so no in-process reconnect
/// is possible. Orchestrated mode notifies the parent (which re-adds the scene with a
/// freshly minted adapter); standalone server mode lets update_scene_room re-mint.
/// Without this the dead room handle errors on every publish attempt forever.
fn reap_terminal_scene_rooms(
    mut commands: Commands,
    mut server_rooms: ResMut<ServerSceneRooms>,
    terminal: Query<(), With<comms::livekit::room::ServerRoomTerminal>>,
    orchestrated: Option<Res<OrchestratedScenes>>,
) {
    server_rooms.0.retain(|scene, (_, entity)| {
        if terminal.get(*entity).is_err() {
            return true;
        }
        warn!("[headless] scene room for {scene} terminally disconnected (duplicate identity / kicked); tearing down");
        if let Ok(mut e) = commands.get_entity(*entity) {
            e.despawn();
        }
        if orchestrated.is_some() {
            ctl_emit(&serde_json::json!({
                "type": "scene-room-disconnected", "scene": scene
            }));
        }
        false
    });
}

/// Ask the orchestrator for fresh delegations nearing expiry (hammurabi semantics:
/// 5-minute refresh buffer, at most one request per scene per 30 s).
fn request_delegation_renewals(
    delegations: Res<StorageDelegations>,
    time: Res<Time>,
    mut last_request: Local<std::collections::HashMap<String, f32>>,
) {
    const REFRESH_BUFFER_MS: i64 = 5 * 60 * 1000;
    const REQUEST_THROTTLE_SECS: f32 = 30.0;

    let now_ms = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let elapsed = time.elapsed_secs();

    // drop throttle state for scenes no longer holding a delegation (removed scenes),
    // so this map doesn't grow unbounded over the engine's lifetime
    last_request.retain(|scene, _| delegations.by_scene.contains_key(scene));

    for (scene, delegation) in &delegations.by_scene {
        if now_ms < delegation.expiration - REFRESH_BUFFER_MS {
            continue;
        }
        let due = last_request
            .get(scene)
            .is_none_or(|last| elapsed - last > REQUEST_THROTTLE_SECS);
        if due {
            last_request.insert(scene.clone(), elapsed);
            ctl_emit(&serde_json::json!({
                "type": "storage-delegation-request", "scene": scene
            }));
        }
    }
}

// ---------------- orchestrated-mode systems ----------------

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn drain_control_commands(
    control: Res<ControlChannel>,
    mut orch: ResMut<OrchestratedScenes>,
    mut portables: ResMut<PortableScenes>,
    mut manager: AdapterManager,
    mut commands: Commands,
    mut server_rooms: ResMut<ServerSceneRooms>,
    wallet: Res<Wallet>,
    ipfs: IpfsAssetServer,
    preview: Res<PreviewMode>,
    scenes: Query<&RendererSceneContext>,
    mut delegations: ResMut<StorageDelegations>,
    config: Res<AppConfig>,
    mut mint_tasks: Local<Vec<(String, bevy::tasks::Task<Result<String, anyhow::Error>>)>>,
) {
    let store_delegation = |scene_id: &str, encoded: &str, delegations: &mut StorageDelegations| {
        match StorageDelegation::parse(encoded, &map_realm_name(&config.server)) {
            Ok(delegation) => {
                // reject a renewal that rebinds to another scene's credential
                // (hammurabi's same-scene guard)
                let same_scene = delegations.by_scene.get(scene_id).is_none_or(|current| {
                    current.world == delegation.world
                        && current.scene_id == delegation.scene_id
                        && current.parcel == delegation.parcel
                });
                if !same_scene {
                    ctl_emit(&serde_json::json!({
                        "type": "error", "scene": scene_id,
                        "error": "storage delegation is bound to a different scene; ignored"
                    }));
                    return;
                }
                delegations.by_scene.insert(scene_id.to_owned(), delegation);
                ctl_emit(&serde_json::json!({
                    "type": "storage-delegation-set", "scene": scene_id
                }));
            }
            Err(e) => ctl_emit(&serde_json::json!({
                "type": "error", "scene": scene_id,
                "error": format!("bad storage delegation: {e}")
            })),
        }
    };
    let connect = |scene_id: &str,
                   adapter: &str,
                   manager: &mut AdapterManager,
                   commands: &mut Commands,
                   server_rooms: &mut ServerSceneRooms| {
        // ONLY livekit adapters: any other protocol (signed-login, ws-room,
        // fixed-adapter recursion) could make the engine sign a remote-chosen
        // payload with its identity. Applies to orchestrator input AND
        // gatekeeper mint responses alike.
        if !adapter.starts_with("livekit:") {
            ctl_emit(&serde_json::json!({
                "type": "scene-failed", "scene": scene_id,
                "error": format!("refusing non-livekit adapter: {}", &adapter[..adapter.len().min(24)])
            }));
            return;
        }
        if let Some(ent) = manager.connect(adapter) {
            commands
                .entity(ent)
                .try_insert(comms::SceneRoom(scene_id.to_owned()));
            server_rooms
                .0
                .insert(scene_id.to_owned(), (adapter.to_owned(), ent));
            ctl_emit(&serde_json::json!({"type": "scene-room-connected", "scene": scene_id}));
        } else {
            ctl_emit(&serde_json::json!({
                "type": "error", "scene": scene_id, "error": "adapter connect failed"
            }));
        }
    };

    // poll pending preview-gatekeeper mints
    let mut i = 0;
    while i < mint_tasks.len() {
        if let Some(result) = mint_tasks[i].1.complete() {
            let (scene_id, _) = mint_tasks.swap_remove(i);
            match result {
                Ok(adapter) => connect(
                    &scene_id,
                    &adapter,
                    &mut manager,
                    &mut commands,
                    &mut server_rooms,
                ),
                Err(e) => ctl_emit(&serde_json::json!({
                    "type": "error", "scene": scene_id, "error": format!("gatekeeper mint failed: {e}")
                })),
            }
        } else {
            i += 1;
        }
    }

    let Ok(rx) = control.0.lock() else { return };
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            ControlCommand::AddScene {
                scene_id,
                urn,
                adapter,
                storage_delegation,
            } => {
                portables.insert(
                    scene_id.clone(),
                    PortableSource {
                        pid: urn,
                        parent_scene: None,
                        ens: None,
                        super_user: false,
                    },
                );
                orch.wanted.insert(scene_id.clone(), adapter.clone());
                if let Some(encoded) = &storage_delegation {
                    store_delegation(&scene_id, encoded, &mut delegations);
                }
                ctl_emit(&serde_json::json!({"type": "scene-added", "scene": scene_id}));

                if let Some(adapter) = adapter {
                    // pre-minted by the trusted orchestrator: never sign anything ourselves
                    connect(
                        &scene_id,
                        &adapter,
                        &mut manager,
                        &mut commands,
                        &mut server_rooms,
                    );
                } else if !preview.is_preview {
                    // production: the adapter MUST be minted by the trusted orchestrator.
                    // The engine never signs gatekeeper handshakes outside local preview.
                    ctl_emit(&serde_json::json!({
                        "type": "scene-failed", "scene": scene_id,
                        "error": "adapter required outside preview mode"
                    }));
                } else {
                    // smoke-test fallback: mint from the local preview gatekeeper with the
                    // guest identity (mirrors hammurabi's LocalPreview flow)
                    let wallet = wallet.clone();
                    let client = ipfs.ipfs().client();
                    let sid = scene_id.clone();
                    let task = IoTaskPool::get().spawn_compat(async move {
                        let url =
                            "https://comms-gatekeeper-local.decentraland.org/get-server-scene-adapter";
                        let uri = http::Uri::try_from(url)?;
                        let meta = serde_json::json!({
                            "intent": "dcl:explorer:comms-handshake",
                            "signer": "dcl:explorer",
                            "isGuest": true,
                            "realm": {"serverName": "LocalPreview"},
                            "realmName": "LocalPreview",
                            "sceneId": sid,
                        })
                        .to_string();
                        let headers = sign_request("POST", &uri, &wallet, meta).await?;
                        let mut request = client
                            .post(url)
                            .timeout(std::time::Duration::from_secs(10))
                            .header("Content-Type", "application/json");
                        for (k, v) in headers {
                            request = request.header(k, v);
                        }
                        let response = request.send().await?;
                        if !response.status().is_success() {
                            anyhow::bail!("gatekeeper status {}", response.status());
                        }
                        let body: serde_json::Value = response.json().await?;
                        body["adapter"]
                            .as_str()
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| anyhow::anyhow!("no adapter in response"))
                    });
                    mint_tasks.push((scene_id, task));
                }
            }
            ControlCommand::RemoveScene { scene_id } => {
                portables.remove(&scene_id);
                orch.wanted.remove(&scene_id);
                delegations.by_scene.remove(&scene_id);
                if let Some((_, ent)) = server_rooms.0.remove(&scene_id) {
                    if let Ok(mut c) = commands.get_entity(ent) {
                        c.despawn();
                    }
                }
                ctl_emit(&serde_json::json!({"type": "scene-removed", "scene": scene_id}));
            }
            ControlCommand::Status => {
                for ctx in scenes.iter() {
                    ctl_emit(&serde_json::json!({
                        "type": "scene-status", "scene": ctx.hash,
                        "tick": ctx.tick_number, "broken": ctx.broken,
                    }));
                }
            }
            ControlCommand::StorageDelegationResponse {
                scene_id,
                delegation,
            } => {
                if let (Some(scene_id), Some(encoded)) = (&scene_id, &delegation) {
                    store_delegation(scene_id, encoded, &mut delegations);
                }
            }
        }
    }
}

/// Demux per-scene logs onto stdout as machine-parsable lines keyed by scene hash,
/// so the orchestrator can route them to its per-scene SSE log buffers.
fn demux_scene_logs(
    scenes: Query<(Entity, &RendererSceneContext)>,
    mut receivers: Local<
        std::collections::HashMap<
            Entity,
            (String, common::util::RingBufferReceiver<SceneLogMessage>),
        >,
    >,
) {
    for (ent, ctx) in scenes.iter() {
        let (_, rx) = receivers.entry(ent).or_insert_with(|| {
            let (_missed, backlog, rx) = ctx.logs.read();
            for log in backlog {
                emit_scene_log(&ctx.hash, &log);
            }
            (ctx.hash.clone(), rx)
        });
        while let Ok(log) = rx.try_recv() {
            emit_scene_log(&ctx.hash, &log);
        }
    }
    receivers.retain(|ent, _| scenes.contains(*ent));
}

fn emit_scene_log(hash: &str, log: &SceneLogMessage) {
    let level = match log.level {
        dcl::SceneLogLevel::Log => "log",
        dcl::SceneLogLevel::SceneError => "error",
        dcl::SceneLogLevel::SystemError => "system",
    };
    println!(
        "@scene-log {}",
        serde_json::json!({"scene": hash, "level": level, "ts": log.timestamp, "msg": log.message})
    );
}

/// Periodic per-scene status + first-tick / broken events for the orchestrator.
fn emit_scene_status(
    time: Res<Time>,
    scenes: Query<&RendererSceneContext>,
    mut last: Local<f32>,
    mut live: Local<std::collections::HashSet<String>>,
    mut broken: Local<std::collections::HashSet<String>>,
) {
    // Prune dedup entries for scenes that no longer exist, so a scene that is removed
    // and later re-added (same hash, fresh entity) re-emits scene-live / scene-broken.
    // Without this the orchestrator's remove+re-add recovery works only once per scene
    // per engine lifetime — a re-broken scene would silently stay broken while the
    // orchestrator still believes it is live.
    let current: std::collections::HashSet<String> =
        scenes.iter().map(|ctx| ctx.hash.clone()).collect();
    live.retain(|hash| current.contains(hash));
    broken.retain(|hash| current.contains(hash));

    for ctx in scenes.iter() {
        if ctx.tick_number >= 1 && !live.contains(&ctx.hash) {
            live.insert(ctx.hash.clone());
            ctl_emit(&serde_json::json!({"type": "scene-live", "scene": ctx.hash}));
        }
        if ctx.broken && !broken.contains(&ctx.hash) {
            broken.insert(ctx.hash.clone());
            ctl_emit(&serde_json::json!({"type": "scene-broken", "scene": ctx.hash}));
        }
    }

    let elapsed = time.elapsed_secs();
    if elapsed - *last > 5.0 {
        *last = elapsed;
        for ctx in scenes.iter() {
            ctl_emit(&serde_json::json!({
                "type": "scene-status", "scene": ctx.hash,
                "tick": ctx.tick_number, "broken": ctx.broken,
            }));
        }
    }
}

/// Report scenes that FAILED to load (bad/missing entity definition, JS init error)
/// to the orchestrator. A failed scene never gains a RendererSceneContext, so neither
/// emit_scene_status nor the supervisor observes it — without this it sits invisibly
/// wedged in live_scenes, and the orchestrator keeps it "active" so join events never
/// respawn it. Emit scene-broken (deduped per live entity) so the orchestrator runs
/// its bounded remove + re-add recovery, which re-fetches the entity and clears a
/// transient content-server flake. Keyed by Entity, so a re-added scene (fresh entity,
/// same hash) reports again and the per-scene restart budget applies.
fn emit_failed_scene_status(
    loading: Query<(Entity, &SceneHash, &SceneLoading)>,
    mut reported: Local<std::collections::HashSet<Entity>>,
    orchestrated: Option<Res<OrchestratedScenes>>,
) {
    if orchestrated.is_none() {
        return;
    }
    let failed: Vec<(Entity, &str)> = loading
        .iter()
        .filter(|(_, _, state)| matches!(state, SceneLoading::Failed))
        .map(|(entity, hash, _)| (entity, hash.0.as_str()))
        .collect();
    let current: std::collections::HashSet<Entity> = failed.iter().map(|(e, _)| *e).collect();
    reported.retain(|e| current.contains(e));
    for (entity, hash) in failed {
        if reported.insert(entity) {
            error!("[headless] scene {hash} failed to load");
            ctl_emit(&serde_json::json!({"type": "scene-broken", "scene": hash}));
        }
    }
}
