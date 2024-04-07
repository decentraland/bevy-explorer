use std::{f32::consts::PI, path::PathBuf};

use attach::AttachPlugin;
use avatar_texture::AvatarTexturePlugin;
use bevy::{
    asset::{io::AssetReader, AsyncReadExt},
    gltf::Gltf,
    prelude::*,
    render::{
        mesh::skinning::SkinnedMesh,
        view::{NoFrustumCulling, RenderLayers},
    },
    scene::InstanceId,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bevy_console::ConsoleCommand;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use collectibles::{
    base_wearables,
    emotes::AvatarAnimations,
    wearables::{
        RequestedWearables, WearableCategory, WearableDefinition, WearablePointers, WearableUrn,
    },
};
use colliders::AvatarColliderPlugin;
use console::DoAddConsoleCommand;
use npc_dynamics::NpcMovementPlugin;
use scene_material::{SceneMaterial, SceneMaterialExt};

pub mod animate;
pub mod attach;
pub mod avatar_texture;
pub mod colliders;
pub mod foreign_dynamics;
pub mod mask_material;
pub mod npc_dynamics;

use common::{
    sets::SetupSets,
    structs::{AppConfig, AttachPoints, PrimaryUser},
    util::{DespawnWith, TryPushChildrenEx},
};
use comms::{
    global_crdt::{ForeignPlayer, GlobalCrdtState},
    profile::UserProfile,
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::Color3,
        sdk::components::{PbAvatarBase, PbAvatarEquippedData, PbAvatarShape},
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, IpfsAssetServer,
};
use scene_runner::{
    update_world::{billboard::Billboard, AddCrdtInterfaceExt},
    util::ConsoleRelay,
    ContainingScene, SceneEntity,
};
use world_ui::{spawn_world_ui_view, WorldUi};

use crate::{animate::AvatarAnimPlayer, avatar_texture::PRIMARY_AVATAR_RENDERLAYER};

use self::{
    animate::AvatarAnimationPlugin,
    foreign_dynamics::PlayerMovementPlugin,
    mask_material::{MaskMaterial, MaskMaterialPlugin},
};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaskMaterialPlugin);
        app.add_plugins(PlayerMovementPlugin);
        app.add_plugins(NpcMovementPlugin);
        app.add_plugins(AvatarAnimationPlugin);
        app.add_plugins(AttachPlugin);
        app.add_plugins(AvatarColliderPlugin);
        app.add_plugins(AvatarTexturePlugin);
        app.add_systems(
            Update,
            (
                update_avatar_info,
                update_base_avatar_shape,
                select_avatar,
                update_render_avatar,
                spawn_scenes,
                process_avatar,
                set_avatar_visibility,
            ),
        );

        app.insert_resource(AvatarWorldUi {
            view: Entity::PLACEHOLDER,
            ui_root: Entity::PLACEHOLDER,
        });
        app.add_systems(Startup, setup.in_set(SetupSets::Main));

        app.add_crdt_lww_component::<PbAvatarShape, AvatarShape>(
            SceneComponentId::AVATAR_SHAPE,
            ComponentPosition::Any,
        );

        app.add_console_command::<DebugDumpAvatar, _>(debug_dump_avatar);
    }
}

#[derive(Resource)]
struct AvatarWorldUi {
    view: Entity,
    ui_root: Entity,
}

fn setup(mut commands: Commands, images: ResMut<Assets<Image>>, mut view: ResMut<AvatarWorldUi>) {
    view.view = spawn_world_ui_view(&mut commands, images.into_inner());
    view.ui_root = commands
        .spawn((
            NodeBundle {
                style: Style {
                    width: Val::Px(0.0),
                    min_width: Val::Px(0.0),
                    max_width: Val::Px(0.0),
                    max_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    ..Default::default()
                },
                ..Default::default()
            },
            TargetCamera(view.view),
        ))
        .id();
}

#[derive(Component, Default)]
pub struct AvatarDynamicState {
    pub velocity: Vec3,
    pub ground_height: f32,
}

// send received avatar info into scenes
fn update_avatar_info(
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
                emotes_urns: avatar
                    .emotes
                    .as_ref()
                    .unwrap_or(&Vec::default())
                    .iter()
                    .map(|emote| emote.urn.clone())
                    .collect(),
            },
        )
    }
}

#[derive(Component, Clone)]
pub struct AvatarShape(pub PbAvatarShape);

impl From<PbAvatarShape> for AvatarShape {
    fn from(value: PbAvatarShape) -> Self {
        Self(value)
    }
}

impl From<&UserProfile> for AvatarShape {
    fn from(profile: &UserProfile) -> Self {
        AvatarShape(PbAvatarShape {
            id: profile.content.eth_address.clone(),
            // add label only for foreign players
            name: Some(profile.content.name.to_owned()),
            body_shape: Some(
                profile
                    .content
                    .avatar
                    .body_shape
                    .to_owned()
                    .unwrap_or(base_wearables::default_bodyshape_urn().to_string()),
            ),
            skin_color: Some(
                profile
                    .content
                    .avatar
                    .skin
                    .map(|skin| skin.color)
                    .unwrap_or(Color3 {
                        r: 0.6,
                        g: 0.462,
                        b: 0.356,
                    }),
            ),
            hair_color: Some(
                profile
                    .content
                    .avatar
                    .hair
                    .map(|hair| hair.color)
                    .unwrap_or(Color3 {
                        r: 0.283,
                        g: 0.142,
                        b: 0.0,
                    }),
            ),
            eye_color: Some(
                profile
                    .content
                    .avatar
                    .eyes
                    .map(|eye| eye.color)
                    .unwrap_or(Color3 {
                        r: 0.6,
                        g: 0.462,
                        b: 0.356,
                    }),
            ),
            expression_trigger_id: None,
            expression_trigger_timestamp: None,
            talking: None,
            wearables: profile.content.avatar.wearables.to_vec(),
            emotes: profile
                .content
                .avatar
                .emotes
                .as_ref()
                .map(|emotes| {
                    emotes
                        .iter()
                        .map(|emote| emote.urn.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        })
    }
}

// set (foreign) user's default avatar shape based on profile data
fn update_base_avatar_shape(
    mut commands: Commands,
    root_avatar_defs: Query<(Entity, Option<&ForeignPlayer>, &UserProfile), Changed<UserProfile>>,
) {
    for (ent, maybe_player, profile) in &root_avatar_defs {
        let id = match maybe_player {
            Some(player) => player.scene_id,
            None => SceneEntityId::PLAYER,
        };

        debug!("updating default avatar for {id}");

        let mut avatar_shape = AvatarShape::from(profile);

        // show label only for other players
        if maybe_player.is_none() {
            avatar_shape.0.name = None;
        }

        commands.entity(ent).try_insert(avatar_shape);
    }
}

#[derive(Component)]
pub struct AvatarSelection {
    scene: Option<Entity>,
    shape: PbAvatarShape,
    render_layers: Option<RenderLayers>,
    automatic_delete: bool,
}

// choose the avatar shape based on current scene of the player
#[allow(clippy::type_complexity)]
fn select_avatar(
    mut commands: Commands,
    mut root_avatar_defs: Query<
        (
            Entity,
            Option<&ForeignPlayer>,
            &AvatarShape,
            Ref<AvatarShape>,
            Option<&mut AvatarSelection>,
        ),
        Or<(With<ForeignPlayer>, With<PrimaryUser>)>,
    >,
    scene_avatar_defs: Query<(Entity, &SceneEntity, &AvatarShape, Ref<AvatarShape>)>,
    orphaned_avatar_selections: Query<(Entity, &AvatarSelection), Without<AvatarShape>>,
    containing_scene: ContainingScene,
) {
    struct AvatarUpdate {
        base_name: String,
        update_shape: Option<PbAvatarShape>,
        active_scenes: HashSet<Entity>,
        prev_source: Option<Entity>,
        current_source: Option<Entity>,
    }

    let mut updates = HashMap::default();

    // set up initial state
    for (entity, maybe_player, base_shape, ref_avatar, maybe_prev_selection) in
        root_avatar_defs.iter()
    {
        let id = maybe_player
            .map(|p| p.scene_id)
            .unwrap_or(SceneEntityId::PLAYER);
        updates.insert(
            id,
            AvatarUpdate {
                base_name: base_shape.0.name.clone().unwrap_or_else(|| "Guest".into()),
                update_shape: ref_avatar.is_changed().then_some(base_shape.0.clone()),
                active_scenes: containing_scene.get(entity),
                prev_source: maybe_prev_selection
                    .as_ref()
                    .map(|prev| prev.scene)
                    .unwrap_or_default(),
                current_source: None,
            },
        );
    }

    for (ent, scene_ent, scene_avatar_shape, ref_avatar) in scene_avatar_defs.iter() {
        let Some(update) = updates.get_mut(&scene_ent.id) else {
            // this is an NPC avatar, attach selection immediately
            if ref_avatar.is_changed() {
                commands.entity(ent).try_insert(AvatarSelection {
                    scene: Some(scene_ent.root),
                    shape: PbAvatarShape {
                        name: Some(
                            scene_avatar_shape
                                .0
                                .name
                                .clone()
                                .unwrap_or_else(|| "NPC".into()),
                        ),
                        ..scene_avatar_shape.0.clone()
                    },
                    render_layers: None,
                    automatic_delete: true,
                });

                debug!("npc avatar {:?}", scene_ent);
            }

            continue;
        };

        if !update.active_scenes.contains(&scene_ent.root) {
            continue;
        }

        // this is the source
        update.current_source = Some(ent);

        if ref_avatar.is_changed() || update.prev_source != update.current_source {
            // and it needs to be updated
            update.update_shape = Some(PbAvatarShape {
                name: Some(
                    scene_avatar_shape
                        .0
                        .name
                        .clone()
                        .unwrap_or_else(|| update.base_name.clone()),
                ),
                ..scene_avatar_shape.0.clone()
            });
        } else {
            // doesn't need to be updated, even if the base shape changed
            // TODO this probably won't pick up name changes though ?
            update.update_shape = None;
        }
    }

    // update avatar selection on foreign players
    for (entity, maybe_player, base_shape, _, maybe_prev_selection) in root_avatar_defs.iter_mut() {
        let id = maybe_player
            .map(|p| p.scene_id)
            .unwrap_or(SceneEntityId::PLAYER);
        let Some(update) = updates.remove(&id) else {
            error!("apparently i never inserted {id}?!");
            continue;
        };
        let needs_update =
            update.current_source != update.prev_source || update.update_shape.is_some();

        if needs_update {
            debug!(
                "updating selected avatar for {} -> {:?}",
                id, update.current_source
            );

            let shape = update.update_shape.unwrap_or(base_shape.0.clone());
            if let Some(mut selection) = maybe_prev_selection {
                selection.shape = shape;
                selection.scene = update.current_source;
            } else {
                commands.entity(entity).try_insert(AvatarSelection {
                    scene: update.current_source,
                    shape,
                    render_layers: if maybe_player.is_none() {
                        Some(PRIMARY_AVATAR_RENDERLAYER)
                    } else {
                        None
                    },
                    automatic_delete: true,
                });
            }
        }
    }

    // remove any orphans
    for (entity, selection) in &orphaned_avatar_selections {
        if selection.automatic_delete {
            if let Some(mut commands) = commands.get_entity(entity) {
                commands.remove::<AvatarSelection>();
            }
        }
    }
}

#[derive(Component)]
pub struct AvatarDefinition {
    label: Option<String>,
    body: WearableDefinition,
    skin_color: Color,
    hair_color: Color,
    eyes_color: Color,
    wearables: Vec<WearableDefinition>,
    hides: HashSet<WearableCategory>,
    render_layer: Option<RenderLayers>,
}

#[derive(Component)]
pub struct RetryRenderAvatar;

// load wearables and create renderable avatar entity once all loaded
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_render_avatar(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &AvatarSelection,
            Option<&Children>,
            Option<&AttachPoints>,
        ),
        Or<(Changed<AvatarSelection>, With<RetryRenderAvatar>)>,
    >,
    mut removed_selections: RemovedComponents<AvatarSelection>,
    children: Query<(&Children, &AttachPoints)>,
    avatar_render_entities: Query<(), With<AvatarDefinition>>,
    wearable_pointers: Res<WearablePointers>,
    ipfas: IpfsAssetServer,
    mut requested_wearables: ResMut<RequestedWearables>,
) {
    let mut missing_wearables = HashSet::default();

    // remove renderable entities when avatar selection is removed
    for entity in removed_selections.read() {
        if let Ok((children, attach_points)) = children.get(entity) {
            // reparent attach points
            commands
                .entity(entity)
                .try_push_children(&attach_points.entities());
            for render_child in children
                .iter()
                .filter(|child| avatar_render_entities.get(**child).is_ok())
            {
                commands.entity(*render_child).despawn_recursive();
            }
        }
    }

    for (entity, selection, maybe_children, maybe_attach_points) in &query {
        commands.entity(entity).remove::<RetryRenderAvatar>();

        debug!("updating render avatar");
        if let Some(attach_points) = maybe_attach_points {
            // reparent attach points
            commands
                .entity(entity)
                .try_push_children(&attach_points.entities());
        }

        // remove existing children
        if let Some(children) = maybe_children {
            for render_child in children
                .iter()
                .filter(|child| avatar_render_entities.get(**child).is_ok())
            {
                commands.entity(*render_child).despawn_recursive();
            }
        }

        // get body shape
        let body = selection
            .shape
            .body_shape
            .as_ref()
            .map(WearableUrn::new)
            .unwrap_or(base_wearables::default_bodyshape_urn());

        let body_data = match wearable_pointers.get(&body) {
            Some(Ok(data)) => data,
            Some(Err(())) => {
                debug!("failed to resolve body {body}");
                // don't retry
                continue;
            }
            None => {
                debug!("waiting for hash from body {body}");
                missing_wearables.insert(body);
                commands.entity(entity).try_insert(RetryRenderAvatar);
                continue;
            }
        };
        let body_shape_str = &body_data.meta.data.representations[0].body_shapes[0].to_lowercase();

        let ext = body_data.meta.data.representations[0]
            .main_file
            .rsplit_once('.')
            .unwrap()
            .1;
        if ext != "glb" {
            panic!("{ext}");
        }

        // get wearables
        let mut all_loaded = true;
        let wearable_datas: Vec<_> = selection
            .shape
            .wearables
            .iter()
            .map(WearableUrn::new)
            .flat_map(|wearable| match wearable_pointers.get(&wearable) {
                Some(Ok(data)) => Some(data),
                Some(Err(())) => {
                    debug!("skipping failed wearable {wearable:?}");
                    None
                }
                None => {
                    commands.entity(entity).try_insert(RetryRenderAvatar);
                    debug!("waiting for hash from wearable {wearable:?}");
                    all_loaded = false;
                    missing_wearables.insert(wearable);
                    None
                }
            })
            .collect();

        if !all_loaded {
            continue;
        }

        // load wearable gtlf/images
        let body_wearable = match WearableDefinition::new(body_data, &ipfas, "") {
            Some(body) => body,
            None => {
                warn!("failed to load body shape, can't render");
                return;
            }
        };

        let mut wearables = wearable_datas
            .into_iter()
            .flat_map(|data| WearableDefinition::new(data, &ipfas, body_shape_str))
            .map(|defn| (defn.category, defn))
            .collect::<HashMap<_, _>>();

        // add defaults
        let defaults: Vec<_> = base_wearables::default_wearables(&body)
            .flat_map(|default| {
                let Some(Ok(data)) = wearable_pointers.get(default.base()) else {
                    warn!("failed to load default renderable {:?}", default);
                    return None;
                };
                WearableDefinition::new(data, &ipfas, body_shape_str)
            })
            .collect();

        for default in defaults {
            if !wearables.contains_key(&default.category) {
                wearables.insert(default.category, default);
            }
        }

        // remove hidden
        let hides = HashSet::from_iter(wearables.values().flat_map(|w| w.hides.iter()).copied());
        wearables.retain(|cat, _| !hides.contains(cat));

        debug!("avatar definition loaded: {wearables:?}");
        commands.entity(entity).with_children(|commands| {
            commands.spawn((
                SpatialBundle {
                    transform: Transform::from_rotation(Quat::from_rotation_y(PI)),
                    ..Default::default()
                },
                AvatarDefinition {
                    label: selection.shape.name.as_ref().map(|name| {
                        format!(
                            "{}#{}",
                            name,
                            selection
                                .shape
                                .id
                                .chars()
                                .skip(selection.shape.id.len().saturating_sub(4))
                                .collect::<String>()
                        )
                    }),
                    body: body_wearable,
                    wearables: wearables.into_values().collect(),
                    hides,
                    skin_color: selection
                        .shape
                        .skin_color
                        .unwrap_or(Color3 {
                            r: 0.6,
                            g: 0.462,
                            b: 0.356,
                        })
                        .into(),
                    hair_color: selection
                        .shape
                        .hair_color
                        .unwrap_or(Color3 {
                            r: 0.283,
                            g: 0.142,
                            b: 0.0,
                        })
                        .into(),
                    eyes_color: selection
                        .shape
                        .eye_color
                        .unwrap_or(Color3 {
                            r: 0.6,
                            g: 0.462,
                            b: 0.356,
                        })
                        .into(),
                    render_layer: selection.render_layers,
                },
            ));
        });
    }

    requested_wearables.0.extend(missing_wearables);
}

#[derive(Component)]
pub struct AvatarLoaded {
    body_instance: InstanceId,
    wearable_instances: Vec<Option<InstanceId>>,
    skin_materials: HashSet<Handle<StandardMaterial>>,
    hair_materials: HashSet<Handle<StandardMaterial>>,
}
#[derive(Component)]
pub struct AvatarProcessed;

// instantiate avatar gltfs
#[allow(clippy::type_complexity)]
fn spawn_scenes(
    mut commands: Commands,
    query: Query<(Entity, &AvatarDefinition), (Without<AvatarLoaded>, Without<AvatarProcessed>)>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut scene_spawner: ResMut<SceneSpawner>,
) {
    for (ent, def) in query.iter() {
        let any_loading = def
            .body
            .model
            .iter()
            .chain(
                def.wearables
                    .iter()
                    .flat_map(|wearable| wearable.model.as_ref()),
            )
            .any(|h_model| {
                matches!(
                    asset_server.get_load_state(h_model),
                    Some(bevy::asset::LoadState::Loading)
                )
            });

        if any_loading {
            continue;
        }

        let Some(gltf) = def.body.model.as_ref().and_then(|h_gltf| gltfs.get(h_gltf)) else {
            match def
                .body
                .model
                .as_ref()
                .and_then(|h_gtlf| asset_server.get_load_state(h_gtlf))
            {
                Some(bevy::asset::LoadState::Loading) | Some(bevy::asset::LoadState::NotLoaded) => {
                    // nothing to do
                }
                otherwise => {
                    warn!("failed to load body gltf: {otherwise:?}");
                    commands.entity(ent).try_insert(AvatarProcessed);
                }
            }
            continue;
        };

        let Some(h_scene) = gltf.default_scene.as_ref() else {
            warn!("body gltf has no default scene");
            commands.entity(ent).try_insert(AvatarProcessed);
            continue;
        };

        // store skin/hair materials to update with color on spawned instance
        let mut skin_materials = HashSet::default();
        let mut hair_materials = HashSet::default();

        skin_materials.extend(
            gltf.named_materials
                .iter()
                .filter(|(name, _)| name.to_lowercase().contains("skin"))
                .map(|(_, h)| h.clone_weak()),
        );

        hair_materials.extend(
            gltf.named_materials
                .iter()
                .filter(|(name, _)| name.to_lowercase().contains("hair"))
                .map(|(_, h)| h.clone_weak()),
        );

        let body_instance = scene_spawner.spawn_as_child(h_scene.clone(), ent);

        let instances = def
            .wearables
            .iter()
            .flat_map(|wearable| &wearable.model)
            .flat_map(|h_gltf| {
                match asset_server.get_load_state(h_gltf) {
                    Some(bevy::asset::LoadState::Loaded) => (),
                    otherwise => {
                        warn!("wearable gltf didn't work out: {otherwise:?}");
                        return None;
                    }
                }

                let gltf = gltfs.get(h_gltf).unwrap();
                let gltf_scene_handle = gltf.default_scene.as_ref();

                // store skin/hair materials
                skin_materials.extend(
                    gltf.named_materials
                        .iter()
                        .filter(|(name, _)| name.to_lowercase().contains("skin"))
                        .map(|(_, h)| h.clone_weak()),
                );
                hair_materials.extend(
                    gltf.named_materials
                        .iter()
                        .filter(|(name, _)| name.to_lowercase().contains("hair"))
                        .map(|(_, h)| h.clone_weak()),
                );

                Some(
                    gltf_scene_handle
                        .map(|h_scene| scene_spawner.spawn_as_child(h_scene.clone(), ent)),
                )
            });

        debug!("avatar files loaded");
        commands.entity(ent).try_insert((
            AvatarLoaded {
                body_instance,
                wearable_instances: instances.collect(),
                skin_materials,
                hair_materials,
            },
            Visibility::Hidden,
        ));
    }
}

// update materials and hide base parts
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn process_avatar(
    mut commands: Commands,
    query: Query<(Entity, &AvatarDefinition, &AvatarLoaded, &Parent), Without<AvatarProcessed>>,
    scene_spawner: Res<SceneSpawner>,
    mut instance_ents: Query<(
        &mut Visibility,
        &Parent,
        Option<&Handle<StandardMaterial>>,
        Option<&Handle<Mesh>>,
    )>,
    named_ents: Query<&Name>,
    mut skins: Query<&mut SkinnedMesh>,
    standard_materials: Res<Assets<StandardMaterial>>,
    mut scene_materials: ResMut<Assets<SceneMaterial>>,
    mut mask_materials: ResMut<Assets<MaskMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    attach_points: Query<&AttachPoints>,
    animations: Res<AvatarAnimations>,
    ui_view: Res<AvatarWorldUi>,
    dui: Res<DuiRegistry>,
) {
    for (avatar_ent, def, loaded_avatar, root_player_entity) in query.iter() {
        let not_loaded = !scene_spawner.instance_is_ready(loaded_avatar.body_instance)
            || loaded_avatar
                .wearable_instances
                .iter()
                .any(|maybe_instance| {
                    maybe_instance
                        .map_or(false, |instance| !scene_spawner.instance_is_ready(instance))
                });

        if not_loaded {
            debug!("not loaded...");
            continue;
        }

        let mut instance_scene_materials = HashMap::default();
        let mut armature_node = None;
        let mut target_armature_entities = HashMap::default();

        // hide and colour the base model
        for scene_ent in scene_spawner.iter_instance_entities(loaded_avatar.body_instance) {
            if let Some(layer) = def.render_layer {
                // set render layer for primary avatar
                commands.entity(scene_ent).try_insert(layer);
            }

            let Ok((mut vis, parent, maybe_h_mat, maybe_h_mesh)) = instance_ents.get_mut(scene_ent)
            else {
                continue;
            };

            let Ok(name) = named_ents.get(scene_ent) else {
                continue;
            };
            let name = name.to_lowercase();

            // add animation player to armature root
            if name.to_lowercase() == "armature" && armature_node.is_none() {
                let mut player = AnimationPlayer::default();
                // play default idle anim to avoid t-posing
                if let Some(clip) = animations
                    .get_server("Idle_Male")
                    .and_then(|anim| anim.clips.values().next())
                {
                    player.start(clip.clone());
                }
                commands.entity(scene_ent).try_insert(player);
                // record the node with the animator
                commands
                    .entity(root_player_entity.get())
                    .try_insert(AvatarAnimPlayer(scene_ent));
                armature_node = Some(scene_ent);
            }

            // record bone entities
            if name.to_lowercase().starts_with("avatar_") {
                target_armature_entities.insert(name.to_lowercase(), scene_ent);
            }

            if let Some(h_mesh) = maybe_h_mesh {
                if let Some(mesh_data) = meshes.get(h_mesh) {
                    let is_skinned = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some();
                    if is_skinned {
                        commands.entity(scene_ent).try_insert(NoFrustumCulling);
                    }
                } else {
                    warn!("missing mesh for wearable, removing frustum culling just in case");
                    commands.entity(scene_ent).try_insert(NoFrustumCulling);
                }
            }

            if let Some(h_mat) = maybe_h_mat {
                commands
                    .entity(scene_ent)
                    .remove::<Handle<StandardMaterial>>();

                if let Some(mat) = standard_materials.get(h_mat) {
                    let base_color = if loaded_avatar.skin_materials.contains(h_mat) {
                        def.skin_color
                    } else if loaded_avatar.hair_materials.contains(h_mat) {
                        def.hair_color
                    } else {
                        mat.base_color
                    };

                    let new_mat = SceneMaterial::unbounded(StandardMaterial {
                        base_color,
                        ..mat.clone()
                    });
                    let instance_mat = instance_scene_materials
                        .entry(h_mat.clone_weak())
                        .or_insert_with(|| scene_materials.add(new_mat));
                    commands.entity(scene_ent).try_insert(instance_mat.clone());
                }
            }

            let masks = [
                ("mask_eyes", def.eyes_color, WearableCategory::EYES, true),
                (
                    "mask_eyebrows",
                    def.hair_color,
                    WearableCategory::EYEBROWS,
                    false,
                ),
                ("mask_mouth", def.skin_color, WearableCategory::MOUTH, false),
            ];

            let Ok(parent_name) = named_ents.get(parent.get()) else {
                continue;
            };
            let parent_name = parent_name.to_lowercase();

            debug!("parent: {parent_name}");

            for (suffix, color, category, no_mask_means_ignore_color) in masks.into_iter() {
                if parent_name.ends_with(suffix) {
                    *vis = Visibility::Hidden;

                    if let Some(WearableDefinition { texture, mask, .. }) =
                        def.wearables.iter().find(|w| w.category == category)
                    {
                        debug!("setting {suffix} color {:?}", color);
                        if let Some(mask) = mask.as_ref() {
                            debug!("using mask for {suffix}");
                            let mask_material = mask_materials.add(MaskMaterial {
                                color,
                                base_texture: texture.clone().unwrap(),
                                mask_texture: mask.clone(),
                            });
                            commands
                                .entity(scene_ent)
                                .try_insert(mask_material)
                                .remove::<Handle<SceneMaterial>>();
                        } else {
                            debug!("no mask for {suffix}");
                            let material =
                                scene_materials.add(SceneMaterial::unbounded(StandardMaterial {
                                    base_color: if no_mask_means_ignore_color {
                                        Color::WHITE
                                    } else {
                                        color
                                    },
                                    base_color_texture: texture.clone(),
                                    alpha_mode: AlphaMode::Blend,
                                    ..Default::default()
                                }));
                            commands.entity(scene_ent).try_insert(material);
                        };
                        *vis = Visibility::Inherited;
                    }
                }
            }

            let hiders = [
                ("ubody_basemesh", WearableCategory::UPPER_BODY),
                ("lbody_basemesh", WearableCategory::LOWER_BODY),
                ("feet_basemesh", WearableCategory::FEET),
                // ("head", WearableCategory::HEAD),
                // ("head_basemesh", WearableCategory::HEAD),
                // ("mask_eyes", WearableCategory::HEAD),
                // ("mask_eyebrows", WearableCategory::HEAD),
                // ("mask_mouth", WearableCategory::HEAD),
            ];

            for (hidename, category) in hiders {
                if parent_name.ends_with(hidename) {
                    // todo construct hides better so we don't need to scan the wearables here
                    if def.hides.contains(&category)
                        || def
                            .wearables
                            .iter()
                            .any(|w| w.category == WearableCategory::SKIN || w.category == category)
                    {
                        *vis = Visibility::Hidden;
                    }
                }
            }
        }

        let Some(armature_node) = armature_node else {
            warn!("no armature node!");
            continue;
        };

        if target_armature_entities.is_empty() {
            warn!("boneless body!");
            continue;
        } else {
            // reparent hands
            if let Ok(attach_points) = attach_points.get(root_player_entity.get()) {
                if let Some(left_hand) =
                    target_armature_entities.get(&String::from("avatar_lefthand"))
                {
                    commands
                        .entity(*left_hand)
                        .try_push_children(&[attach_points.left_hand]);
                } else {
                    warn!("no left hand");
                    warn!("available: {:#?}", target_armature_entities.keys());
                }
                if let Some(right_hand) =
                    target_armature_entities.get(&String::from("avatar_righthand"))
                {
                    commands
                        .entity(*right_hand)
                        .try_push_children(&[attach_points.right_hand]);
                } else {
                    warn!("no right hand");
                }
            } else {
                warn!("no attach points");
            }
        }

        // color the components of wearables
        for instance in &loaded_avatar.wearable_instances {
            let Some(instance) = instance else {
                warn!("failed to load instance for wearable");
                continue;
            };

            let mut armature_map = HashMap::default();

            for scene_ent in scene_spawner.iter_instance_entities(*instance) {
                if let Some(layer) = def.render_layer {
                    // set render layer for primary avatar
                    commands.entity(scene_ent).try_insert(layer);
                }

                let Ok((_, parent, maybe_h_mat, maybe_h_mesh)) = instance_ents.get(scene_ent)
                else {
                    continue;
                };

                let Ok(parent_name) = named_ents.get(parent.get()) else {
                    continue;
                };
                let parent_name = parent_name.to_lowercase();

                let Ok(name) = named_ents.get(scene_ent) else {
                    continue;
                };
                let name = name.to_lowercase();

                // record bone entities so we can remap them, and delete this instance
                if name.to_lowercase().starts_with("avatar_") {
                    if let Some(target) = target_armature_entities.get(&name.to_lowercase()) {
                        armature_map.insert(scene_ent, target);
                    }
                    if parent_name == "armature" {
                        commands.entity(scene_ent).despawn_recursive();
                    }
                    continue;
                }

                // move children of the root to the body mesh
                if parent_name.to_lowercase() == "armature" {
                    commands.entity(scene_ent).set_parent(armature_node);
                }

                if let Some(h_mesh) = maybe_h_mesh {
                    if let Some(mesh_data) = meshes.get_mut(h_mesh) {
                        mesh_data.normalize_joint_weights();
                        let is_skinned =
                            mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some();
                        if is_skinned {
                            commands.entity(scene_ent).try_insert(NoFrustumCulling);
                        }
                    } else {
                        warn!("missing mesh for wearable, removing frustum culling just in case");
                        commands.entity(scene_ent).try_insert(NoFrustumCulling);
                    }
                }

                if let Some(h_mat) = maybe_h_mat {
                    commands
                        .entity(scene_ent)
                        .remove::<Handle<StandardMaterial>>();

                    if let Some(mat) = standard_materials.get(h_mat) {
                        let base_color = if loaded_avatar.skin_materials.contains(h_mat) {
                            def.skin_color
                        } else if loaded_avatar.hair_materials.contains(h_mat) {
                            def.hair_color
                        } else {
                            mat.base_color
                        };

                        let new_mat = SceneMaterial::unbounded(StandardMaterial {
                            base_color,
                            ..mat.clone()
                        });
                        let instance_mat = instance_scene_materials
                            .entry(h_mat.clone_weak())
                            .or_insert_with(|| scene_materials.add(new_mat));
                        commands.entity(scene_ent).try_insert(instance_mat.clone());
                    }
                }
            }

            // remap bones
            for scene_ent in scene_spawner.iter_instance_entities(*instance) {
                if let Ok(mut skin) = skins.get_mut(scene_ent) {
                    let joints =
                        skin.joints
                            .iter()
                            .map(|joint| {
                                *armature_map.get(joint).unwrap_or_else(|| {
                            let original_name = named_ents.get(*joint);
                            warn!("missing armature node in wearable mapping: {original_name:?}");
                            armature_map.values().next().unwrap()
                        })
                            })
                            .copied()
                            .collect();
                    skin.joints = joints;
                }
            }
        }

        let wearable_models = def.wearables.iter().filter(|w| w.model.is_some()).count();
        let wearable_texs = def.wearables.iter().filter(|w| w.model.is_none()).count();

        debug!(
            "avatar processed, 1+{} models, {} textures. hides: {:?}, skin mats: {:?}, hair mats: {:?}, used mats: {:?}",
            wearable_models, wearable_texs, def.hides, loaded_avatar.skin_materials.len(), loaded_avatar.hair_materials.len(), instance_scene_materials.len()
        );

        commands
            .entity(avatar_ent)
            .try_insert((AvatarProcessed, Visibility::Inherited));

        if def.render_layer.is_none() {
            // add nametag
            if let Some(label) = def.label.as_ref() {
                debug!("spawn avatar label for {label}");
                let label_ui = commands
                    .entity(ui_view.ui_root)
                    .spawn_template(
                        &dui,
                        "avatar-nametag",
                        DuiProps::new().with_prop("name", label.to_string()),
                    )
                    .unwrap()
                    .root;

                debug!("{:?} as child of {:?}", label_ui, ui_view.view);
                commands.entity(label_ui).insert(DespawnWith(avatar_ent));

                commands.entity(avatar_ent).with_children(|commands| {
                    commands.spawn((
                        SpatialBundle {
                            transform: Transform::from_translation(Vec3::Y * 2.2),
                            ..Default::default()
                        },
                        WorldUi {
                            dbg: label.clone(),
                            pix_per_m: 200.0,
                            valign: 0.0,
                            halign: 0.0,
                            add_y_pix: 0.0,
                            bounds: Vec4::new(
                                std::f32::MIN,
                                std::f32::MIN,
                                std::f32::MAX,
                                std::f32::MAX,
                            ),
                            view: ui_view.view,
                            ui_node: label_ui,
                        },
                        Billboard::Y,
                    ));
                });
            }
        }
    }
}

fn set_avatar_visibility(
    mut q: Query<(&GlobalTransform, &mut Visibility, Option<&RenderLayers>), With<AvatarProcessed>>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    config: Res<AppConfig>,
) {
    let Ok(player_pos) = player.get_single().map(|gt| gt.translation()) else {
        return;
    };

    let default_layer = RenderLayers::layer(0);
    let mut distances = q
        .iter()
        .filter(|(_, _, maybe_layer)| {
            maybe_layer.map_or(true, |layer| layer.intersects(&default_layer))
        })
        .map(|(t, ..)| (t.translation() - player_pos).length_squared())
        .collect::<Vec<_>>();
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less));
    let cutoff = distances
        .get(config.max_avatars)
        .copied()
        .unwrap_or(f32::MAX);

    for (t, mut vis, maybe_layer) in q.iter_mut() {
        let is_root_layer = maybe_layer.map_or(true, |layer| layer.intersects(&default_layer));
        *vis = if is_root_layer && (t.translation() - player_pos).length_squared() >= cutoff {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_dump_avatar")]
struct DebugDumpAvatar;

#[allow(clippy::too_many_arguments)]
fn debug_dump_avatar(
    mut input: ConsoleCommand<DebugDumpAvatar>,
    player: Query<&AvatarShape, With<PrimaryUser>>,
    ipfas: IpfsAssetServer,
    entity_definitions: Res<Assets<EntityDefinition>>,
    mut tasks: Local<Vec<Task<()>>>,
    console_relay: Res<ConsoleRelay>,
    wearable_pointers: Res<WearablePointers>,
    mut store: Local<HashSet<Handle<EntityDefinition>>>,
) {
    if let Some(Ok(_)) = input.take() {
        let Ok(shape) = player.get_single() else {
            return;
        };

        let hashes: Vec<_> = shape
            .0
            .wearables
            .iter()
            .flat_map(
                |wearable| match wearable_pointers.hash(WearableUrn::new(wearable)) {
                    Some(Ok(hash)) => Some(hash),
                    _ => None,
                },
            )
            .collect();

        for scene_hash in hashes {
            let h_scene = ipfas.load_hash::<EntityDefinition>(scene_hash);
            let Some(def) = entity_definitions.get(&h_scene) else {
                input.reply_failed("can't resolve wearable handle - try again in a few seconds");
                store.insert(h_scene);
                continue;
            };

            let dump_folder = ipfas
                .ipfs()
                .cache_path()
                .to_owned()
                .join("scene_dump")
                .join("wearables")
                .join(scene_hash);
            std::fs::create_dir_all(&dump_folder).unwrap();

            // total / succeed / fail
            let count = std::sync::Arc::new(std::sync::Mutex::new((0, 0, 0)));

            for content_file in def.content.files() {
                count.lock().unwrap().0 += 1;
                let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                    scene_hash.to_owned(),
                    content_file.to_owned(),
                ));

                let path = PathBuf::from(&ipfs_path);

                let ipfs = ipfas.ipfs().clone();
                let content_file = content_file.clone();
                let dump_folder = dump_folder.clone();
                let count = count.clone();
                let send = console_relay.send.clone();
                tasks.push(IoTaskPool::get().spawn(async move {
                    let report = |fail: Option<String>| {
                        let mut count = count.lock().unwrap();
                        if let Some(fail) = fail {
                            count.2 += 1;
                            let _ = send.send(fail.into());
                        } else {
                            count.1 += 1;
                        }
                        if count.0 == count.1 + count.2 {
                            if count.2 == 0 {
                                let _ =
                                    send.send(format!("[ok] {} files downloaded", count.0).into());
                            } else {
                                let _ = send.send(
                                    format!("[failed] {}/{} files downloaded", count.1, count.0)
                                        .into(),
                                );
                            }
                        }
                    };

                    let Ok(mut reader) = ipfs.read(&path).await else {
                        report(Some(format!(
                            "{content_file} failed: couldn't load bytes\n"
                        )));
                        return;
                    };
                    let mut bytes = Vec::default();
                    if let Err(e) = reader.read_to_end(&mut bytes).await {
                        report(Some(format!("{content_file} failed: {e}")));
                        return;
                    }

                    let file = dump_folder.join(&content_file);
                    if let Some(parent) = file.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            report(Some(format!(
                                "{content_file} failed: couldn't create parent: {e}"
                            )));
                            return;
                        }
                    }
                    if let Err(e) = std::fs::write(file, bytes) {
                        report(Some(format!("{content_file} failed: {e}")));
                        return;
                    }

                    report(None);
                }));
            }

            input.reply(format!(
                "scene hash {}, downloading {} files",
                scene_hash,
                tasks.len()
            ));
        }
    }

    tasks.retain_mut(|t| !t.is_finished());
}
