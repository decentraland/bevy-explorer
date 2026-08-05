mod states;

use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::Relationship,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{
    plugin::states::*, ActiveReceiver, ActiveTransmitter, Presentation, ReceiverImage, Screenshare,
    VideoCast, VideoStream,
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
        app.add_observer(component_on_add::<Screenshare, Screensharer>);
        app.add_observer(component_on_remove::<Screenshare, Screensharer>);
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
        app.add_systems(OnEnter(Transmission::Off), drop_transmissions);
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
#[relationship_target(relationship = Screensharer)]
struct ManagingScreenshare(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingScreenshare)]
struct Screensharer(Entity);

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
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    ));

    commands.spawn(LivestreamManager(handle));
    debug!("LivestreamManager setup");
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
    trigger: Trigger<OnAdd, ActiveReceiver>,
    mut commands: Commands,
    livestream_manager: Single<&LivestreamManager>,
    mut next_state: ResMut<NextState<Receiver>>,
) {
    let entity = trigger.target();
    debug!("New receiver {}", entity);
    commands
        .entity(entity)
        .insert(ReceiverImage((**livestream_manager).clone()));
    next_state.set(Receiver::On);
}

fn receiver_off(
    trigger: Trigger<OnRemove, ActiveReceiver>,
    mut commands: Commands,
    receivers: Query<(), With<ActiveReceiver>>,
    mut next_state: ResMut<NextState<Receiver>>,
) {
    let entity = trigger.target();
    debug!("Receiver {} removed", entity);
    commands.entity(entity).try_remove::<ReceiverImage>();
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
            &ManagingScreenshare,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
    )>,
    active_transmission: Option<Single<Entity, With<ActiveTransmitter>>>,
) {
    let (livestream_manager, any_streamer) = livestream_manager.into_inner();
    let collection = match any_streamer {
        (Some(presentations), _, _, _) => presentations.collection(),
        (_, Some(screenshares), _, _) => screenshares.collection(),
        (_, _, Some(casts), _) => casts.collection(),
        (_, _, _, Some(videos)) => videos.collection(),
        _ => unreachable!("Infallible"),
    };

    let highest_priority = collection.iter().next().copied();

    let old_active = active_transmission.as_deref().copied();

    if highest_priority != old_active {
        let highest_priority = highest_priority
            .expect("Entity is available if any of the relationships are populated");
        debug!("{highest_priority} is the transmitter with highest priority.");
        if let Some(old_active) = old_active {
            commands
                .entity(old_active)
                .try_remove::<ActiveTransmitter>();
        }
        commands
            .entity(highest_priority)
            .insert(ActiveTransmitter((*livestream_manager).clone()));
    }
}

fn drop_transmissions(
    mut commands: Commands,
    transmission: Single<Entity, With<ActiveTransmitter>>,
) {
    commands
        .entity(*transmission)
        .try_remove::<ActiveTransmitter>();
}
