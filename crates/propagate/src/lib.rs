#![allow(clippy::type_complexity)]
use std::marker::PhantomData;

use bevy::{
    app::{App, Plugin, Update},
    ecs::query::QueryFilter,
    prelude::{
        Changed, Children, Commands, Component, Entity, IntoSystemConfigs, Local, Parent, Query,
        RemovedComponents, SystemSet, With, Without,
    },
};

/// Causes the inner component to be added to this entity and all children.
/// A child with a Propagate<C> component of it's own will override propagation from
/// that point in the tree
#[derive(Component, Clone, PartialEq)]
pub struct Propagate<C: Component + Clone + PartialEq>(pub C);

/// Internal struct for managing propagation
#[derive(Component, Clone, PartialEq)]
pub struct Inherited<C: Component + Clone + PartialEq>(pub C);

/// Stops the output component being added to this entity.
/// Children will still inherit the component from this entity or its parents
#[derive(Component, Default)]
pub struct PropagateOver<C: Component + Clone + PartialEq>(PhantomData<fn() -> C>);

/// Stops the propagation at this entity. Children will not inherit the component.
#[derive(Component, Default)]
pub struct PropagateStop<C: Component + Clone + PartialEq>(PhantomData<fn() -> C>);

pub struct HierarchyPropagatePlugin<C: Component + Clone + PartialEq, F: QueryFilter = ()> {
    _p: PhantomData<fn() -> (C, F)>,
}

impl<C: Component + Clone + PartialEq, F: QueryFilter> Default for HierarchyPropagatePlugin<C, F> {
    fn default() -> Self {
        Self {
            _p: Default::default(),
        }
    }
}

#[derive(SystemSet, Clone, PartialEq, PartialOrd, Ord)]
pub struct PropagateSet<C: Component + Clone + PartialEq> {
    _p: PhantomData<fn() -> C>,
}

impl<C: Component + Clone + PartialEq> std::fmt::Debug for PropagateSet<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PropagateSet")
            .field("_p", &self._p)
            .finish()
    }
}

impl<C: Component + Clone + PartialEq> std::cmp::Eq for PropagateSet<C> {}
impl<C: Component + Clone + PartialEq> std::hash::Hash for PropagateSet<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self._p.hash(state);
    }
}

impl<C: Component + Clone + PartialEq> Default for PropagateSet<C> {
    fn default() -> Self {
        Self {
            _p: Default::default(),
        }
    }
}

impl<C: Component + Clone + PartialEq, F: QueryFilter + 'static> Plugin
    for HierarchyPropagatePlugin<C, F>
{
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_source::<C, F>,
                update_stopped::<C, F>,
                update_reparented::<C, F>,
                propagate_inherited::<C, F>,
                propagate_output::<C, F>,
            )
                .chain()
                .in_set(PropagateSet::<C>::default()),
        );
    }
}

pub fn update_source<C: Component + Clone + PartialEq, F: QueryFilter>(
    mut commands: Commands,
    changed: Query<(Entity, &Propagate<C>), (Changed<Propagate<C>>, Without<PropagateStop<C>>)>,
    mut removed: RemovedComponents<Propagate<C>>,
) {
    for (entity, source) in &changed {
        commands
            .entity(entity)
            .try_insert(Inherited(source.0.clone()));
    }

    for removed in removed.read() {
        if let Some(mut commands) = commands.get_entity(removed) {
            commands.remove::<(Inherited<C>, C)>();
        }
    }
}

pub fn update_stopped<C: Component + Clone + PartialEq, F: QueryFilter>(
    mut commands: Commands,
    q: Query<Entity, (With<Inherited<C>>, F, With<PropagateStop<C>>)>,
) {
    for entity in q.iter() {
        let mut cmds = commands.entity(entity);
        cmds.remove::<Inherited<C>>();
    }
}

pub fn update_reparented<C: Component + Clone + PartialEq, F: QueryFilter>(
    mut commands: Commands,
    moved: Query<
        (Entity, &Parent, Option<&Inherited<C>>),
        (
            Changed<Parent>,
            Without<Propagate<C>>,
            Without<PropagateStop<C>>,
            F,
        ),
    >,
    parents: Query<&Inherited<C>>,
) {
    for (entity, parent, maybe_inherited) in &moved {
        if let Ok(inherited) = parents.get(parent.get()) {
            commands.entity(entity).try_insert(inherited.clone());
        } else if maybe_inherited.is_some() {
            commands.entity(entity).remove::<(Inherited<C>, C)>();
        }
    }
}

pub fn propagate_inherited<C: Component + Clone + PartialEq, F: QueryFilter>(
    mut commands: Commands,
    changed: Query<
        (&Inherited<C>, &Children),
        (Changed<Inherited<C>>, Without<PropagateStop<C>>, F),
    >,
    recurse: Query<
        (Option<&Children>, Option<&Inherited<C>>),
        (Without<Propagate<C>>, Without<PropagateStop<C>>, F),
    >,
    mut to_process: Local<Vec<(Entity, Option<Inherited<C>>)>>,
    mut removed: RemovedComponents<Inherited<C>>,
) {
    // gather changed
    for (inherited, children) in &changed {
        to_process.extend(
            children
                .iter()
                .map(|child| (*child, Some(inherited.clone()))),
        );
    }

    // and removed
    for entity in removed.read() {
        if let Ok((Some(children), _)) = recurse.get(entity) {
            to_process.extend(children.iter().map(|child| (*child, None)))
        }
    }

    // propagate
    while let Some((entity, maybe_inherited)) = (*to_process).pop() {
        let Ok((maybe_children, maybe_current)) = recurse.get(entity) else {
            continue;
        };

        if maybe_current == maybe_inherited.as_ref() {
            continue;
        }

        if let Some(children) = maybe_children {
            to_process.extend(
                children
                    .iter()
                    .map(|child| (*child, maybe_inherited.clone())),
            );
        }

        if let Some(inherited) = maybe_inherited {
            commands.entity(entity).try_insert(inherited.clone());
        } else {
            commands.entity(entity).remove::<(Inherited<C>, C)>();
        }
    }
}

pub fn propagate_output<C: Component + Clone + PartialEq, F: QueryFilter>(
    mut commands: Commands,
    changed: Query<
        (Entity, &Inherited<C>, Option<&C>),
        (Changed<Inherited<C>>, Without<PropagateOver<C>>, F),
    >,
) {
    for (entity, inherited, maybe_current) in &changed {
        if maybe_current.is_some_and(|c| &inherited.0 == c) {
            continue;
        }

        commands.entity(entity).try_insert(inherited.0.clone());
    }
}
