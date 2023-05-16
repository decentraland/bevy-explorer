use std::{f32::consts::PI, str::FromStr};

use bevy::{
    ecs::system::SystemParam,
    gltf::Gltf,
    math::Vec3Swizzles,
    prelude::*,
    render::view::NoFrustumCulling,
    scene::InstanceId,
    utils::{HashMap, HashSet},
};
use serde::{Deserialize, Serialize};
use urn::Urn;

pub mod base_wearables;
pub mod mask_material;

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

use self::mask_material::{MaskMaterial, MaskMaterialPlugin};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(MaskMaterialPlugin);
        app.init_resource::<WearablePointers>();
        app.init_resource::<WearableMetas>();
        app.add_system(load_base_wearables);
        app.add_system(update_avatar_info);
        app.add_system(update_base_avatar_shape);
        app.add_system(select_avatar);
        app.add_system(update_render_avatar);
        app.add_system(spawn_scenes);
        app.add_system(process_avatar.in_base_set(CoreSet::PostUpdate));

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

#[derive(Debug)]
pub enum WearablePointerResult {
    Exists(String),
    Missing,
}

#[derive(Resource, Default, Debug)]
pub struct WearablePointers(HashMap<Urn, WearablePointerResult>);

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
            *task = Some(
                asset_server
                    .ipfs()
                    .active_entities(&pointers, Some(base_wearables::BASE_URL)),
            );
        }
        Some(ref mut active_task) => match active_task.complete() {
            None => (),
            Some(Err(e)) => warn!("failed to acquire base wearables: {e}"),
            Some(Ok(active_entities)) => {
                debug!("first active entity: {:?}", active_entities.get(0));
                for entity in active_entities {
                    asset_server.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                    );

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
                                wearable_pointers
                                    .0
                                    .insert(urn, WearablePointerResult::Exists(entity.id.clone()));
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

// send received avatar info into scenes
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
                emotes: avatar
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

#[derive(Component)]
pub struct AvatarAttachment(pub PbAvatarAttach);

impl From<PbAvatarAttach> for AvatarAttachment {
    fn from(value: PbAvatarAttach) -> Self {
        Self(value)
    }
}

// set (foreign) user's default avatar shape based on profile data
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
                    .map(|emotes| {
                        emotes
                            .iter()
                            .map(|emote| emote.urn.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            }));
        }
    }
}

// helper to get the scene entity containing a given world position
#[derive(SystemParam)]
pub struct ContainingScene<'w, 's> {
    transforms: Query<'w, 's, &'static GlobalTransform, With<SceneEntity>>,
    pointers: Res<'w, ScenePointers>,
    live_scenes: Res<'w, LiveScenes>,
}

impl<'w, 's> ContainingScene<'w, 's> {
    fn get(&self, ent: Entity) -> Option<Entity> {
        let parcel = (self.transforms.get(ent).ok()?.translation().xz() / PARCEL_SIZE).as_ivec2();

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

// choose the avatar shape based on current scene of the player
#[allow(clippy::type_complexity)]
fn select_avatar(
    mut commands: Commands,
    mut root_avatar_defs: Query<(
        Entity,
        &ForeignPlayer,
        &AvatarShape,
        Changed<AvatarShape>,
        Option<&mut AvatarSelection>,
    )>,
    scene_avatar_defs: Query<(Entity, &SceneEntity, &AvatarShape, Changed<AvatarShape>)>,
    orphaned_avatar_selections: Query<Entity, (With<AvatarSelection>, Without<AvatarShape>)>,
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
        updates.insert(
            player.scene_id,
            AvatarUpdate {
                update_shape: changed.then_some(base_shape.0.clone()),
                active_scene: containing_scene.get(entity),
                prev_source: maybe_prev_selection
                    .as_ref()
                    .map(|prev| prev.scene)
                    .unwrap_or_default(),
                current_source: None,
            },
        );
    }

    for (ent, scene_ent, scene_avatar_shape, changed) in scene_avatar_defs.iter() {
        let Some(mut update) = updates.get_mut(&scene_ent.id) else {
            // this is an NPC avatar, attach selection immediately
            if changed {
                commands.entity(ent).insert(AvatarSelection {
                    scene: Some(scene_ent.root),
                    shape: scene_avatar_shape.0.clone(),
                });

                error!("npc avatar {:?}", scene_ent);
            }

            continue;
        };

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

    // update avatar selection on foreign players
    for (entity, player, base_shape, _, maybe_prev_selection) in root_avatar_defs.iter_mut() {
        let update = updates.remove(&player.scene_id).unwrap();
        let needs_update =
            update.current_source != update.prev_source || update.update_shape.is_some();

        if needs_update {
            debug!(
                "updating selected avatar for {} -> {:?}",
                player.scene_id, update.current_source
            );

            let shape = update.update_shape.unwrap_or(base_shape.0.clone());
            if let Some(mut selection) = maybe_prev_selection {
                selection.shape = shape;
                selection.scene = update.current_source;
            } else if let Some(mut commands) = commands.get_entity(entity) {
                commands.insert(AvatarSelection {
                    scene: update.current_source,
                    shape,
                });
            }
        }
    }

    // remove any orphans
    for entity in &orphaned_avatar_selections {
        if let Some(mut commands) = commands.get_entity(entity) {
            commands.remove::<AvatarSelection>();
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct WearableCategory {
    pub slot: &'static str,
    pub is_texture: bool,
}

impl WearableCategory {
    const EYES: WearableCategory = WearableCategory::texture("eyes");
    const EYEBROWS: WearableCategory = WearableCategory::texture("eyebrows");
    const MOUTH: WearableCategory = WearableCategory::texture("mouth");

    const FACIAL_HAIR: WearableCategory = WearableCategory::model("facial_hair");
    const HAIR: WearableCategory = WearableCategory::model("hair");
    const HEAD: WearableCategory = WearableCategory::model("head");
    const BODY_SHAPE: WearableCategory = WearableCategory::model("body_shape");
    const UPPER_BODY: WearableCategory = WearableCategory::model("upper_body");
    const LOWER_BODY: WearableCategory = WearableCategory::model("lower_body");
    const FEET: WearableCategory = WearableCategory::model("feet");
    const EARRING: WearableCategory = WearableCategory::model("earring");
    const EYEWEAR: WearableCategory = WearableCategory::model("eyewear");
    const HAT: WearableCategory = WearableCategory::model("hat");
    const HELMET: WearableCategory = WearableCategory::model("helmet");
    const MASK: WearableCategory = WearableCategory::model("mask");
    const TIARA: WearableCategory = WearableCategory::model("tiara");
    const TOP_HEAD: WearableCategory = WearableCategory::model("top_head");
    const SKIN: WearableCategory = WearableCategory::model("skin");

    const fn model(slot: &'static str) -> Self {
        Self {
            slot,
            is_texture: false,
        }
    }

    const fn texture(slot: &'static str) -> Self {
        Self {
            slot,
            is_texture: true,
        }
    }
}

impl FromStr for WearableCategory {
    type Err = anyhow::Error;

    fn from_str(slot: &str) -> Result<WearableCategory, Self::Err> {
        match slot {
            "eyebrows" => Ok(Self::EYEBROWS),
            "eyes" => Ok(Self::EYES),
            "facial_hair" => Ok(Self::FACIAL_HAIR),
            "hair" => Ok(Self::HAIR),
            "head" => Ok(Self::HEAD),
            "body_shape" => Ok(Self::BODY_SHAPE),
            "mouth" => Ok(Self::MOUTH),
            "upper_body" => Ok(Self::UPPER_BODY),
            "lower_body" => Ok(Self::LOWER_BODY),
            "feet" => Ok(Self::FEET),
            "earring" => Ok(Self::EARRING),
            "eyewear" => Ok(Self::EYEWEAR),
            "hat" => Ok(Self::HAT),
            "helmet" => Ok(Self::HELMET),
            "mask" => Ok(Self::MASK),
            "tiara" => Ok(Self::TIARA),
            "top_head" => Ok(Self::TOP_HEAD),
            "skin" => Ok(Self::SKIN),
            _ => {
                warn!("unrecognised wearable category: {slot}");
                Err(anyhow::anyhow!("unrecognised wearable category: {slot}"))
            }
        }
    }
}

#[derive(Debug)]
pub struct WearableDefinition {
    category: WearableCategory,
    hides: HashSet<WearableCategory>,
    model: Option<Handle<Gltf>>,
    texture: Option<Handle<Image>>,
    mask: Option<Handle<Image>>,
}

impl WearableDefinition {
    pub fn new(
        meta: &WearableMeta,
        asset_server: &AssetServer,
        body_shape: &str,
        content_hash: &str,
    ) -> Option<WearableDefinition> {
        let Some(representation) = (
            if body_shape.is_empty() {
                Some(&meta.data.representations[0])
            } else {
                meta.data.representations.iter().find(|rep| rep.body_shapes.iter().any(|rep_shape| rep_shape.to_lowercase() == *body_shape))
            }
        ) else {
            warn!("no representation for body shape");
            return None;
        };

        let Ok(category) = WearableCategory::from_str(&meta.data.category) else { return None };

        let hides = HashSet::from_iter(
            representation
                .override_hides
                .iter()
                .chain(representation.override_replaces.iter())
                .flat_map(|c| WearableCategory::from_str(c)),
        );

        let (model, texture, mask) = if category.is_texture {
            if !representation.main_file.ends_with(".png") {
                warn!(
                    "expected .png main file for category {}, found {}",
                    category.slot, representation.main_file
                );
                return None;
            }

            let texture = representation
                .contents
                .iter()
                .find(|f| {
                    f.to_lowercase().ends_with(".png") && !f.to_lowercase().ends_with("_mask.png")
                })
                .and_then(|f| {
                    asset_server
                        .load_content_file::<Image>(f, content_hash)
                        .ok()
                });
            let mask = representation
                .contents
                .iter()
                .find(|f| f.to_lowercase().ends_with("_mask.png"))
                .and_then(|f| {
                    asset_server
                        .load_content_file::<Image>(f, content_hash)
                        .ok()
                });

            (None, texture, mask)
        } else {
            if !representation.main_file.to_lowercase().ends_with(".glb") {
                warn!(
                    "expected .glb main file, found {}",
                    representation.main_file
                );
                return None;
            }

            let model = asset_server
                .load_content_file::<Gltf>(&representation.main_file, content_hash)
                .ok();

            (model, None, None)
        };

        Some(Self {
            category,
            hides,
            model,
            texture,
            mask,
        })
    }
}

#[derive(Component)]
pub struct AvatarDefinition {
    body: WearableDefinition,
    skin_color: Color,
    hair_color: Color,
    eyes_color: Color,
    wearables: Vec<WearableDefinition>,
    hides: HashSet<WearableCategory>,
}

#[derive(Component)]
pub struct RetryRenderAvatar;

// load wearables and create renderable avatar entity once all loaded
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_render_avatar(
    mut commands: Commands,
    query: Query<
        (Entity, &AvatarSelection, Option<&Children>),
        Or<(Changed<AvatarSelection>, With<RetryRenderAvatar>)>,
    >,
    mut removed_selections: RemovedComponents<AvatarSelection>,
    children: Query<&Children>,
    avatar_render_entities: Query<(), With<AvatarDefinition>>,
    mut wearable_pointers: ResMut<WearablePointers>,
    mut wearable_metas: ResMut<WearableMetas>,
    asset_server: Res<AssetServer>,
    mut wearable_task: Local<Option<(ActiveEntityTask, HashSet<Urn>)>>,
) {
    let mut missing_wearables = HashSet::default();

    // update resources with active entity results
    if let Some((mut task, mut wearables)) = wearable_task.take() {
        match task.complete() {
            Some(Ok(entities)) => {
                debug!("got results: {:?}", entities.len());

                for entity in entities {
                    asset_server.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                    );

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
                                wearables.remove(&urn);
                                wearable_pointers
                                    .0
                                    .insert(urn, WearablePointerResult::Exists(entity.id.clone()));
                            }
                            Err(e) => {
                                warn!("failed to parse wearable urn: {e}");
                            }
                        };
                    }

                    wearable_metas.0.insert(entity.id, wearable_data);
                }

                // any urns left in the hashset were requested but not returned
                for urn in wearables {
                    debug!("missing {urn}");
                    wearable_pointers
                        .0
                        .insert(urn, WearablePointerResult::Missing);
                }
            }
            Some(Err(e)) => {
                warn!("failed to resolve entities: {e}");
            }
            None => {
                debug!("waiting for wearable resolve");
                *wearable_task = Some((task, wearables));
            }
        }
    }

    // remove renderable entities when avatar selection is removed
    for entity in removed_selections.iter() {
        if let Ok(children) = children.get(entity) {
            for render_child in children
                .iter()
                .filter(|child| avatar_render_entities.get(**child).is_ok())
            {
                commands.entity(*render_child).despawn_recursive();
            }
        }
    }

    for (entity, selection, maybe_children) in &query {
        commands.entity(entity).remove::<RetryRenderAvatar>();

        debug!("updating render avatar");
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
        let body = selection.shape.body_shape.as_ref().unwrap().to_lowercase();
        let body = Urn::from_str(&body).unwrap();
        let hash = match wearable_pointers.0.get(&body) {
            Some(WearablePointerResult::Exists(hash)) => hash,
            Some(WearablePointerResult::Missing) => {
                debug!("failed to resolve body {body}");
                // don't retry
                continue;
            }
            None => {
                debug!("waiting for hash from body {body}");
                missing_wearables.insert(body);
                commands.entity(entity).insert(RetryRenderAvatar);
                continue;
            }
        };
        let body_meta = wearable_metas.0.get(hash).unwrap();
        let body_shape = &body_meta.data.representations[0].body_shapes[0].to_lowercase();

        let ext = body_meta.data.representations[0]
            .main_file
            .rsplit_once('.')
            .unwrap()
            .1;
        if ext != "glb" {
            panic!("{ext}");
        }

        // get wearables
        let mut all_loaded = true;
        let wearable_hashes: Vec<_> = selection
            .shape
            .wearables
            .iter()
            .flat_map(|wearable| {
                let wearable = Urn::from_str(wearable).unwrap();
                match wearable_pointers.0.get(&wearable) {
                    Some(WearablePointerResult::Exists(hash)) => Some(hash),
                    Some(WearablePointerResult::Missing) => {
                        debug!("skipping failed wearable {wearable}");
                        None
                    }
                    None => {
                        commands.entity(entity).insert(RetryRenderAvatar);
                        debug!("waiting for hash from wearable {wearable}");
                        all_loaded = false;
                        missing_wearables.insert(wearable);
                        None
                    }
                }
            })
            .collect();

        if !all_loaded {
            continue;
        }

        // load wearable gtlf/images
        let body_wearable = match WearableDefinition::new(body_meta, &asset_server, "", hash) {
            Some(body) => body,
            None => {
                warn!("failed to load body shape, can't render");
                return;
            }
        };

        let wearables = wearable_hashes
            .into_iter()
            .flat_map(|hash| {
                let meta = wearable_metas.0.get(hash).unwrap();
                WearableDefinition::new(meta, &asset_server, body_shape, hash)
            })
            .collect::<Vec<_>>();

        let hides = HashSet::from_iter(wearables.iter().flat_map(|w| w.hides.iter()).copied());

        debug!("avatar definition loaded: {wearables:?}");
        commands.entity(entity).with_children(|commands| {
            commands.spawn((
                SpatialBundle {
                    transform: Transform::from_rotation(Quat::from_rotation_y(PI)),
                    ..Default::default()
                },
                AvatarDefinition {
                    body: body_wearable,
                    wearables,
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
                },
            ));
        });
    }

    if wearable_task.is_none() && !missing_wearables.is_empty() {
        debug!("requesting: {:?}", missing_wearables);
        let pointers = missing_wearables
            .iter()
            .map(|urn| urn.to_string())
            .collect();
        *wearable_task = Some((
            asset_server.ipfs().active_entities(&pointers, None),
            missing_wearables,
        ));
    }
}

#[derive(Component)]
pub struct AvatarLoaded {
    body_instance: InstanceId,
    wearable_instances: Vec<Option<InstanceId>>,
    skin_materials: HashSet<Handle<StandardMaterial>>,
    hair_materials: HashSet<Handle<StandardMaterial>>,
}

#[derive(Component)]
pub struct AvatarSpawned;

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
                    bevy::asset::LoadState::Loading
                )
            });

        if any_loading {
            continue;
        }

        let Some(gltf) = def.body.model.as_ref()
            .and_then(|h_gltf| gltfs.get(h_gltf))
        else {
            warn!("failed to load body gltf");
            commands.entity(ent).insert(AvatarProcessed);
            continue;
        };

        let Some(h_scene) = gltf.default_scene.as_ref() else {
            warn!("body gltf has no default scene");
            commands.entity(ent).insert(AvatarProcessed);
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
                    bevy::asset::LoadState::Loaded => (),
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
        commands.entity(ent).insert(AvatarLoaded {
            body_instance,
            wearable_instances: instances.collect(),
            skin_materials,
            hair_materials,
        });
    }
}

// update materials and hide base parts
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn process_avatar(
    mut commands: Commands,
    query: Query<(Entity, &AvatarDefinition, &AvatarLoaded), Without<AvatarProcessed>>,
    scene_spawner: Res<SceneSpawner>,
    mut instance_ents: Query<(
        &mut Visibility,
        &Parent,
        Option<&Handle<StandardMaterial>>,
        Option<&Handle<Mesh>>,
    )>,
    named_ents: Query<&Name>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mask_materials: ResMut<Assets<MaskMaterial>>,
) {
    for (avatar_ent, def, loaded_avatar) in query.iter() {
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

        let mut colored_materials = HashMap::default();

        // hide and colour the base model
        for scene_ent in scene_spawner.iter_instance_entities(loaded_avatar.body_instance) {
            let Ok((mut vis, parent, maybe_h_mat, maybe_h_mesh)) = instance_ents.get_mut(scene_ent) else { continue };
            let Ok(name) = named_ents.get(parent.get()) else { continue };
            let name = name.to_lowercase();

            debug!("name: {name}");

            if maybe_h_mesh.is_some() {
                // disable frustum culling - some strange effect causes Aabb gen to fail
                // commands.entity(scene_ent).insert(Aabb::from_min_max(
                //     Vec3::new(-1.21, 0.0, -0.7),
                //     Vec3::new(1.21, 2.42, 0.7),
                // ));
                commands.entity(scene_ent).insert(NoFrustumCulling);
            }

            if let Some(h_mat) = maybe_h_mat {
                if loaded_avatar.skin_materials.contains(h_mat) {
                    if let Some(mat) = materials.get(h_mat) {
                        let new_mat = StandardMaterial {
                            base_color: def.skin_color,
                            ..mat.clone()
                        };
                        let h_colored_mat = colored_materials
                            .entry(h_mat.clone_weak())
                            .or_insert_with(|| materials.add(new_mat));
                        commands.entity(scene_ent).insert(h_colored_mat.clone());
                    }
                }

                if loaded_avatar.hair_materials.contains(h_mat) {
                    if let Some(mat) = materials.get(h_mat) {
                        let new_mat = StandardMaterial {
                            base_color: def.hair_color,
                            ..mat.clone()
                        };
                        let h_colored_mat = colored_materials
                            .entry(h_mat.clone_weak())
                            .or_insert_with(|| materials.add(new_mat));
                        commands.entity(scene_ent).insert(h_colored_mat.clone());
                    }
                }
            }

            let masks = [
                ("mask_eyes", def.eyes_color, WearableCategory::EYES),
                ("mask_eyebrows", def.hair_color, WearableCategory::EYEBROWS),
                ("mask_mouth", def.skin_color, WearableCategory::MOUTH),
            ];

            for (suffix, color, category) in masks.into_iter() {
                if name.ends_with(suffix) {
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
                                .insert(mask_material)
                                .remove::<Handle<StandardMaterial>>();
                        } else {
                            debug!("no mask for {suffix}");
                            let material = materials.add(StandardMaterial {
                                base_color: color,
                                base_color_texture: texture.clone(),
                                alpha_mode: AlphaMode::Blend,
                                ..Default::default()
                            });
                            commands.entity(scene_ent).insert(material);
                        };
                        *vis = Visibility::Inherited;
                    }
                }
            }

            let hiders = [
                ("ubody_basemesh", WearableCategory::UPPER_BODY),
                ("lbody_basemesh", WearableCategory::LOWER_BODY),
                ("feet_basemesh", WearableCategory::FEET),
                ("head", WearableCategory::HEAD),
                ("head_basemesh", WearableCategory::HEAD),
                ("mask_eyes", WearableCategory::HEAD),
                ("mask_eyebrows", WearableCategory::HEAD),
                ("mask_mouth", WearableCategory::HEAD),
            ];

            for (hidename, category) in hiders {
                if name.ends_with(hidename) {
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

        // color the components of wearables
        for instance in &loaded_avatar.wearable_instances {
            let Some(instance) = instance else {
                warn!("failed to load instance for wearable");
                continue;
            };

            for scene_ent in scene_spawner.iter_instance_entities(*instance) {
                let Ok((_, _, maybe_h_mat, maybe_h_mesh)) = instance_ents.get(scene_ent) else { continue };

                if maybe_h_mesh.is_some() {
                    // disable frustum culling - some strange effect causes Aabb gen to fail
                    // commands.entity(scene_ent).insert(Aabb::from_min_max(
                    //     Vec3::new(-1.21, 0.0, -0.7),
                    //     Vec3::new(1.21, 2.42, 0.7),
                    // ));
                    commands.entity(scene_ent).insert(NoFrustumCulling);
                }

                if let Some(h_mat) = maybe_h_mat {
                    if loaded_avatar.skin_materials.contains(h_mat) {
                        if let Some(mat) = materials.get(h_mat) {
                            let new_mat = StandardMaterial {
                                base_color: def.skin_color,
                                ..mat.clone()
                            };
                            let h_colored_mat = colored_materials
                                .entry(h_mat.clone_weak())
                                .or_insert_with(|| materials.add(new_mat));
                            commands.entity(scene_ent).insert(h_colored_mat.clone());
                        }
                    }

                    if loaded_avatar.hair_materials.contains(h_mat) {
                        if let Some(mat) = materials.get(h_mat) {
                            let new_mat = StandardMaterial {
                                base_color: def.hair_color,
                                ..mat.clone()
                            };
                            let h_colored_mat = colored_materials
                                .entry(h_mat.clone_weak())
                                .or_insert_with(|| materials.add(new_mat));
                            commands.entity(scene_ent).insert(h_colored_mat.clone());
                        }
                    }
                }
            }
        }

        let wearable_models = def.wearables.iter().filter(|w| w.model.is_some()).count();
        let wearable_texs = def.wearables.iter().filter(|w| w.model.is_none()).count();

        debug!(
            "avatar processed, 1+{} models, {} textures. hides: {:?}, skin mats: {:?}, hair mats: {:?}, used mats: {:?}",
            wearable_models, wearable_texs, def.hides, loaded_avatar.skin_materials.len(), loaded_avatar.hair_materials.len(), colored_materials.len()
        );
        commands.entity(avatar_ent).insert(AvatarProcessed);
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
pub struct AvatarEmote {
    pub slot: u32,
    pub urn: String,
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
    emotes: Option<Vec<AvatarEmote>>,
    snapshots: Option<AvatarSnapshots>,
}
