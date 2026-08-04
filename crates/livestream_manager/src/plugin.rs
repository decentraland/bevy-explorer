use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::Relationship,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{ActiveTransmiter, Presentation, VideoCast, VideoStream};

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

#[derive(Component, Deref)]
struct LivestreamManager(Handle<Image>);

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

fn setup_manager(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handle = images.add(Image::new_fill(
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255, 0, 255, 255],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::all(),
    ));

    commands.spawn(LivestreamManager(handle));
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
        .try_remove::<(R, ActiveTransmiter)>();
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
    active_stream: Query<Entity, With<ActiveTransmiter>>,
) -> bool {
    !livestream_manager.is_empty() && active_stream.is_empty()
}

#[expect(clippy::type_complexity)]
fn activate_transmission(
    mut commands: Commands,
    livestream_manager: Single<(
        &LivestreamManager,
        AnyOf<(
            &ManagingPresentations,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
    )>,
) {
    let (livestream_manager, any_streamer) = livestream_manager.into_inner();
    let collection = match any_streamer {
        (Some(presentations), _, _) => presentations.collection(),
        (_, Some(casts), _) => casts.collection(),
        (_, _, Some(videos)) => videos.collection(),
        _ => unreachable!("Infallible"),
    };

    let highest_priority = collection.iter().next().copied().unwrap();

    commands
        .entity(highest_priority)
        .insert(ActiveTransmiter((*livestream_manager).clone()));
}
