use std::{f32::consts::PI, str::FromStr};

use attach::AttachPlugin;
use avatar_texture::AvatarTexturePlugin;
use bevy::{
    gltf::Gltf,
    prelude::*,
    render::{
        mesh::skinning::SkinnedMesh,
        view::{NoFrustumCulling, RenderLayers},
    },
    scene::InstanceId,
    utils::{HashMap, HashSet},
};
use bevy_mod_billboard::BillboardTextBundle;
use colliders::AvatarColliderPlugin;
use serde::Deserialize;
use urn::Urn;

pub mod animate;
pub mod attach;
pub mod avatar_texture;
pub mod base_wearables;
pub mod colliders;
pub mod foreign_dynamics;
pub mod mask_material;

use common::{
    structs::{AttachPoints, PrimaryUser},
    util::{TaskExt, TryPushChildrenEx},
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
use ipfs::{ActiveEntityTask, IpfsAssetServer, IpfsModifier};
use scene_runner::{update_world::AddCrdtInterfaceExt, ContainingScene, SceneEntity};
use ui_core::TEXT_SHAPE_FONT;

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
        app.add_plugins(AvatarAnimationPlugin);
        app.add_plugins(AttachPlugin);
        app.add_plugins(AvatarColliderPlugin);
        app.add_plugins(AvatarTexturePlugin);
        app.init_resource::<WearablePointers>();
        app.init_resource::<WearableMetas>();
        app.add_systems(Update, load_base_wearables);
        app.add_systems(Update, update_avatar_info);
        app.add_systems(Update, update_base_avatar_shape);
        app.add_systems(Update, select_avatar);
        app.add_systems(Update, update_render_avatar);
        app.add_systems(Update, spawn_scenes);
        app.add_systems(PostUpdate, process_avatar);

        app.add_crdt_lww_component::<PbAvatarShape, AvatarShape>(
            SceneComponentId::AVATAR_SHAPE,
            ComponentPosition::Any,
        );
    }
}

#[derive(Component, Default)]
pub struct AvatarDynamicState {
    pub velocity: Vec3,
    pub ground_height: f32,
}

#[derive(Debug)]
pub enum WearablePointerResult {
    Exists(String),
    Missing,
}

impl WearablePointerResult {
    pub fn hash(&self) -> Option<&str> {
        match self {
            WearablePointerResult::Exists(h) => Some(h),
            WearablePointerResult::Missing => None,
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct WearablePointers(pub HashMap<Urn, WearablePointerResult>);

#[derive(Resource, Default, Debug)]
pub struct WearableMetas(pub HashMap<String, WearableMeta>);

#[derive(Deserialize, Debug, Component, Clone)]
pub struct WearableMeta {
    pub id: String,
    pub description: String,
    pub thumbnail: String,
    pub rarity: Option<String>,
    pub data: WearableData,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WearableData {
    pub tags: Vec<String>,
    pub category: WearableCategory,
    pub representations: Vec<WearableRepresentation>,
}

#[derive(Deserialize, Debug, Clone)]
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
    ipfas: IpfsAssetServer,
) {
    if *once || ipfas.active_endpoint().is_none() {
        return;
    }

    match *task {
        None => {
            let pointers = base_wearables::base_wearables();
            *task = Some(
                ipfas
                    .ipfs()
                    .active_entities(ipfs::ActiveEntitiesRequest::Pointers(pointers), Some(base_wearables::BASE_URL)),
            );
        }
        Some(ref mut active_task) => match active_task.complete() {
            None => (),
            Some(Err(e)) => warn!("failed to acquire base wearables: {e}"),
            Some(Ok(active_entities)) => {
                debug!("first active entity: {:?}", active_entities.get(0));
                for entity in active_entities {
                    ipfas.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                        entity.metadata.as_ref().map(ToString::to_string),
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
            SceneComponentId::AVATAR_BASE,
            CrdtType::LWW_ANY,
            player.scene_id,
            &PbAvatarBase {
                name: avatar.name.as_deref().unwrap_or("???").to_owned(),
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
            Changed<AvatarShape>,
            Option<&mut AvatarSelection>,
        ),
        Or<(With<ForeignPlayer>, With<PrimaryUser>)>,
    >,
    scene_avatar_defs: Query<(Entity, &SceneEntity, &AvatarShape, Changed<AvatarShape>)>,
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
    for (entity, maybe_player, base_shape, changed, maybe_prev_selection) in root_avatar_defs.iter()
    {
        let id = maybe_player
            .map(|p| p.scene_id)
            .unwrap_or(SceneEntityId::PLAYER);
        updates.insert(
            id,
            AvatarUpdate {
                base_name: base_shape.0.name.clone().unwrap_or_else(|| "Guest".into()),
                update_shape: changed.then_some(base_shape.0.clone()),
                active_scenes: containing_scene.get(entity),
                prev_source: maybe_prev_selection
                    .as_ref()
                    .map(|prev| prev.scene)
                    .unwrap_or_default(),
                current_source: None,
            },
        );
    }

    for (ent, scene_ent, scene_avatar_shape, changed) in scene_avatar_defs.iter() {
        let Some(update) = updates.get_mut(&scene_ent.id) else {
            // this is an NPC avatar, attach selection immediately
            if changed {
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

        if changed || update.prev_source != update.current_source {
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

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct WearableCategory {
    pub slot: &'static str,
    pub is_texture: bool,
}

impl<'de> serde::Deserialize<'de> for WearableCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(WearableCategory::from_str(s.as_str()).unwrap_or(WearableCategory::UNKNOWN))
    }
}

impl WearableCategory {
    const UNKNOWN: WearableCategory = WearableCategory::texture("unknown");

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

impl WearableCategory {
    pub fn iter() -> impl Iterator<Item = &'static WearableCategory> {
        [
            Self::EYES,
            Self::EYEBROWS,
            Self::MOUTH,
            Self::FACIAL_HAIR,
            Self::HAIR,
            Self::HEAD,
            Self::UPPER_BODY,
            Self::LOWER_BODY,
            Self::FEET,
            Self::EARRING,
            Self::EYEWEAR,
            Self::HAT,
            Self::HELMET,
            Self::MASK,
            Self::TIARA,
            Self::TOP_HEAD,
            Self::SKIN,
        ]
        .iter()
    }
}

#[derive(Debug, Clone)]
pub struct WearableDefinition {
    pub category: WearableCategory,
    pub hides: HashSet<WearableCategory>,
    pub model: Option<Handle<Gltf>>,
    pub texture: Option<Handle<Image>>,
    pub mask: Option<Handle<Image>>,
    pub thumbnail: Option<Handle<Image>>,
}

impl WearableDefinition {
    pub fn new(
        meta: &WearableMeta,
        ipfas: &IpfsAssetServer,
        body_shape: &str,
        content_hash: &str,
    ) -> Option<WearableDefinition> {
        let Some(representation) = (if body_shape.is_empty() {
            Some(&meta.data.representations[0])
        } else {
            meta.data.representations.iter().find(|rep| {
                rep.body_shapes
                    .iter()
                    .any(|rep_shape| rep_shape.to_lowercase() == *body_shape)
            })
        }) else {
            warn!("no representation for body shape {body_shape}");
            return None;
        };

        let category = meta.data.category;
        if category == WearableCategory::UNKNOWN {
            warn!("unknown wearable category");
            return None;
        }

        let hides = HashSet::from_iter(
            representation
                .override_hides
                .iter()
                .chain(representation.override_replaces.iter())
                .flat_map(|c| WearableCategory::from_str(c)),
        );

        let (model, texture, mask) = if category.is_texture {
            // don't validate the main file, as some base wearables have no extension on the main_file member (Eyebrows_09 e.g)
            // if !representation.main_file.ends_with(".png") {
            //     warn!(
            //         "expected .png main file for category {}, found {}",
            //         category.slot, representation.main_file
            //     );
            //     return None;
            // }

            let texture = representation
                .contents
                .iter()
                .find(|f| {
                    f.to_lowercase().ends_with(".png") && !f.to_lowercase().ends_with("_mask.png")
                })
                .and_then(|f| ipfas.load_content_file::<Image>(f, content_hash).ok());
            let mask = representation
                .contents
                .iter()
                .find(|f| f.to_lowercase().ends_with("_mask.png"))
                .and_then(|f| ipfas.load_content_file::<Image>(f, content_hash).ok());

            (None, texture, mask)
        } else {
            if !representation.main_file.to_lowercase().ends_with(".glb") {
                warn!(
                    "expected .glb main file, found {}",
                    representation.main_file
                );
                return None;
            }

            let model = ipfas
                .load_content_file::<Gltf>(&representation.main_file, content_hash)
                .ok();

            (model, None, None)
        };

        let thumbnail = ipfas
            .load_content_file::<Image>(&meta.thumbnail, content_hash)
            .ok();

        Some(Self {
            category,
            hides,
            model,
            texture,
            mask,
            thumbnail,
        })
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
    mut wearable_pointers: ResMut<WearablePointers>,
    mut wearable_metas: ResMut<WearableMetas>,
    ipfas: IpfsAssetServer,
    mut wearable_task: Local<Option<(ActiveEntityTask, HashSet<Urn>)>>,
) {
    let mut missing_wearables = HashSet::default();

    // update resources with active entity results
    if let Some((mut task, mut wearables)) = wearable_task.take() {
        match task.complete() {
            Some(Ok(entities)) => {
                debug!("got results: {:?}", entities.len());

                for entity in entities {
                    ipfas.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                        entity.metadata.as_ref().map(ToString::to_string),
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
                                debug!("{} -> {}", pointer, entity.id);
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
            .unwrap_or(&base_wearables::default_bodyshape())
            .to_lowercase();
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
                commands.entity(entity).try_insert(RetryRenderAvatar);
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
                        commands.entity(entity).try_insert(RetryRenderAvatar);
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
        let body_wearable = match WearableDefinition::new(body_meta, &ipfas, "", hash) {
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
                WearableDefinition::new(meta, &ipfas, body_shape, hash)
            })
            .collect::<Vec<_>>();
        let mut wearables = HashMap::from_iter(
            wearables
                .into_iter()
                .map(|wearable| (wearable.category, wearable)),
        );

        // add defaults
        let defaults: Vec<_> = base_wearables::default_wearables()
            .flat_map(|default| {
                let Some(WearablePointerResult::Exists(hash)) =
                    wearable_pointers.0.get(&Urn::from_str(default).unwrap())
                else {
                    warn!("failed to load default renderable {}", default);
                    return None;
                };
                let meta = wearable_metas.0.get(hash).unwrap();
                WearableDefinition::new(meta, &ipfas, body_shape, hash)
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
                    label: selection.shape.name.clone(),
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

    if wearable_task.is_none() && !missing_wearables.is_empty() {
        let base_wearables = HashSet::from_iter(base_wearables::base_wearables());
        let pointers = missing_wearables
            .iter()
            .map(ToString::to_string)
            .filter(|urn| !base_wearables.contains(urn))
            .collect::<Vec<_>>();

        if !pointers.is_empty() {
            debug!("requesting: {:?}", missing_wearables);
            *wearable_task = Some((
                ipfas.ipfs().active_entities(ipfs::ActiveEntitiesRequest::Pointers(pointers), None),
                missing_wearables,
            ));
        }
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
        commands.entity(ent).try_insert(AvatarLoaded {
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mask_materials: ResMut<Assets<MaskMaterial>>,
    meshes: Res<Assets<Mesh>>,
    attach_points: Query<&AttachPoints>,
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

        let mut colored_materials = HashMap::default();
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
                commands
                    .entity(scene_ent)
                    .try_insert(AnimationPlayer::default());
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
                if loaded_avatar.skin_materials.contains(h_mat) {
                    if let Some(mat) = materials.get(h_mat) {
                        let new_mat = StandardMaterial {
                            base_color: def.skin_color,
                            ..mat.clone()
                        };
                        let h_colored_mat = colored_materials
                            .entry(h_mat.clone_weak())
                            .or_insert_with(|| materials.add(new_mat));
                        commands.entity(scene_ent).try_insert(h_colored_mat.clone());
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
                        commands.entity(scene_ent).try_insert(h_colored_mat.clone());
                    }
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
                                .remove::<Handle<StandardMaterial>>();
                        } else {
                            debug!("no mask for {suffix}");
                            let material = materials.add(StandardMaterial {
                                base_color: if no_mask_means_ignore_color {
                                    Color::WHITE
                                } else {
                                    color
                                },
                                base_color_texture: texture.clone(),
                                alpha_mode: AlphaMode::Blend,
                                ..Default::default()
                            });
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
                ("head", WearableCategory::HEAD),
                ("head_basemesh", WearableCategory::HEAD),
                ("mask_eyes", WearableCategory::HEAD),
                ("mask_eyebrows", WearableCategory::HEAD),
                ("mask_mouth", WearableCategory::HEAD),
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
                        .push_children(&[attach_points.left_hand]);
                } else {
                    warn!("no left hand");
                    warn!("available: {:#?}", target_armature_entities.keys());
                }
                if let Some(right_hand) =
                    target_armature_entities.get(&String::from("avatar_righthand"))
                {
                    commands
                        .entity(*right_hand)
                        .push_children(&[attach_points.right_hand]);
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
                    if let Some(mesh_data) = meshes.get(h_mesh) {
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
                    if loaded_avatar.skin_materials.contains(h_mat) {
                        if let Some(mat) = materials.get(h_mat) {
                            let new_mat = StandardMaterial {
                                base_color: def.skin_color,
                                ..mat.clone()
                            };
                            let h_colored_mat = colored_materials
                                .entry(h_mat.clone_weak())
                                .or_insert_with(|| materials.add(new_mat));
                            commands.entity(scene_ent).try_insert(h_colored_mat.clone());
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
                            commands.entity(scene_ent).try_insert(h_colored_mat.clone());
                        }
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
            wearable_models, wearable_texs, def.hides, loaded_avatar.skin_materials.len(), loaded_avatar.hair_materials.len(), colored_materials.len()
        );
        commands
            .entity(avatar_ent)
            .try_insert(AvatarProcessed)
            .with_children(|commands| {
                // add a name tag
                if let Some(label) = def.label.as_ref() {
                    commands.spawn(BillboardTextBundle {
                        text: Text::from_section(
                            label,
                            TextStyle {
                                font_size: 50.0,
                                color: Color::WHITE,
                                font: TEXT_SHAPE_FONT.get().unwrap().clone(),
                            },
                        )
                        .with_alignment(TextAlignment::Center),
                        transform: Transform::from_translation(Vec3::Y * 2.2)
                            .with_scale(Vec3::splat(0.003)),
                        ..Default::default()
                    });
                }
            });
    }
}

#[derive(Component)]
struct PendingAvatarTask(HashSet<Urn>);
