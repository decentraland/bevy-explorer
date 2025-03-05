use comms::{
    preview::PreviewMode,
    profile::{CurrentUserProfile, ProfileCache, UserProfile},
    CommsPlugin,
};
use console::ConsolePlugin;
use imposters::{render::ImposterMissing, DclImposterPlugin};

use bevy::{
    core::TaskPoolThreadAssignmentPolicy, prelude::*, window::ExitCondition, winit::WinitSettings,
};

use common::{
    profile::SerializedProfile,
    rpc::RpcCall,
    sets::SetupSets,
    structs::{
        AppConfig, AvatarDynamicState, CursorLocks, GraphicsSettings, IVec2Arg, PrimaryCamera,
        PrimaryCameraRes, PrimaryPlayerRes, SceneImposterBake, SceneLoadDistance, SystemAudio,
        ToolTips,
    },
    util::{config_file, UtilsPlugin},
};
use input_manager::{AcceptInput, InputMap};
use nft::asset_source::Nft;
use restricted_actions::RestrictedActionsPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    initialize_scene::ScenePointers, permissions::PermissionManager,
    update_world::mesh_collider::GroundCollider, OutOfWorld, SceneRunnerPlugin,
};

use ipfs::{CurrentRealm, IpfsIoPlugin};
use system_bridge::SystemBridgePlugin;
use ui_core::{scrollable::ScrollTargetEvent, UiCorePlugin};
use visuals::SceneGlobalLight;
use wallet::Wallet;

fn main() {
    let mut args = pico_args::Arguments::from_env();
    let config_file = config_file();

    let levels = args.value_from_str("--levels").unwrap_or(5);
    let range = args
        .value_from_str("--range")
        .map(|f: f32| f * 16.0)
        .unwrap_or(f32::MAX);

    let base_config: AppConfig = std::fs::read(&config_file)
        .ok()
        .and_then(|f| serde_json::from_slice(&f).ok())
        .unwrap_or_default();

    let final_config = AppConfig {
        server: args
            .value_from_str("--server")
            .ok()
            .unwrap_or(base_config.server),
        location: args
            .value_from_str::<_, IVec2Arg>("--location")
            .ok()
            .map(|va| va.0)
            .unwrap_or(IVec2::ZERO),
        graphics: GraphicsSettings {
            vsync: false,
            log_fps: false,
            fps_target: 999,
            ..base_config.graphics
        },
        scene_threads: args
            .value_from_str("--threads")
            .ok()
            .unwrap_or(base_config.scene_threads),
        scene_load_distance: -1.0,
        scene_unload_extra_distance: 0.0,
        scene_imposter_bake: SceneImposterBake::FullSpeed,
        scene_imposter_distances: std::iter::repeat(0.0)
            .take(levels)
            .chain(std::iter::once(range))
            .collect(),
        scene_log_to_console: args.contains("--scene_log_to_console"),
        ..base_config
    };

    let content_server_override = args.value_from_str("--content-server").ok();
    let zip_output = args.value_from_str("--zip-output").ok();

    let remaining = args.finish();
    if !remaining.is_empty() {
        println!(
            "failed to parse args: {}",
            remaining
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        );
        return;
    }

    let mut app = App::new();
    app.insert_resource(WinitSettings {
        focused_mode: bevy::winit::UpdateMode::Continuous,
        unfocused_mode: bevy::winit::UpdateMode::Continuous,
    });
    app.add_plugins(
        DefaultPlugins
            .set(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions {
                    async_compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: 2,
                        max_threads: 8,
                        percent: 0.25,
                    },
                    io: TaskPoolThreadAssignmentPolicy {
                        min_threads: 8,
                        max_threads: 8,
                        percent: 0.25,
                    },
                    compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: 2,
                        max_threads: 8,
                        percent: 0.25,
                    },
                    ..Default::default()
                },
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..Default::default()
            })
            .build()
            .add_before::<bevy::asset::AssetPlugin, _>(IpfsIoPlugin {
                preview: false,
                starting_realm: Some(final_config.server.clone()),
                content_server_override,
                assets_root: Default::default(),
                num_slots: final_config.max_concurrent_remotes,
            }),
    );

    // Analytics
    app.insert_resource(SceneLoadDistance {
        load: final_config.scene_load_distance,
        unload: final_config.scene_unload_extra_distance,
        load_imposter: final_config
            .scene_imposter_distances
            .last()
            .map(|last| {
                // actual distance we need is last + diagonal of the largest mip size
                let mip_size =
                    (1 << (final_config.scene_imposter_distances.len() - 1)) as f32 * 16.0;
                let req = last + (2.0 * mip_size * mip_size).sqrt();
                println!(
                    "imposter mips: {:?} -> distance {}",
                    final_config.scene_imposter_distances, req
                );
                req
            })
            .unwrap_or(0.0),
    });

    app.insert_resource(final_config);

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));

    app.add_plugins(UtilsPlugin)
        .add_plugins(UiCorePlugin)
        .add_plugins(ConsolePlugin { add_egui: true })
        .add_plugins(SceneBoundPlugin)
        .add_plugins(SceneRunnerPlugin)
        .add_plugins(CommsPlugin)
        .add_plugins(RestrictedActionsPlugin)
        .add_plugins(DclImposterPlugin {
            zip_output,
            download: false,
        })
        .add_plugins(SystemBridgePlugin { bare: true });

    app.insert_resource(PrimaryPlayerRes(Entity::PLACEHOLDER))
        .insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER))
        .add_systems(Startup, setup.in_set(SetupSets::Init));

    // add required things that don't get initialized by their plugins
    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();
    app.init_resource::<ProfileCache>()
        .insert_resource(wallet)
        .add_event::<SystemAudio>()
        .init_resource::<PermissionManager>()
        .init_resource::<InputMap>()
        .init_resource::<AcceptInput>()
        .init_resource::<ToolTips>()
        .init_resource::<SceneGlobalLight>()
        .add_event::<RpcCall>()
        .add_event::<ScrollTargetEvent>()
        .init_resource::<PreviewMode>()
        .init_asset::<Nft>()
        .init_resource::<CursorLocks>();

    // requires local version of `bevy_mod_debugdump` due to once_cell version conflict.
    // probably resolved by updating deno. TODO: add feature flag for this after bumping deno
    // bevy_mod_debugdump::print_main_schedule(&mut app);
    #[cfg(not(feature = "console"))]
    log_panics::init();

    app.add_systems(PreUpdate, check_done);

    app.run();
}

fn check_done(
    q: Query<(), With<ImposterMissing>>,
    realm: Res<CurrentRealm>,
    pointers: Res<ScenePointers>,
    mut counter: Local<usize>,
    config: Res<AppConfig>,
    mut exit: EventWriter<AppExit>,
) {
    // wait for realm
    if realm.address.is_empty() {
        *counter = 0;
        return;
    }

    // wait for pointers
    if pointers.get(config.location).is_none() {
        *counter = 0;
        return;
    }
    if !pointers.is_full() {
        *counter = 0;
        return;
    }

    // wait till nothing missing
    if q.is_empty() {
        *counter += 1;
        if *counter == 10 {
            println!("all done!");
            exit.send_default();
        }
    } else {
        *counter = 0;
    }
}

fn setup(
    mut commands: Commands,
    mut player_resource: ResMut<PrimaryPlayerRes>,
    mut cam_resource: ResMut<PrimaryCameraRes>,
    config: Res<AppConfig>,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
) {
    info!("main::setup");
    // create the main player
    let player_id = commands
        .spawn((
            SpatialBundle {
                transform: Transform::from_translation(Vec3::new(
                    8.0 + 16.0 * config.location.x as f32,
                    8.0,
                    -8.0 + -16.0 * config.location.y as f32,
                )),
                ..Default::default()
            },
            config.player_settings.clone(),
            OutOfWorld,
            AvatarDynamicState::default(),
            GroundCollider::default(),
        ))
        .id();

    let camera_id = commands
        .spawn((Camera3dBundle::default(), PrimaryCamera::default()))
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
    current_profile.is_deployed = true;
}
