use bevy::{prelude::*, render::view::VisibilitySystems, transform::TransformSystem};
use common::structs::{CurrentRealm, PrimaryCamera};
use ipfs::{ipfs_path::IpfsPath, EntityDefinition};

use crate::shell_texturing::{Ground, ParcelGrass};

pub(super) struct GroundVisibilityPlugin;

impl Plugin for GroundVisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GroundVisibility(true)).add_systems(
            PostUpdate,
            (update, apply)
                .chain()
                .after(TransformSystem::TransformPropagate)
                .before(VisibilitySystems::VisibilityPropagate),
        );
    }
}

#[derive(Resource, PartialEq)]
pub(super) struct GroundVisibility(pub bool);

pub(super) fn terrain_visible(
    realm: &CurrentRealm,
    definitions: &Assets<EntityDefinition>,
) -> bool {
    let Some(urns) = realm.config.scenes_urn.as_deref() else {
        return true;
    };
    let [urn] = urns else {
        return true;
    };
    let Some(hash) = IpfsPath::new_from_urn::<EntityDefinition>(&urn.replace('?', "?=&"))
        .ok()
        .and_then(|path| path.context_free_hash().ok().flatten())
    else {
        return true;
    };
    definitions
        .iter()
        .find(|(_, definition)| definition.id == hash)
        .and_then(|(_, definition)| definition.metadata.as_ref())
        .and_then(|metadata| metadata.get("landscapeTerrain"))
        .and_then(|value| value.as_bool())
        != Some(false)
}

fn update(
    realm: Option<Res<CurrentRealm>>,
    definitions: Option<Res<Assets<EntityDefinition>>>,
    mut visibility: ResMut<GroundVisibility>,
) {
    let next = match (realm, definitions) {
        (Some(realm), Some(definitions)) => {
            if !realm.is_changed() && !definitions.is_changed() {
                return;
            }
            terrain_visible(&realm, &definitions)
        }
        _ => true,
    };
    visibility.set_if_neq(GroundVisibility(next));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply(
    visibility: Res<GroundVisibility>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut ground: Query<
        (&mut Visibility, Has<Ground>),
        Or<(
            With<Ground>,
            With<ParcelGrass>,
            With<crate::landscape_trees::LandscapeTree>,
        )>,
    >,
) {
    let below_ground = camera
        .single()
        .is_ok_and(|camera| camera.translation().y < 0.);
    for (mut visible, base_ground) in &mut ground {
        let next = if visibility.0 && !(base_ground && below_ground) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visible.set_if_neq(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_single_fixed_scene_can_disable_terrain() {
        let mut definitions = Assets::<EntityDefinition>::default();
        let _definition = definitions.add(EntityDefinition {
            id: "scene".into(),
            metadata: Some(r#"{"landscapeTerrain":false}"#.parse().unwrap()),
            ..default()
        });
        let mut realm = CurrentRealm::default();
        assert!(terrain_visible(&realm, &definitions));
        realm.config.scenes_urn = Some(vec!["urn:decentraland:entity:scene".into()]);
        assert!(!terrain_visible(&realm, &definitions));
        realm.config.scenes_urn = Some(vec![
            "urn:decentraland:entity:scene".into(),
            "urn:decentraland:entity:other".into(),
        ]);
        assert!(terrain_visible(&realm, &definitions));
        realm.config.scenes_urn = Some(vec!["urn:decentraland:entity:other".into()]);
        assert!(terrain_visible(&realm, &definitions));
    }

    #[test]
    fn late_metadata_and_realm_changes_update_existing_and_new_ground() {
        let mut app = App::new();
        let mut realm = CurrentRealm::default();
        realm.config.scenes_urn = Some(vec!["urn:decentraland:entity:scene".into()]);
        app.insert_resource(realm)
            .init_resource::<Assets<EntityDefinition>>()
            .add_plugins(GroundVisibilityPlugin);
        let ground = app.world_mut().spawn((Ground, Visibility::Inherited)).id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(ground),
            Some(&Visibility::Inherited)
        );
        let definition = app
            .world_mut()
            .resource_mut::<Assets<EntityDefinition>>()
            .add(EntityDefinition {
                id: "scene".into(),
                metadata: Some(r#"{"landscapeTerrain":false}"#.parse().unwrap()),
                ..default()
            });
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(ground),
            Some(&Visibility::Hidden)
        );
        let later_ground = app.world_mut().spawn((Ground, Visibility::Inherited)).id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(later_ground),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .resource_mut::<Assets<EntityDefinition>>()
            .get_mut(&definition)
            .unwrap()
            .metadata = Some(r#"{"landscapeTerrain":true}"#.parse().unwrap());
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(ground),
            Some(&Visibility::Inherited)
        );
        app.world_mut()
            .resource_mut::<CurrentRealm>()
            .config
            .scenes_urn = None;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(later_ground),
            Some(&Visibility::Inherited)
        );
    }
}
