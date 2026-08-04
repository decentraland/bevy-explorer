mod states;

use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::Relationship,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{
    plugin::states::*, ActiveReceiver, ActiveTransmitter, Presentation, VideoCast, VideoStream,
};

pub struct LivestreamManagerPlugin;

impl Plugin for LivestreamManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Transmitter>();
        app.init_state::<Receiver>();
        app.add_computed_state::<Transmission>();

        app.add_systems(Startup, setup_manager);

        app.add_observer(component_on_add::<Presentation, PresentationCaster>);
        app.add_observer(component_on_remove::<Presentation, PresentationCaster>);
        app.add_observer(component_on_add::<VideoCast, VideoCaster>);
        app.add_observer(component_on_remove::<VideoCast, VideoCaster>);
        app.add_observer(component_on_add::<VideoStream, VideoStreamer>);
        app.add_observer(component_on_remove::<VideoStream, VideoStreamer>);
        app.add_observer(transmitter_on);
        app.add_observer(transmitter_off);
        app.add_observer(receiver_on);
        app.add_observer(receiver_off);

        app.add_systems(
            Update,
            manage_streams.run_if(not(in_state(Transmission::Off))),
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
        .try_remove::<(R, ActiveTransmitter)>();
}

fn transmitter_on(
    _trigger: Trigger<OnAdd, ActiveTransmitter>,
    mut next_state: ResMut<NextState<Transmitter>>,
) {
    debug!("New transmitter {}", _trigger.target());
    next_state.set(Transmitter::On);
}

fn transmitter_off(
    _trigger: Trigger<OnRemove, ActiveTransmitter>,
    mut next_state: ResMut<NextState<Transmitter>>,
) {
    debug!(
        "Transmitter {} removed, transmitters are now Off",
        _trigger.target()
    );
    next_state.set(Transmitter::Off);
}

fn receiver_on(
    _trigger: Trigger<OnAdd, ActiveReceiver>,
    mut next_state: ResMut<NextState<Receiver>>,
) {
    debug!("New receiver {}", _trigger.target());
    next_state.set(Receiver::On);
}

fn receiver_off(
    _trigger: Trigger<OnRemove, ActiveReceiver>,
    receivers: Query<(), With<ActiveReceiver>>,
    mut next_state: ResMut<NextState<Receiver>>,
) {
    debug!("Receiver {} removed", _trigger.target());
    if receivers.iter().len() == 1 {
        debug!("Receivers are now Off");
        next_state.set(Receiver::Off);
    }
}

#[expect(clippy::type_complexity)]
fn manage_streams(
    mut commands: Commands,
    livestream_manager: Single<(
        &LivestreamManager,
        AnyOf<(
            &ManagingPresentations,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
    )>,
    active_transmission: Option<Single<Entity, With<ActiveTransmitter>>>,
) {
    let (livestream_manager, any_streamer) = livestream_manager.into_inner();
    let collection = match any_streamer {
        (Some(presentations), _, _) => presentations.collection(),
        (_, Some(casts), _) => casts.collection(),
        (_, _, Some(videos)) => videos.collection(),
        _ => unreachable!("Infallible"),
    };

    let highest_priority = collection.iter().next().copied();

    if highest_priority != active_transmission.as_deref().copied() {
        let highest_priority = highest_priority
            .expect("Entity is available if any of the relationships are populated");
        debug!("{highest_priority} is the transmitter with highest priority.");
        commands
            .entity(highest_priority)
            .insert(ActiveTransmitter((*livestream_manager).clone()));
    }
}
