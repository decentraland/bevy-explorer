//! The render-free app: the plugin set proven by `scene_runner`'s integration tests
//! (`TestPlugins` + `init_test_app`), plus the realm/preview loading and comms the real
//! explorer uses. Each entry adds its own log, task pool and runner first.

use avatar::AvatarCorePlugin;
use bevy::{
    diagnostic::{DiagnosticsPlugin, FrameCountPlugin},
    gizmos::GizmoPlugin,
    gltf::GltfPlugin,
    input::InputPlugin,
    prelude::*,
    render::mesh::MeshPlugin,
    scene::ScenePlugin,
    state::app::StatesPlugin,
    time::TimePlugin,
};
use bevy_dui::DuiPlugin;
use collectibles::EmoteMetadataPlugin;
use common::{
    inputs::InputMap,
    profile::SerializedProfile,
    rpc::RpcCallEvent,
    sets::SetupSets,
    structs::{
        AppConfig, AvatarDynamicState, CursorLocks, EngineMovementControl, GraphicsSettings,
        HeadSync, NoRenderApp, PermissionType, PermissionUsed, PermissionValue, PointAtSync,
        PrimaryCamera, PrimaryCameraRes, PrimaryPlayerRes, PrimaryUser, SceneGlobalLight,
        SceneLoadDistance, SystemAudio, TimeOfDay, ToolTips,
    },
    util::UtilsPlugin,
};
use comms::{
    profile::{CurrentUserProfile, ProfileCache, UserProfile},
    CommsPlugin,
};
use console::ConsolePlugin;
use input_manager::{CumulativeAxisData, InputPriorities};
use ipfs::{map_realm_name, IpfsIoPlugin};
use nft::asset_source::Nft;
use restricted_actions::RestrictedActionsPlugin;
use scene_material::SceneBoundPlugin;
use scene_runner::{
    initialize_scene::PARCEL_SIZE, permissions::PermissionManager, SceneRunnerPlugin,
};
use system_api_types::launch_options::LaunchOptions;
use system_bridge::SystemBridgePlugin;
use tween::TweenPlugin;
use user_input::avatar_movement::{
    ActivePlayerComponent, AvatarMovement, AvatarMovementInfo, FromConfig, GroundCollider,
};
use wallet::{delegation::StorageDelegations, Wallet, WalletPlugin};

/// The app config of a headless run. `scene_log_to_console` is off where scene console output
/// is served another way (an orchestrated server's `@scene-log` frames).
pub fn config(
    realm: &str,
    location: IVec2,
    tick_hz: u32,
    scene_threads: usize,
    scene_log_to_console: bool,
) -> AppConfig {
    AppConfig {
        home_realm: Some(realm.to_owned()),
        home_location: Some(location),
        graphics: GraphicsSettings {
            vsync: false,
            log_fps: false,
            fps_target: tick_hz as usize,
            ..Default::default()
        },
        scene_threads,

        // load everything around the fake player generously; unload never.
        scene_load_distance: 100.0,
        scene_unload_extra_distance: 0.0,
        scene_log_to_console,
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
    }
}

/// Adds everything but the log, task pool and runner. `wallet` is finalized (a guest);
/// `server_mode` is whether this run serves its scenes (`common::structs::server_mode`,
/// latched by the caller).
pub fn assemble(
    app: &mut App,
    config: AppConfig,
    launch: &LaunchOptions,
    realm: &str,
    server_mode: bool,
    wallet: Wallet,
) {
    // Skip the render-only scene plugins (scene UI, TextShape, scene materials, billboards,
    // lights, visibility, pointer results) and comms' microphone and video. Must precede the
    // plugins: the gates are read at plugin-build time.
    app.insert_resource(NoRenderApp);

    // ---- render-free plugin set (mirrors scene_runner TestPlugins) ----
    app.add_plugins(FrameCountPlugin)
        .add_plugins(TimePlugin)
        .add_plugins(TransformPlugin)
        .add_plugins(DiagnosticsPlugin)
        .add_plugins(IpfsIoPlugin {
            preview: launch.preview,
            assets_root: None,
            starting_realm: Some(map_realm_name(realm)),
            content_server_override: launch.content_server.clone(),
            num_slots: config.max_concurrent_remotes,
        })
        .add_plugins(AssetPlugin::default())
        .add_plugins(MeshPlugin)
        .add_plugins(
            GltfPlugin::default()
                .with_uri_resolver(std::sync::Arc::new(ipfs::ipfs_path::resolve_content_uri)),
        )
        .add_plugins(AnimationPlugin)
        .add_plugins(InputPlugin)
        .add_plugins(ScenePlugin)
        .add_plugins(StatesPlugin)
        .add_plugins(ConsolePlugin {
            add_bevy_console: false,
        })
        .add_plugins(WalletPlugin)
        .add_plugins(CommsPlugin)
        // emote metadata (the loop flag scenes are told) resolves through the collectible
        // manager; no clip or sound is ever loaded here
        .add_plugins(EmoteMetadataPlugin)
        // the render-free part of the avatar stack: foreign avatar bevy transforms (without
        // these, engine-side position logic — scene membership, avatar colliders, trigger
        // areas — sees every remote player at the origin), profile info and emote reports
        // in scene crdt
        .add_plugins(AvatarCorePlugin)
        .add_plugins(DuiPlugin)
        .add_plugins(SystemBridgePlugin { bare: true });

    // manual asset inits the scene runtime needs (from init_test_app). The text stack
    // (Font/TextPipeline/CosmicFontSystem) and StretchUvMaterial went with the scene-UI
    // and TextShape plugins below.
    app.init_asset::<Shader>()
        .init_asset::<AnimationClip>()
        .init_asset::<Image>()
        .register_asset_loader(StubImageLoader);

    app.add_plugins(MaterialPlugin::<StandardMaterial>::default())
        .add_plugins(GizmoPlugin)
        .add_plugins(UtilsPlugin)
        .add_plugins(SceneRunnerPlugin)
        // tweens drive transforms (moving platforms → colliders), so they run headless
        // like AnimatorPlugin; texture-move no-ops without material components
        .add_plugins(TweenPlugin)
        .add_plugins(SceneBoundPlugin)
        .add_plugins(RestrictedActionsPlugin);

    register_gltf_scene_types(app);

    // embedded assets: the scene-loading material (grid.png / loading.wgsl)
    app.add_plugins(assets::EmbedAssetsPlugin);

    // the shared launch options' resources and plugins (--log-fps logs the tick rate, against
    // --tick-hz)
    crate::launch::apply(app, launch, &config, &map_realm_name(realm));

    // ---- resources & events the scene runtime needs (no render/UI plugins) ----
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
            load: 100.0,
            unload: 0.0,
            load_imposter: 0.0,
        })
        // servers must never join realm-wide comms (archipelago / world room) — they would
        // show up as a ghost participant. A non-server headless follows its realm about's
        // fixed_adapter like any client (offline about ⇒ no comms), so it can act as a
        // synthetic client. Scene rooms are a separate, authoritative-presence concern.
        .insert_resource(comms::DisableRealmComms(server_mode))
        .init_resource::<StorageDelegations>()
        .add_event::<RpcCallEvent>()
        .add_event::<SystemAudio>()
        .add_event::<PermissionUsed>();

    app.configure_sets(Startup, SetupSets::Init.before(SetupSets::Main));
    app.add_systems(Startup, setup.in_set(SetupSets::Init));
    app.add_systems(Update, drain_permissions);
}

/// Stands in for the render-only `ImageLoader`: without it the asset server errors
/// per gltf texture. Nothing samples textures here, so don't decode them.
struct StubImageLoader;

impl bevy::asset::AssetLoader for StubImageLoader {
    type Asset = Image;
    type Settings = bevy::image::ImageLoaderSettings;
    type Error = std::convert::Infallible;

    async fn load(
        &self,
        _reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        Ok(Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[255, 255, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::empty(),
        ))
    }

    fn extensions(&self) -> &[&str] {
        bevy::image::ImageLoader::SUPPORTED_FILE_EXTENSIONS
    }
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
    wallet: Res<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
) {
    // fake player: process_scene_lifecycle early-returns without a PrimaryUser,
    // and PrimaryEntities::player() panics without the marker. Placed at the scene
    // location so position-based loading picks up the parcel scene.
    let player_pos = Vec3::new(
        8.0 + PARCEL_SIZE * config.home_location().x as f32,
        0.0,
        -8.0 + -PARCEL_SIZE * config.home_location().y as f32,
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

    // wallet is already finalized in main(); build the profile from its address
    println!("[headless] guest address: {:#x}", wallet.address().unwrap());
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
