use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::RelationshipSourceCollection,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{states::*, *};

pub struct LivestreamManagerPlugin;

impl Plugin for LivestreamManagerPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<TransmissionKind>();
        app.init_state::<Transmitter>();
        app.init_state::<Receiver>();
        app.add_computed_state::<TransmissionState>();

        app.add_systems(Startup, setup_manager);

        app.add_observer(transmitter_kind_on_insert);
        app.add_observer(transmitter_kind_on_replace);
        app.add_observer(audio_transmitter_kind_on_insert);
        app.add_observer(audio_transmitter_kind_on_replace);
        app.add_observer(active_video_cast_on_add);
        app.add_observer(active_video_cast_on_remove);
        app.add_observer(transmitter_on);
        app.add_observer(transmitter_off);
        app.add_observer(receiver_on);
        app.add_observer(receiver_off);

        app.add_systems(
            Update,
            (
                transmission_kind,
                manage_streams.run_if(in_state(TransmissionState::NeedStream)),
                manage_casts.run_if(in_state(TransmissionState::Cast)),
            )
                .chain(),
        );
        app.add_systems(OnEnter(Receiver::Off), drop_transmissions);
        app.add_systems(
            OnEnter(TransmissionState::Cast),
            enter_transmission_kind::<ManagingAudioCasts>,
        );
        app.add_systems(
            OnExit(TransmissionState::Cast),
            exit_transmission_kind::<ManagingAudioCasts>,
        );
        app.add_systems(
            OnEnter(TransmissionState::Stream),
            enter_transmission_kind::<ManagingAudioStreams>,
        );
        app.add_systems(
            OnExit(TransmissionState::Stream),
            exit_transmission_kind::<ManagingAudioStreams>,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TransmissionState {
    Off,
    NeedStream,
    Stream,
    Cast,
}

impl ComputedStates for TransmissionState {
    type SourceStates = (TransmissionKind, Transmitter, Receiver);

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            (TransmissionKind::Off, _, _) | (_, _, Receiver::Off) => Some(TransmissionState::Off),
            (TransmissionKind::Stream, Transmitter::Off, _) => Some(TransmissionState::NeedStream),
            (TransmissionKind::Stream, Transmitter::On, _) => Some(TransmissionState::Stream),
            (TransmissionKind::Cast, _, _) => Some(TransmissionState::Cast),
        }
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
#[relationship_target(relationship = ActiveVideoCaster)]
struct ManagingActiveCasts(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingActiveCasts)]
struct ActiveVideoCaster(Entity);

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

#[derive(Component)]
#[relationship_target(relationship = AudioCaster)]
struct ManagingAudioCasts(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingAudioCasts)]
struct AudioCaster(Entity);

#[derive(Component)]
#[relationship_target(relationship = AudioStreamer)]
struct ManagingAudioStreams(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ManagingAudioStreams)]
struct AudioStreamer(Entity);

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

fn transmitter_kind_on_insert(
    trigger: Trigger<OnInsert, TransmitterKind>,
    mut commands: Commands,
    livestream_manager: Single<Entity, With<LivestreamManager>>,
    transmitter: Query<&TransmitterKind>,
) {
    let entity = trigger.target();
    let Ok(transmitter_kind) = transmitter.get(entity) else {
        unreachable!("Infallible query");
    };

    debug!("New {:?} {}", transmitter_kind, entity);
    let mut entity_cmd = commands.entity(trigger.target());

    match transmitter_kind {
        TransmitterKind::Presentation => {
            entity_cmd.insert(PresentationCaster(*livestream_manager));
        }
        TransmitterKind::Screenshare => {
            entity_cmd.insert(Screensharer(*livestream_manager));
        }
        TransmitterKind::VideoCast => {
            entity_cmd.insert(VideoCaster(*livestream_manager));
        }
        TransmitterKind::Stream => {
            entity_cmd.insert(VideoStreamer(*livestream_manager));
        }
    }
}

fn transmitter_kind_on_replace(
    trigger: Trigger<OnReplace, TransmitterKind>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    debug!("TransmitterKind replaced on {}", entity);
    commands.entity(entity).try_remove::<(
        PresentationCaster,
        Screensharer,
        ActiveVideoCaster,
        VideoCaster,
        VideoStreamer,
        ActiveVideoCast,
        ActiveTransmitter,
    )>();
}

fn audio_transmitter_kind_on_insert(
    trigger: Trigger<OnInsert, AudioTransmitterKind>,
    mut commands: Commands,
    livestream_manager: Single<Entity, With<LivestreamManager>>,
    transmitter: Query<&AudioTransmitterKind>,
    transmission_state: Res<State<TransmissionState>>,
) {
    let entity = trigger.target();
    let Ok(transmitter_kind) = transmitter.get(entity) else {
        unreachable!("Infallible query");
    };

    let mut entity_cmd = commands.entity(trigger.target());

    match transmitter_kind {
        AudioTransmitterKind::Cast => {
            entity_cmd.insert(AudioCaster(*livestream_manager));
            if *transmission_state == TransmissionState::Cast {
                entity_cmd.insert(ActiveAudioTransmitter);
            }
        }
        AudioTransmitterKind::Stream => {
            entity_cmd.insert(AudioStreamer(*livestream_manager));
            if matches!(
                **transmission_state,
                TransmissionState::NeedStream | TransmissionState::Stream
            ) {
                entity_cmd.insert(ActiveAudioTransmitter);
            }
        }
    }
}

fn audio_transmitter_kind_on_replace(
    trigger: Trigger<OnReplace, AudioTransmitterKind>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    commands
        .entity(entity)
        .try_remove::<(AudioCaster, AudioStreamer, ActiveAudioTransmitter)>();
}

fn active_video_cast_on_add(
    trigger: Trigger<OnAdd, ActiveVideoCast>,
    mut commands: Commands,
    livestream_manager: Single<Entity, With<LivestreamManager>>,
) {
    commands
        .entity(trigger.target())
        .insert(ActiveVideoCaster(*livestream_manager));
}

fn active_video_cast_on_remove(
    trigger: Trigger<OnRemove, ActiveVideoCast>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.target())
        .try_remove::<ActiveVideoCaster>();
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
fn transmission_kind(
    livestream_manager: Single<
        (
            Option<&ManagingPresentations>,
            Option<&ManagingScreenshare>,
            Option<&ManagingActiveCasts>,
            Option<&ManagingCasts>,
            Option<&ManagingVideoStreams>,
        ),
        With<LivestreamManager>,
    >,
    transmission_kind: Res<State<TransmissionKind>>,
    mut next_transmission_kind: ResMut<NextState<TransmissionKind>>,
) {
    let has_presentations = livestream_manager.0.is_some();
    let has_screenshares = livestream_manager.1.is_some();
    let has_active_casts = livestream_manager.2.is_some();
    let has_casts = livestream_manager.3.is_some();
    let has_streams = livestream_manager.4.is_some();
    let new_transmission_kind =
        if has_presentations || has_screenshares || has_active_casts || has_casts {
            TransmissionKind::Cast
        } else if has_streams {
            TransmissionKind::Stream
        } else {
            TransmissionKind::Off
        };

    if new_transmission_kind != **transmission_kind {
        debug!("Changing transmission kind to {:?}", new_transmission_kind);
        next_transmission_kind.set(new_transmission_kind);
    }
}

#[expect(clippy::type_complexity)]
fn manage_casts(
    mut commands: Commands,
    livestream_manager: Single<(
        &LivestreamManager,
        AnyOf<(
            &ManagingPresentations,
            &ManagingScreenshare,
            &ManagingActiveCasts,
            &ManagingCasts,
            &ManagingVideoStreams,
        )>,
    )>,
    active_transmission: Option<Single<Entity, With<ActiveTransmitter>>>,
) {
    let (livestream_manager, any_streamer) = livestream_manager.into_inner();
    let collection = match any_streamer {
        (Some(presentations), _, _, _, _) => presentations.collection(),
        (_, Some(screenshares), _, _, _) => screenshares.collection(),
        (_, _, Some(active_casts), _, _) => active_casts.collection(),
        (_, _, _, Some(casts), _) => casts.collection(),
        (_, _, _, _, Some(videos)) => videos.collection(),
        _ => unreachable!("Infallible"),
    };

    let highest_priority = collection.iter().next();

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

fn manage_streams(
    mut commands: Commands,
    livestream_manager: Single<(&LivestreamManager, &ManagingVideoStreams)>,
    maybe_active_transmitter: Option<Single<Entity, With<ActiveTransmitter>>>,
) {
    let (livestream_manager, managing_video_streams) = livestream_manager.into_inner();
    let Some(stream) = managing_video_streams.collection().first() else {
        // It is possible to be in TransmissionKind::Stream an not
        // have a stream because states changes happen on the start of
        // the next frame
        trace!("No available streams");
        return;
    };

    if let Some(active_transmitter) = maybe_active_transmitter {
        debug!(
            "{} replaced by {} as ActiveTransmitter",
            *active_transmitter, *stream
        );
        commands
            .entity(*active_transmitter)
            .try_remove::<ActiveTransmitter>();
    }
    debug!("{} now ActiveTransmitter", *stream);
    commands
        .entity(*stream)
        .try_insert(ActiveTransmitter((*livestream_manager).clone()));
}

fn enter_transmission_kind<'w, R: RelationshipTarget>(
    mut commands: Commands,
    livestream_manager: Single<'w, &R, With<LivestreamManager>>,
) {
    for entity in livestream_manager.collection().iter() {
        commands.entity(entity).try_insert(ActiveAudioTransmitter);
    }
}

fn exit_transmission_kind<'w, R: RelationshipTarget>(
    mut commands: Commands,
    livestream_manager: Single<'w, &R, With<LivestreamManager>>,
) {
    for entity in livestream_manager.collection().iter() {
        commands
            .entity(entity)
            .try_remove::<ActiveAudioTransmitter>();
    }
}
