use std::str::FromStr;

use bevy::{
    ecs::system::SystemParam,
    math::Vec3Swizzles,
    prelude::*,
    utils::{HashMap, HashSet}, gltf::Gltf,
};
use serde::{Deserialize, Serialize};
use urn::Urn;

pub mod base_wearables;

use crate::{
    comms::{
        global_crdt::{ForeignPlayer, GlobalCrdtState},
        profile::UserProfile,
    },
    dcl::interface::{ComponentPosition, CrdtType},
    dcl_component::{
        proto_components::{
            common::Color3,
            sdk::components::{
                PbAvatarAttach, PbAvatarCustomization, PbAvatarEquippedData, PbAvatarShape,
            },
        },
        SceneComponentId,
    },
    ipfs::{ActiveEntityTask, IpfsLoaderExt, IpfsModifier},
    scene_runner::{
        initialize_scene::{LiveScenes, PointerResult, ScenePointers, PARCEL_SIZE},
        update_world::AddCrdtInterfaceExt,
        SceneEntity,
    },
    util::TaskExt,
};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WearablePointers>();
        app.init_resource::<WearableMetas>();
        app.add_system(load_base_wearables);
        app.add_system(update_avatar_info);
        app.add_system(update_base_avatar_shape);
        app.add_system(select_avatar);
        app.add_system(update_render_avatar);
        app.add_system(attach_gltfs);

        app.add_crdt_lww_component::<PbAvatarShape, AvatarShape>(
            SceneComponentId::AVATAR_SHAPE,
            ComponentPosition::Any,
        );
        app.add_crdt_lww_component::<PbAvatarAttach, AvatarAttachment>(
            SceneComponentId::AVATAR_ATTACHMENT,
            ComponentPosition::Any,
        );
    }
}

#[derive(Resource, Default, Debug)]
pub struct WearablePointers(HashMap<Urn, String>);

#[derive(Resource, Default, Debug)]
pub struct WearableMetas(HashMap<String, WearableMeta>);

#[derive(Deserialize, Debug)]
pub struct WearableMeta {
    pub description: String,
    pub thumbnail: String,
    pub rarity: String,
    pub data: WearableData,
}

#[derive(Deserialize, Debug)]
pub struct WearableData {
    pub tags: Vec<String>,
    pub category: String,
    pub representations: Vec<WearableRepresentation>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WearableRepresentation {
    pub body_shapes: Vec<String>,
    pub main_file: String,
    pub override_replaces: Vec<String>,
    pub override_hides: Vec<String>,
    pub contents: Vec<String>,
}

fn load_base_wearables(
    mut once: Local<bool>,
    mut task: Local<Option<ActiveEntityTask>>,
    mut wearable_pointers: ResMut<WearablePointers>,
    mut wearable_metas: ResMut<WearableMetas>,
    asset_server: Res<AssetServer>,
) {
    if *once || asset_server.active_endpoint().is_none() {
        return;
    }

    match *task {
        None => {
            let pointers = base_wearables::base_wearables();
            *task = Some(asset_server.ipfs().active_entities(&pointers));
        }
        Some(ref mut active_task) => match active_task.complete() {
            None => (),
            Some(Err(e)) => warn!("failed to acquire base wearables: {e}"),
            Some(Ok(active_entities)) => {
                error!("{:?}", active_entities.get(0));
                for entity in active_entities {
                    asset_server.ipfs().add_collection(entity.id.clone(), entity.content, Some(IpfsModifier{ base_url: Some(base_wearables::URL.to_owned()) }));

                    let Some(metadata) = entity.metadata else {
                        warn!("no metadata on wearable");
                        continue;
                    };
                    let wearable_data = match serde_json::from_value::<WearableMeta>(metadata) {
                        Ok(data) => data,
                        Err(e) => {
                            warn!("failed to deserialize wearable data: {e}");
                            continue;
                        }
                    };
                    for pointer in entity.pointers {
                        match Urn::from_str(&pointer) {
                            Ok(urn) => {
                                wearable_pointers.0.insert(urn, entity.id.clone());
                            }
                            Err(e) => {
                                warn!("failed to parse wearable urn: {e}");
                            }
                        };
                    }

                    wearable_metas.0.insert(entity.id, wearable_data);
                }
                *task = None;
                *once = true;
            }
        },
    }
}

#[derive(Component)]
struct AvatarRenderEntity;

fn update_avatar_info(
    updated_players: Query<(&ForeignPlayer, &UserProfile), Changed<UserProfile>>,
    mut global_state: ResMut<GlobalCrdtState>,
) {
    for (player, profile) in &updated_players {
        let avatar = &profile.content.avatar;
        global_state.update_crdt(
            SceneComponentId::AVATAR_CUSTOMIZATION,
            CrdtType::LWW_ANY,
            player.scene_id,
            &PbAvatarCustomization {
                skin_color: avatar.skin.map(|c| c.color),
                eyes_color: avatar.eyes.map(|c| c.color),
                hair_color: avatar.hair.map(|c| c.color),
                body_shape_urn: avatar
                    .body_shape
                    .as_deref()
                    .map(ToString::to_string)
                    .unwrap_or(base_wearables::base_wearables().remove(0)),
            },
        );
        global_state.update_crdt(
            SceneComponentId::AVATAR_EQUIPPED_DATA,
            CrdtType::LWW_ANY,
            player.scene_id,
            &PbAvatarEquippedData {
                urns: avatar.wearables.to_vec(),
                emotes: avatar.emotes.as_ref().unwrap_or(&Vec::default()).to_vec(),
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

#[derive(Component)]
pub struct AvatarAttachment(pub PbAvatarAttach);

impl From<PbAvatarAttach> for AvatarAttachment {
    fn from(value: PbAvatarAttach) -> Self {
        Self(value)
    }
}

fn update_base_avatar_shape(
    mut commands: Commands,
    root_avatar_defs: Query<(Entity, &ForeignPlayer, &UserProfile), Changed<UserProfile>>,
) {
    for (ent, player, profile) in &root_avatar_defs {
        debug!("updating default avatar for {}", player.scene_id);

        if let Some(mut commands) = commands.get_entity(ent) {
            commands.insert(AvatarShape(PbAvatarShape {
                id: format!("{:#x}", player.address),
                name: Some(profile.content.name.to_owned()),
                body_shape: Some(
                    profile
                        .content
                        .avatar
                        .body_shape
                        .to_owned()
                        .unwrap_or(base_wearables::default_bodyshape()),
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
                eye_color: Some(profile.content.avatar.eyes.map(|eye| eye.color).unwrap_or(
                    Color3 {
                        r: 0.6,
                        g: 0.462,
                        b: 0.356,
                    },
                )),
                expression_trigger_id: None,
                expression_trigger_timestamp: None,
                talking: None,
                wearables: profile.content.avatar.wearables.to_vec(),
                emotes: profile
                    .content
                    .avatar
                    .emotes
                    .as_ref()
                    .map(Clone::clone)
                    .unwrap_or_default(),
            }));
        }
    }
}

#[derive(SystemParam)]
pub struct ContainingScene<'w, 's> {
    transforms: Query<'w, 's, &'static GlobalTransform, With<SceneEntity>>,
    pointers: Res<'w, ScenePointers>,
    live_scenes: Res<'w, LiveScenes>,
}

impl<'w, 's> ContainingScene<'w, 's> {
    fn get(&self, ent: Entity) -> Option<Entity> {
        let parcel = (self
            .transforms
            .get(ent)
            .ok()?
            .translation().xz() / PARCEL_SIZE).as_ivec2();

        if let Some(PointerResult::Exists(hash)) = self.pointers.0.get(&parcel) {
            self.live_scenes.0.get(hash).copied()
        } else {
            None
        }
    }
}

#[derive(Component)]
pub struct AvatarSelection {
    scene: Option<Entity>,
    shape: PbAvatarShape,
}

fn select_avatar(
    mut commands: Commands,
    mut root_avatar_defs: Query<(Entity, &ForeignPlayer, &AvatarShape, Changed<AvatarShape>, Option<&mut AvatarSelection>)>,
    scene_avatar_defs: Query<(Entity, &SceneEntity, &AvatarShape, Changed<AvatarShape>)>,
    containing_scene: ContainingScene,
) {
    struct AvatarUpdate {
        update_shape: Option<PbAvatarShape>,
        active_scene: Option<Entity>,
        prev_source: Option<Entity>,
        current_source: Option<Entity>,
        
    }

    let mut updates = HashMap::default();

    // set up initial state
    for (entity, player, base_shape, changed, maybe_prev_selection) in root_avatar_defs.iter() {
        updates.insert(player.scene_id, AvatarUpdate {
            update_shape: changed.then_some(base_shape.0.clone()),
            active_scene: containing_scene.get(entity),
            prev_source: maybe_prev_selection.as_ref().map(|prev| prev.scene).unwrap_or_default(),
            current_source: None,
        });
    }

    for (ent, scene_ent, scene_avatar_shape, changed) in scene_avatar_defs.iter() {
        let Some(mut update) = updates.get_mut(&scene_ent.id) else { continue };

        if Some(scene_ent.root) != update.active_scene {
            continue;
        }

        // this is the source
        update.current_source = Some(ent);

        if changed || update.prev_source != update.current_source {
            // and it needs to be updated
            update.update_shape = Some(scene_avatar_shape.0.clone());
        } else {
            // doesn't need to be updated, even if the base shape changed
            update.update_shape = None;
        }
    }

    for (entity, player, base_shape, _, maybe_prev_selection) in root_avatar_defs.iter_mut() {
        let update = updates.remove(&player.scene_id).unwrap();
        let needs_update = update.current_source != update.prev_source || update.update_shape.is_some();

        if needs_update {
            debug!("updating selected avatar for {} -> {:?}", player.scene_id, update.current_source);

            let shape = update.update_shape.unwrap_or(base_shape.0.clone());
            if let Some(mut selection) = maybe_prev_selection {
                selection.shape = shape;
                selection.scene = update.current_source;
            } else {
                if let Some(mut commands) = commands.get_entity(entity) {
                    commands.insert(AvatarSelection {
                        scene: update.current_source,
                        shape,
                    });
                }
            }
        }
    }
}

fn update_render_avatar(
    mut commands: Commands,
    query: Query<(Entity, &AvatarSelection, Option<&Children>), Changed<AvatarSelection>>,
    avatar_render_entities: Query<(), With<AvatarRenderEntity>>,
    wearable_pointers: Res<WearablePointers>,
    wearable_metas: Res<WearableMetas>,
    asset_server: Res<AssetServer>,
) {
    for (entity, selection, maybe_children) in &query {
        debug!("updating render avatar");
        // remove existing children
        if let Some(children) = maybe_children {
            for render_child in children.iter().filter(|child| avatar_render_entities.get(**child).is_ok()) {
                commands.entity(*render_child).despawn_recursive();
            }
        }

        let body = selection.shape.body_shape.as_ref().unwrap().to_lowercase();
        println!("body: {}", body);
        let body = Urn::from_str(&body).unwrap();
        let hash = wearable_pointers.0.get(&body).unwrap();
        let meta = wearable_metas.0.get(hash).unwrap();
        let body_shape = &meta.data.representations[0].body_shapes[0].to_lowercase();

        let ext = meta.data.representations[0].main_file.rsplit_once('.').unwrap().1;
        if ext != "glb" {
            panic!("{ext}");
        }

        let body_ent = commands.spawn((
            AvatarRenderEntity,
            SpatialBundle::default(),
            asset_server.load_content_file::<Gltf>(&meta.data.representations[0].main_file, hash).unwrap(),
        )).id();

        commands.entity(entity).add_child(body_ent);

        for wearable in &selection.shape.wearables {
            let wearable = Urn::from_str(&wearable).unwrap();
            let hash = wearable_pointers.0.get(&wearable).unwrap();
            let meta = wearable_metas.0.get(hash).unwrap();

            let maybe_representation = meta.data.representations.iter().find(|rep| rep.body_shapes.iter().find(|rep_shape| rep_shape.to_lowercase() == *body_shape).is_some());

            let Some(representation) = maybe_representation else {
                warn!("no representation for {wearable} matching {body_shape}");
                continue;
            };
    
            let ext = representation.main_file.rsplit_once('.').unwrap().1;
            if ext != "glb" {
                panic!("{wearable} has ext {ext}");
            }
    
            let wearable_ent = commands.spawn((
                AvatarRenderEntity,
                SpatialBundle::default(),
                asset_server.load_content_file::<Gltf>(&representation.main_file, hash).unwrap(),
            )).id();
    
            commands.entity(entity).add_child(wearable_ent);
        }
    }
}

#[derive(Component)]
pub struct AvatarProcessed;

fn attach_gltfs(
    mut commands: Commands,
    query: Query<(Entity, &Handle<Gltf>), (With<AvatarRenderEntity>, Without<AvatarProcessed>)>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut scene_spawner: ResMut<SceneSpawner>,
) {
    for (ent, h_gltf) in query.iter() {
        match asset_server.get_load_state(h_gltf) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                warn!("failed to process gltf");
                commands.entity(ent).insert(AvatarProcessed);
                continue;
            }
            _ => continue,
        }

        let gltf = gltfs.get(h_gltf).unwrap();
        let gltf_scene_handle = gltf.default_scene.as_ref();

        match gltf_scene_handle {
            Some(gltf_scene_handle) => {
                let _ = scene_spawner.spawn_as_child(gltf_scene_handle.clone_weak(), ent);
                commands.entity(ent).insert(AvatarProcessed);
            }
            None => {
                warn!("no default scene found in gltf.");
                commands.entity(ent).insert(AvatarProcessed);
            }
        }

    }
}

#[derive(Component)]
struct PendingAvatarTask(HashSet<Urn>);

#[derive(Serialize, Deserialize, Copy, Clone)]
struct AvatarColor {
    pub color: Color3,
}

#[derive(Serialize, Deserialize)]
pub struct AvatarSnapshots {
    pub face256: String,
    pub body: String,
}

#[derive(Serialize, Deserialize)]
pub struct AvatarWireFormat {
    name: Option<String>,
    #[serde(rename = "bodyShape")]
    body_shape: Option<String>,
    eyes: Option<AvatarColor>,
    hair: Option<AvatarColor>,
    skin: Option<AvatarColor>,
    wearables: Vec<String>,
    emotes: Option<Vec<String>>,
    snapshots: Option<AvatarSnapshots>,
}
