use bevy::prelude::*;
use common::sets::SceneSets;
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbVisibilityComponent, SceneComponentId};

use super::AddCrdtInterfaceExt;

pub struct VisibilityComponentPlugin;

impl Plugin for VisibilityComponentPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbVisibilityComponent, VisibilityComponent>(
            SceneComponentId::VISIBILITY,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_visibility.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct VisibilityComponent(PbVisibilityComponent);

impl From<PbVisibilityComponent> for VisibilityComponent {
    fn from(value: PbVisibilityComponent) -> Self {
        Self(value)
    }
}

fn update_visibility(
    mut commands: Commands,
    mut vis: Query<(&VisibilityComponent, &mut Visibility), Changed<VisibilityComponent>>,
    mut removed: RemovedComponents<VisibilityComponent>,
) {
    for (component, mut vis) in vis.iter_mut() {
        *vis = match component.0.visible {
            Some(false) => Visibility::Hidden,
            _ => Visibility::Visible,
        }
    }

    for ent in removed.iter() {
        if let Some(mut commands) = commands.get_entity(ent) {
            commands.insert(Visibility::Inherited);
        }
    }
}
