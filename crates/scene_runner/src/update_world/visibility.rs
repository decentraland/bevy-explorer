use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    prelude::*,
};
#[cfg(not(test))]
use dcl::interface::ComponentPosition;
use dcl_component::proto_components::sdk::components::PbVisibilityComponent;
#[cfg(not(test))]
use dcl_component::SceneComponentId;

#[cfg(not(test))]
use super::AddCrdtInterfaceExt;

pub struct VisibilityComponentPlugin;

impl Plugin for VisibilityComponentPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(test))]
        app.add_crdt_lww_component::<PbVisibilityComponent, VisibilityComponent>(
            SceneComponentId::VISIBILITY,
            ComponentPosition::EntityOnly,
        );

        app.add_plugins(HierarchyPropagatePlugin::<AncestorVisibility>::default());

        app.add_observer(visibility_component_on_insert);
        app.add_observer(visibility_component_on_replace);
        app.add_observer(ancestor_visibility_on_insert);
    }
}

#[derive(Component, Deref)]
#[component(immutable)]
#[require(Visibility)]
pub struct VisibilityComponent(pub PbVisibilityComponent);

impl From<PbVisibilityComponent> for VisibilityComponent {
    fn from(value: PbVisibilityComponent) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Component)]
struct AncestorVisibility(Visibility);

fn visibility_component_on_insert(
    trigger: Trigger<OnInsert, VisibilityComponent>,
    mut commands: Commands,
    mut visibility_components: Query<(&VisibilityComponent, &mut Visibility)>,
) {
    let entity = trigger.target();
    let Ok((visibility_component, mut visibility)) = visibility_components.get_mut(entity) else {
        unreachable!("Infallible query.");
    };

    *visibility = match visibility_component.visible() {
        true => Visibility::Visible,
        false => Visibility::Hidden,
    };

    if visibility_component.propagate_to_children() {
        commands
            .entity(entity)
            .try_insert(Propagate(AncestorVisibility(*visibility)));
    }
}

fn visibility_component_on_replace(
    trigger: Trigger<OnReplace, VisibilityComponent>,
    mut commands: Commands,
    mut visibility_components: Query<(&VisibilityComponent, &mut Visibility)>,
) {
    let entity = trigger.target();
    let Ok((visibility_component, mut visibility)) = visibility_components.get_mut(entity) else {
        unreachable!("Infallible query.");
    };

    *visibility = Visibility::Inherited;

    if visibility_component.propagate_to_children() {
        commands
            .entity(entity)
            .try_remove::<Propagate<AncestorVisibility>>();
    }
}

fn ancestor_visibility_on_insert(
    trigger: Trigger<OnInsert, AncestorVisibility>,
    mut visibility_components: Query<(
        &AncestorVisibility,
        Has<VisibilityComponent>,
        &mut Visibility,
    )>,
) {
    let entity = trigger.target();
    let Ok((ancestor_visibility, has_visibility_component, mut visibility)) =
        visibility_components.get_mut(entity)
    else {
        unreachable!("Infallible query.");
    };

    if !has_visibility_component {
        *visibility = ancestor_visibility.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_visible_child_visible() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(false),
            }))
            .id();
        let child = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(true),
                    propagate_to_children: Some(false),
                }),
                ChildOf(parent),
            ))
            .id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Visible);
        assert_eq!(world.get(child).unwrap(), Visibility::Visible);
    }

    #[test]
    fn parent_visible_child_hidden() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(false),
            }))
            .id();
        let child = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(false),
                    propagate_to_children: Some(false),
                }),
                ChildOf(parent),
            ))
            .id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Visible);
        assert_eq!(world.get(child).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn parent_hidden_child_visible() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(false),
                propagate_to_children: Some(false),
            }))
            .id();
        let child = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(true),
                    propagate_to_children: Some(false),
                }),
                ChildOf(parent),
            ))
            .id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert_eq!(world.get(child).unwrap(), Visibility::Visible);
    }

    #[test]
    fn parent_hidden_child_hidden() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(false),
                propagate_to_children: Some(false),
            }))
            .id();
        let child = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(false),
                    propagate_to_children: Some(false),
                }),
                ChildOf(parent),
            ))
            .id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert_eq!(world.get(child).unwrap(), Visibility::Hidden);
    }
}
