use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateOver},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
#[component(immutable)]
#[require(Visibility)]
struct AncestorVisibility(Visibility);

fn visibility_component_on_insert(
    trigger: Trigger<OnInsert, VisibilityComponent>,
    mut commands: Commands,
    mut visibility_components: Query<(&VisibilityComponent, &mut Visibility, Option<&ChildOf>)>,
) {
    let entity = trigger.target();
    let Ok((visibility_component, mut visibility, maybe_child_of)) =
        visibility_components.get_mut(entity)
    else {
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
    } else {
        commands
            .entity(entity)
            .try_insert(PropagateOver::<AncestorVisibility>::default());
    }

    if let Some(child_of) = maybe_child_of {
        commands.entity(entity).try_insert(child_of.clone());
    }
}

fn visibility_component_on_replace(
    trigger: Trigger<OnReplace, VisibilityComponent>,
    mut commands: Commands,
    mut visibility_components: Query<(&mut Visibility, Option<&ChildOf>)>,
) {
    let entity = trigger.target();
    let Ok((mut visibility, maybe_child_of)) = visibility_components.get_mut(entity) else {
        unreachable!("Infallible query.");
    };

    *visibility = Visibility::Inherited;

    commands.entity(entity).try_remove::<(
        Propagate<AncestorVisibility>,
        PropagateOver<AncestorVisibility>,
    )>();

    if let Some(child_of) = maybe_child_of {
        commands.entity(entity).try_insert(child_of.clone());
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

    #[test]
    fn parent_visible_propagate_child_none() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }))
            .id();
        let child = world.spawn(ChildOf(parent)).id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Visible);
        assert_eq!(world.get(child).unwrap(), Visibility::Visible);
    }

    #[test]
    fn parent_visible_propagate_children_none() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }))
            .id();
        let children = (0..10)
            .map(|_| world.spawn(ChildOf(parent)).id())
            .collect::<Vec<_>>();
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
        for child in children {
            assert_eq!(world.get(child).unwrap(), Visibility::Visible);
        }
        assert_eq!(world.get(child).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn parent_visible_propagate_ancestors_none() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }))
            .id();
        let children =
            std::iter::successors(Some(parent), |prev| Some(world.spawn(ChildOf(*prev)).id()))
                .take(20)
                .collect::<Vec<_>>();
        let child = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(false),
                    propagate_to_children: Some(false),
                }),
                ChildOf(children[19]),
            ))
            .id();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Visible);
        for child in children {
            assert_eq!(world.get(child).unwrap(), Visibility::Visible);
        }
        assert_eq!(world.get(child).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn two_propagate_and_update() {
        let mut app = App::new();

        app.add_plugins(VisibilityComponentPlugin);

        app.finish();

        let world = app.world_mut();

        let parent = world
            .spawn(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }))
            .id();
        let visible_children =
            std::iter::successors(Some(parent), |prev| Some(world.spawn(ChildOf(*prev)).id()))
                .skip(1)
                .take(4)
                .collect::<Vec<_>>();
        assert!(!visible_children.contains(&parent));
        let midway_descendant = world
            .spawn((
                VisibilityComponent(PbVisibilityComponent {
                    visible: Some(false),
                    propagate_to_children: Some(true),
                }),
                ChildOf(visible_children[3]),
            ))
            .id();
        let hidden_children = std::iter::successors(Some(midway_descendant), |prev| {
            Some(world.spawn(ChildOf(*prev)).id())
        })
        .skip(1)
        .take(4)
        .collect::<Vec<_>>();
        assert!(!hidden_children.contains(&midway_descendant));

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Visible);
        assert!(world
            .entity(parent)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(parent)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &visible_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Visible);
        }
        assert_eq!(world.get(midway_descendant).unwrap(), Visibility::Hidden);
        assert!(world
            .entity(midway_descendant)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(midway_descendant)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &hidden_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }

        world
            .entity_mut(parent)
            .insert(VisibilityComponent(PbVisibilityComponent {
                visible: Some(false),
                propagate_to_children: Some(true),
            }));
        world
            .entity_mut(midway_descendant)
            .insert(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }));

        // this one requires 2 updates for some reason
        for _ in 0..2 {
            app.update();
        }

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert!(world
            .entity(parent)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(parent)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &visible_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }
        assert_eq!(world.get(midway_descendant).unwrap(), Visibility::Visible);
        assert!(world
            .entity(midway_descendant)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(midway_descendant)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &hidden_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Visible);
        }

        world
            .entity_mut(midway_descendant)
            .insert(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(false),
            }));

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert!(world
            .entity(parent)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(parent)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &visible_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }
        assert_eq!(world.get(midway_descendant).unwrap(), Visibility::Visible);
        assert!(!world
            .entity(midway_descendant)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(world
            .entity(midway_descendant)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &hidden_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }

        world
            .entity_mut(midway_descendant)
            .insert(VisibilityComponent(PbVisibilityComponent {
                visible: Some(true),
                propagate_to_children: Some(true),
            }));

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert!(world
            .entity(parent)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(parent)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &visible_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }
        assert_eq!(world.get(midway_descendant).unwrap(), Visibility::Visible);
        assert!(world
            .entity(midway_descendant)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(midway_descendant)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &hidden_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Visible);
        }

        world
            .entity_mut(midway_descendant)
            .remove::<VisibilityComponent>();

        app.update();

        let world = app.world_mut();
        assert_eq!(world.get(parent).unwrap(), Visibility::Hidden);
        assert!(world
            .entity(parent)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(parent)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &visible_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }
        assert_eq!(world.get(midway_descendant).unwrap(), Visibility::Hidden);
        assert!(!world
            .entity(midway_descendant)
            .contains::<Propagate<AncestorVisibility>>());
        assert!(!world
            .entity(midway_descendant)
            .contains::<PropagateOver<AncestorVisibility>>());
        for child in &hidden_children {
            assert_eq!(world.get(*child).unwrap(), Visibility::Hidden);
        }
    }
}
