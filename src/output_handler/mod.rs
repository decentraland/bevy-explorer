use bevy::prelude::*;
use serde::{Deserialize, Deserializer};

use crate::scene_runner::{AddEngineCommandHandlerExt, JsEntityMap, SceneSystems};

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        // register "entity_add" method with EntityAddEngineCommand payload
        app.add_command_event::<EntityAddEngineCommand>("entity_add");
        // add system to handle EntityAddEngineCommand events
        app.add_system(entity_add.label(SceneSystems::HandleOutput));

        app.add_command_event::<EntityTransformUpdateCommand>("entity_transform_update");
        app.add_system(
            entity_transform_update
                .label(SceneSystems::HandleOutput)
                .after(entity_add),
        );
    }
}

#[derive(Deserialize)]
struct EntityAddEngineCommand {
    id: usize,
}

// handle "entity_add" commands
fn entity_add(
    mut commands: Commands,
    mut entity_map: ResMut<JsEntityMap>,
    mut events: EventReader<EntityAddEngineCommand>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for ev in events.iter() {
        if let Some(existing) = entity_map.0.remove(&ev.id) {
            // remove any existing entity with the given id
            commands.entity(existing).despawn_recursive();
        }

        // spawn a default cube
        let entity = commands
            .spawn(PbrBundle {
                mesh: meshes.add(shape::Cube::new(1.0).into()),
                material: materials.add(Color::RED.into()),
                ..Default::default()
            })
            .id();

        info!("spawned {} -> {:?}", ev.id, entity);
        // add the (js entity handle -> entity id) to our map
        entity_map.0.insert(ev.id, entity);
    }
}

#[derive(Deserialize, Clone)]
struct EntityTransformUpdateCommand {
    #[serde(rename = "entityId")]
    entity_id: usize,
    #[serde(deserialize_with = "parse_engine_transform")]
    transform: Transform,
}

#[derive(Deserialize)]
struct EngineTransform {
    position: Vec3,
    rotation: Vec4,
    scale: Vec3,
}

// custom deserializer as the bevy Transform format is different to the message format
fn parse_engine_transform<'de, D: Deserializer<'de>>(source: D) -> Result<Transform, D::Error> {
    let source = EngineTransform::deserialize(source)?;

    Ok(Transform {
        translation: source.position,
        // TODO: not sure how the rotation is meant to be interpreted, i chose euler angles and discarded the 4th component
        rotation: Quat::from_euler(
            EulerRot::XYZ,
            source.rotation.x,
            source.rotation.y,
            source.rotation.z,
        ),
        scale: source.scale,
    })
}

fn entity_transform_update(
    mut pending: Local<Vec<EntityTransformUpdateCommand>>,
    entity_map: ResMut<JsEntityMap>,
    mut events: EventReader<EntityTransformUpdateCommand>,
    mut transforms: Query<&mut Transform>,
) {
    for event in std::mem::take(&mut *pending).iter().chain(events.iter()) {
        let Some(entity) = entity_map.0.get(&event.entity_id) else {
            warn!("entity_transform_update for unknown entity {}", event.entity_id);
            continue;
        };

        if let Ok(mut transform) = transforms.get_mut(*entity) {
            *transform = event.transform;
        } else {
            // the entity exists in the JsEntityMap but not in the world -
            // this is probably because it has been spawned in the same schedule stage,
            // so we'll store the command for next time.
            // TODO: we could also add a flush between add_entity instructions and others
            // which will be much easier in bevy v0.10 releasing ~18th feb.
            debug!(
                "deferring entity_transform_update for pending entity {}",
                event.entity_id
            );
            pending.push(event.clone());
        }
    }
}
