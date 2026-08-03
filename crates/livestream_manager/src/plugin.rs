use bevy::{ecs::relationship::Relationship, prelude::*};

use crate::{ActiveTransmission, Presentation, VideoCast, VideoStream};

pub struct LivestreamManagerPlugin;

impl Plugin for LivestreamManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_manager);

        app.add_observer(component_on_add::<Presentation, PresentationCaster>);
        app.add_observer(component_on_remove::<Presentation, PresentationCaster>);
        app.add_observer(component_on_add::<VideoCast, VideoCaster>);
        app.add_observer(component_on_remove::<VideoCast, VideoCaster>);
        app.add_observer(component_on_add::<VideoStream, VideoStreamer>);
        app.add_observer(component_on_remove::<VideoStream, VideoStreamer>);

        app.add_systems(
            Update,
            activate_transmission.run_if(transmissions_available_but_none_active),
        );
    }
}

#[derive(Component)]
struct LivestreamManager;

#[derive(Component)]
#[relationship_target(relationship = PresentationCaster)]
struct ManagingPresentations(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingPresentations)]
struct PresentationCaster(Entity);

#[derive(Component)]
#[relationship_target(relationship = VideoCaster)]
struct ManagingCasts(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingCasts)]
struct VideoCaster(Entity);

#[derive(Component)]
#[relationship_target(relationship = VideoStreamer)]
struct ManagingVideoStreams(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingVideoStreams)]
struct VideoStreamer(Entity);

fn setup_manager(mut commands: Commands) {
    commands.spawn(LivestreamManager);
}

fn component_on_add<T: Component, R: Relationship>(
    trigger: Trigger<OnAdd, T>,
    mut commands: Commands,
    livestream_manager: Single<Entity, With<LivestreamManager>>,
) {
    commands
        .entity(trigger.target())
        .insert(R::from(*livestream_manager));
}

fn component_on_remove<T: Component, R: Relationship>(
    trigger: Trigger<OnRemove, T>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.target())
        .try_remove::<(R, ActiveTransmission)>();
}

fn transmissions_available_but_none_active(
    livestream_manager: Query<
        AnyOf<(
            &ManagingPresentations,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
        With<LivestreamManager>,
    >,
    active_stream: Query<Entity, With<ActiveTransmission>>,
) -> bool {
    !livestream_manager.is_empty() && active_stream.is_empty()
}

fn activate_transmission(
    mut commands: Commands,
    livestream_manager: Single<
        AnyOf<(
            &ManagingPresentations,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
        With<LivestreamManager>,
    >,
) {
    let collection = match *livestream_manager {
        (Some(presentations), _, _) => presentations.collection(),
        (_, Some(casts), _) => casts.collection(),
        (_, _, Some(videos)) => videos.collection(),
        _ => unreachable!("Infallible"),
    };

    let highest_priority = collection.iter().next().copied().unwrap();

    commands.entity(highest_priority).insert(ActiveTransmission);
}
