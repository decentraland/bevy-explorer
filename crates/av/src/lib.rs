#[cfg(test)]
pub mod test;

// util
#[cfg(feature = "ffmpeg")]
pub mod audio_sink;
#[cfg(feature = "ffmpeg")]
pub mod video_stream;

// audio source (non-streaming audio)
pub mod audio_loader;
pub mod audio_source;
#[cfg(not(feature = "html"))]
pub mod audio_source_native;
#[cfg(feature = "html")]
pub mod audio_source_wasm;

// video
#[cfg(feature = "html")]
pub mod html_video_player;
#[cfg(feature = "ffmpeg")]
pub mod video_player;

#[cfg(feature = "av_player_debug")]
pub mod av_player_debug;

use std::{borrow::Borrow, cmp::Ordering, collections::BTreeSet, marker::PhantomData, ops::Deref};

#[cfg(feature = "ffmpeg")]
use crate::{audio_sink::AudioSink, video_stream::VideoSink};
use audio_source::AudioSourcePlugin;
#[cfg(not(feature = "html"))]
use audio_source_native::AudioSourcePluginImpl;
#[cfg(feature = "ffmpeg")]
use bevy::ecs::component::Mutable;
use bevy::{diagnostic::FrameCount, math::FloatOrd, prelude::*};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        PbAudioEvent, PbAudioStream, PbVideoEvent, PbVideoPlayer, VideoState,
    },
    CrdtType, SceneComponentId,
};
use livestream_manager::{
    ActiveReceiver, ReceiverImage, ReceiverVolume, TransmissionStopped, TransmissionUpdated,
};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

#[cfg(all(not(test), feature = "ffmpeg"))]
use crate::audio_sink::AudioSinkPlugin;
#[cfg(feature = "ffmpeg")]
use crate::video_player::VideoPlayerPlugin;
#[cfg(feature = "html")]
use crate::{
    // foreign players
    audio_source_wasm::AudioSourcePluginImpl,
    html_video_player::VideoPlayerPlugin,
};

const LIVEKIT_VIDEO_STREAM: &str = "livekit-video://current-stream";

pub trait AVPlayer: Component {
    type Source: Component + Deref<Target = str> + PartialEq;
    type Config: Component + AVPlayerConfig + PartialEq;
    type Position: Component + Deref<Target = f32> + PartialEq;

    const ALLOWS_LIVESTREAM: bool;

    fn url(&self) -> &str;
    fn source(&self) -> Self::Source;
    fn config(&self) -> Self::Config;
    fn position(&self) -> Self::Position;

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, video_sink: VideoSink) -> AVSinks<Self>
    where
        Self: Sized;

    fn has_video() -> bool;
}

pub trait AVPlayerConfig {
    fn playing(&self) -> bool;
    fn volume(&self) -> f32;
    fn playback_rate(&self) -> f32;
    fn r#loop(&self) -> bool;
}

#[cfg(feature = "ffmpeg")]
pub trait AVPlayerSinks: Component<Mutability = Mutable> {
    fn audio_sink(&self) -> Option<&AudioSink>;
    fn audio_sink_mut(&mut self) -> Option<&mut AudioSink>;
    fn video_sink(&self) -> Option<&VideoSink>;
    fn video_sink_mut(&mut self) -> Option<&mut VideoSink>;
}

#[cfg(feature = "ffmpeg")]
#[derive(Component)]
pub struct AVSinks<T: AVPlayer> {
    pub audio: Option<AudioSink>,
    pub video: Option<VideoSink>,
    pub _phantom: PhantomData<T>,
}

#[cfg(feature = "ffmpeg")]
impl<T: AVPlayer> AVPlayerSinks for AVSinks<T> {
    fn audio_sink(&self) -> Option<&AudioSink> {
        self.audio.as_ref()
    }

    fn audio_sink_mut(&mut self) -> Option<&mut AudioSink> {
        self.audio.as_mut()
    }

    fn video_sink(&self) -> Option<&VideoSink> {
        self.video.as_ref()
    }

    fn video_sink_mut(&mut self) -> Option<&mut VideoSink> {
        self.video.as_mut()
    }
}

#[derive(Component, Deref)]
#[component(immutable)]
pub struct AudioStream(PbAudioStream);

impl From<PbAudioStream> for AudioStream {
    fn from(value: PbAudioStream) -> Self {
        Self(value)
    }
}

impl AVPlayer for AudioStream {
    type Source = AudioStreamSource;
    type Config = AudioStreamConfig;
    type Position = AudioStreamPosition;

    const ALLOWS_LIVESTREAM: bool = false;

    fn url(&self) -> &str {
        &self.url
    }

    fn source(&self) -> Self::Source {
        AudioStreamSource(self.url.to_owned())
    }

    fn config(&self) -> Self::Config {
        AudioStreamConfig {
            playing: self.playing.unwrap_or(true),
            volume: self.volume.unwrap_or(1.),
        }
    }

    fn position(&self) -> Self::Position {
        AudioStreamPosition(0.)
    }

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, _video_sink: VideoSink) -> AVSinks<Self> {
        AVSinks {
            audio: Some(audio_sink),
            video: None,
            _phantom: Default::default(),
        }
    }

    fn has_video() -> bool {
        false
    }
}

#[derive(Component, Deref)]
#[component(immutable)]
pub struct VideoPlayer(PbVideoPlayer);

impl From<PbVideoPlayer> for VideoPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self(value)
    }
}

impl AVPlayer for VideoPlayer {
    type Source = VideoPlayerSource;
    type Config = VideoPlayerConfig;
    type Position = VideoPlayerPosition;

    const ALLOWS_LIVESTREAM: bool = true;

    fn url(&self) -> &str {
        &self.src
    }

    fn source(&self) -> Self::Source {
        VideoPlayerSource(self.src.to_owned())
    }

    fn config(&self) -> Self::Config {
        VideoPlayerConfig {
            playing: self.playing.unwrap_or(true),
            volume: self.volume.unwrap_or(1.),
            playback_rate: self.playback_rate.unwrap_or(1.),
            r#loop: self.r#loop.unwrap_or(false),
        }
    }

    fn position(&self) -> Self::Position {
        VideoPlayerPosition(self.position.unwrap_or(0.))
    }

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, video_sink: VideoSink) -> AVSinks<Self> {
        AVSinks {
            audio: Some(audio_sink),
            video: Some(video_sink),
            _phantom: Default::default(),
        }
    }

    fn has_video() -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamSource(String);

impl<T: Borrow<AudioStream>> From<T> for AudioStreamSource {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.url.to_owned())
    }
}

impl Deref for AudioStreamSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamConfig {
    playing: bool,
    volume: f32,
}

impl<T: Borrow<AudioStream>> From<T> for AudioStreamConfig {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self {
            playing: borrow.playing.unwrap_or(true),
            volume: borrow.volume.unwrap_or(1.),
        }
    }
}

impl AVPlayerConfig for AudioStreamConfig {
    fn playing(&self) -> bool {
        self.playing
    }

    fn volume(&self) -> f32 {
        self.volume
    }

    fn playback_rate(&self) -> f32 {
        1.
    }

    fn r#loop(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamPosition(f32);

impl<T: Borrow<AudioStream>> From<T> for AudioStreamPosition {
    fn from(_value: T) -> Self {
        Self(0.)
    }
}

impl Deref for AudioStreamPosition {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, PartialEq, Component)]
#[component(immutable)]
pub struct VideoPlayerSource(String);

impl Deref for VideoPlayerSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerSource {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.src.to_owned())
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct VideoPlayerConfig {
    pub playing: bool,
    pub volume: f32,
    pub playback_rate: f32,
    pub r#loop: bool,
}

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerConfig {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self {
            playing: borrow.playing.unwrap_or(true),
            volume: borrow.volume.unwrap_or(1.),
            playback_rate: borrow.playback_rate.unwrap_or(1.),
            r#loop: borrow.r#loop.unwrap_or(false),
        }
    }
}

impl AVPlayerConfig for VideoPlayerConfig {
    fn playing(&self) -> bool {
        self.playing
    }

    fn volume(&self) -> f32 {
        self.volume
    }

    fn playback_rate(&self) -> f32 {
        self.playback_rate
    }

    fn r#loop(&self) -> bool {
        self.r#loop
    }
}

#[derive(Clone, Copy, PartialEq, Component, Deref)]
#[component(immutable)]
pub struct VideoPlayerPosition(f32);

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerPosition {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.position.unwrap_or(0.))
    }
}

/// Marks whether an [`AVPlayer`] should be playing
#[derive(Debug, Component)]
pub struct ShouldBePlaying<T>(PhantomData<T>);

impl<T> Default for ShouldBePlaying<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

/// Marks whether an [`AVPlayer`] is in the same scene as the [`PrimaryUser`]
#[derive(Debug, Component)]
pub struct InScene;

#[derive(Debug, Component)]
pub struct Stream;

#[derive(Default)]
pub struct AVPlayerPlugin;

impl Plugin for AVPlayerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(any(feature = "ffmpeg", feature = "html"))]
        app.add_plugins(VideoPlayerPlugin);
        #[cfg(all(not(test), feature = "ffmpeg"))]
        app.add_plugins(AudioSinkPlugin);
        app.add_plugins(AudioSourcePlugin);
        app.add_plugins(AudioSourcePluginImpl);

        app.add_crdt_lww_component::<PbAudioStream, AudioStream>(
            SceneComponentId::AUDIO_STREAM,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbVideoPlayer, VideoPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (
                av_player_is_in_scene,
                (
                    audio_stream_should_be_playing,
                    video_player_should_be_playing,
                ),
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );

        app.add_observer(av_player_on_insert::<AudioStream>);
        app.add_observer(av_player_on_insert::<VideoPlayer>);
        app.add_observer(av_player_on_remove::<AudioStream>);
        app.add_observer(av_player_on_remove::<VideoPlayer>);
        app.add_observer(stream_on_add::<VideoPlayer>);
        app.add_observer(stream_on_remove);
        app.add_observer(video_texture_output_inserted);
        app.add_observer(video_texture_output_replaced);
        app.add_observer(should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(should_be_playing_on_remove::<VideoPlayer>);

        app.add_observer(receiver_image_added);
        app.add_observer(receiver_image_removed);

        app.add_systems(Update, (receiver_image_updated, transmission_stopped));

        app.add_observer(set_state::<AudioStream>);
        app.add_observer(set_state::<VideoPlayer>);

        #[cfg(feature = "av_player_debug")]
        app.add_plugins(av_player_debug::AvPlayerDebugPlugin);
    }
}

#[expect(clippy::type_complexity)]
fn av_player_on_insert<T: AVPlayer>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    mut av_players: Query<(
        &T,
        &ContainerEntity,
        Option<&T::Source>,
        Option<&T::Config>,
        Option<&T::Position>,
    )>,
) {
    let entity = trigger.target();
    debug!("{} {} updated.", disqualified::ShortName::of::<T>(), entity);
    let Ok((av_player, container_entity, maybe_source, mut maybe_config, mut maybe_position)) =
        av_players.get_mut(entity)
    else {
        return;
    };

    let source_url = av_player.url();

    if maybe_source.is_none_or(|src| &(**src) != source_url) {
        debug!(
            "{}'s {} diverges",
            entity,
            disqualified::ShortName::of::<T::Source>()
        );

        let new_source = av_player.source();
        commands.entity(entity).try_insert(new_source);
        commands.trigger(SetState::<T> {
            entity: *container_entity,
            state: VideoState::VsLoading,
            _phantom: PhantomData,
        });

        let _ = maybe_config.take();
        let _ = maybe_position.take();
    }

    let new_config = av_player.config();
    if maybe_config.is_none_or(|config| (*config) != new_config) {
        debug!(
            "{}'s {} updated",
            entity,
            disqualified::ShortName::of::<T::Config>(),
        );
        commands.entity(entity).try_insert(new_config);
    }

    let new_position = av_player.position();
    if maybe_position.is_none_or(|position| ((**position) - *new_position).abs() >= 0.5) {
        debug!(
            "{}'s {} updated",
            entity,
            disqualified::ShortName::of::<T::Source>(),
        );
        commands.entity(entity).try_insert(new_position);
    }
}

fn av_player_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, T>,
    mut commands: Commands,
    av_players: Query<&ContainerEntity, With<T>>,
) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<(
        T::Source,
        T::Config,
        T::Position,
        InScene,
        ShouldBePlaying<T>,
        Stream,
    )>();
    #[cfg(feature = "ffmpeg")]
    commands.entity(entity).try_remove::<AVSinks<T>>();

    let Ok(container_entity) = av_players.get(entity) else {
        return;
    };

    commands.trigger(SetState::<T> {
        entity: *container_entity,
        state: VideoState::VsNone,
        _phantom: PhantomData,
    });
}

#[expect(clippy::type_complexity)]
fn stream_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, Stream>,
    mut commands: Commands,
    av_players: Query<(Has<ShouldBePlaying<T>>, Option<&T::Config>), (With<T>, With<Stream>)>,
) {
    let entity = trigger.target();
    let Ok((has_should_be_playing, maybe_config)) = av_players.get(entity) else {
        unreachable!("Infallible query");
    };

    if has_should_be_playing {
        debug!("New stream {} should be playing.", entity);
        commands.entity(entity).try_insert((
            ActiveReceiver,
            ReceiverVolume(maybe_config.map(|config| config.volume()).unwrap_or(0.)),
        ));
    }
}

fn stream_on_remove(trigger: Trigger<OnRemove, Stream>, mut commands: Commands) {
    let entity = trigger.target();
    debug!("{} no longer stream.", entity);
    commands
        .entity(entity)
        .try_remove::<(ActiveReceiver, ReceiverImage, VideoTextureOutput)>();
}

fn video_texture_output_inserted(trigger: Trigger<OnInsert, VideoTextureOutput>) {
    let entity = trigger.target();
    debug!("VideoTextureOutput inserted into {}", entity);
}

fn video_texture_output_replaced(trigger: Trigger<OnReplace, VideoTextureOutput>) {
    let entity = trigger.target();
    debug!("VideoTextureOutput replaced into {}", entity);
}

#[expect(clippy::type_complexity)]
fn should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, ShouldBePlaying<T>>,
    mut commands: Commands,
    av_players: Query<(Has<Stream>, Option<&T::Config>), (With<T>, With<ShouldBePlaying<T>>)>,
) {
    let entity = trigger.target();
    let Ok((has_stream, maybe_config)) = av_players.get(entity) else {
        unreachable!("Infallible query");
    };

    if has_stream {
        debug!("Stream {} should be playing.", entity);
        commands.entity(entity).try_insert((
            ActiveReceiver,
            ReceiverVolume(maybe_config.map(|config| config.volume()).unwrap_or(0.)),
        ));
    }
}

fn should_be_playing_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, ShouldBePlaying<T>>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    debug!("Stream {} no longer playing.", entity);
    commands.entity(entity).try_remove::<ActiveReceiver>();
}

#[expect(clippy::type_complexity)]
fn av_player_is_in_scene(
    mut commands: Commands,
    av_players: Query<
        (Entity, &ContainerEntity, Has<InScene>),
        Or<(With<AudioStream>, With<VideoPlayer>)>,
    >,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };
    let containing_scenes = containing_scene.get_position(user.translation());

    for (ent, container, has_in_scene) in av_players.iter() {
        let contained = containing_scenes.contains(&container.root);
        if contained && !has_in_scene {
            // Only call `insert` on those that do not have `InScene`
            commands.entity(ent).try_insert(InScene);
        } else if !contained && has_in_scene {
            // Only call `remove` on those that have `InScene`
            commands.entity(ent).try_remove::<InScene>();
        }
    }
}

#[expect(clippy::type_complexity)]
fn audio_stream_should_be_playing(
    mut commands: Commands,
    av_players: Query<(
        Entity,
        &AudioStream,
        Has<InScene>,
        Has<ShouldBePlaying<AudioStream>>,
    )>,
) {
    for (entity, audio_stream, in_scene, should_be_playing) in av_players {
        match (in_scene, should_be_playing, audio_stream.playing()) {
            (false, true, _) | (_, true, false) => {
                commands
                    .entity(entity)
                    .try_remove::<ShouldBePlaying<AudioStream>>();
            }
            (true, false, true) => {
                commands
                    .entity(entity)
                    .try_insert(ShouldBePlaying::<AudioStream>::default());
            }
            _ => (),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShouldBePlayingCandidate {
    entity: Entity,
    in_scene: bool,
    has_should_be_playing: bool,
    distance_to_player: FloatOrd,
}

impl PartialOrd for ShouldBePlayingCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ShouldBePlayingCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match other.in_scene.cmp(&self.in_scene) {
            Ordering::Equal => (),
            cmp => return cmp,
        }

        match self.distance_to_player.cmp(&other.distance_to_player) {
            Ordering::Equal => (),
            cmp => return cmp,
        }

        self.entity.cmp(&other.entity)
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn video_player_should_be_playing(
    mut commands: Commands,
    av_players: Query<(
        Entity,
        &<VideoPlayer as AVPlayer>::Source,
        &<VideoPlayer as AVPlayer>::Config,
        Has<InScene>,
        Has<ShouldBePlaying<VideoPlayer>>,
        &GlobalTransform,
    )>,
    user: Single<&GlobalTransform, With<PrimaryUser>>,
    config: Res<AppConfig>,
    mut scratch_should_be_playing: Local<BTreeSet<ShouldBePlayingCandidate>>,
    mut scratch_shouldnt_be_playing: Local<Vec<Entity>>,
) {
    for (entity, source, config, has_in_scene, has_should_be_playing, global_transform) in
        av_players
    {
        if !config.playing() || (&**source == LIVEKIT_VIDEO_STREAM && !has_in_scene) {
            if has_should_be_playing {
                scratch_shouldnt_be_playing.push(entity);
            }
            continue;
        }

        scratch_should_be_playing.insert(ShouldBePlayingCandidate {
            entity,
            in_scene: has_in_scene,
            has_should_be_playing,
            distance_to_player: FloatOrd(
                global_transform.translation().distance(user.translation()),
            ),
        });
    }

    // Removing first for better Trigger ordering
    for ent in scratch_should_be_playing
        .iter()
        .skip(config.max_videos)
        // Only call remove on those that have `ShouldBePlaying`
        // The `filter` MUST be after the `skip`
        .filter(|candidate| candidate.has_should_be_playing)
        .map(|candidate| candidate.entity)
        .chain(scratch_shouldnt_be_playing.drain(..))
    {
        commands
            .entity(ent)
            .try_remove::<ShouldBePlaying<VideoPlayer>>();
    }

    for candidate in scratch_should_be_playing.iter().take(config.max_videos) {
        if candidate.distance_to_player == FloatOrd(f32::MAX) {
            if candidate.has_should_be_playing {
                commands
                    .entity(candidate.entity)
                    .try_remove::<ShouldBePlaying<VideoPlayer>>();
            }
        } else {
            if !candidate.has_should_be_playing {
                commands
                    .entity(candidate.entity)
                    .try_insert(ShouldBePlaying::<VideoPlayer>::default());
            }
        }
    }

    scratch_should_be_playing.clear();
    scratch_shouldnt_be_playing.clear();
}

fn receiver_image_added(
    trigger: Trigger<OnAdd, ReceiverImage>,
    mut commands: Commands,
    video_players: Query<&ReceiverImage, (With<VideoPlayer>, With<Stream>)>,
) {
    let entity = trigger.target();

    if let Ok(receiver_image) = video_players.get(entity) {
        debug!("ReceiverImage added to {}", entity);
        commands
            .entity(entity)
            .try_insert(VideoTextureOutput((*receiver_image).clone()));
    }
}

fn receiver_image_removed(trigger: Trigger<OnRemove, ReceiverImage>, mut commands: Commands) {
    let entity = trigger.target();
    debug!("ReceiverImage removed from {}", entity);
    commands.entity(entity).try_remove::<VideoTextureOutput>();
}

fn receiver_image_updated(
    mut commands: Commands,
    av_players: Query<(Entity, &ContainerEntity, &mut VideoTextureOutput), With<ReceiverImage>>,
    mut transmission_updated: EventReader<TransmissionUpdated>,
) {
    if transmission_updated.read().count() > 0 {
        for (entity, container_entity, mut video_texture_output) in av_players {
            debug!("ReceiverImage of {entity} was updated.");
            video_texture_output.set_changed();

            commands.trigger(SetState::<VideoPlayer> {
                entity: *container_entity,
                state: VideoState::VsPlaying,
                _phantom: PhantomData,
            });
        }
    }
}

fn transmission_stopped(
    mut commands: Commands,
    av_players: Query<&ContainerEntity, With<ReceiverImage>>,
    mut transmission_stopped: EventReader<TransmissionStopped>,
) {
    if transmission_stopped.read().count() > 0 {
        for container_entity in av_players {
            commands.trigger(SetState::<VideoPlayer> {
                entity: *container_entity,
                state: VideoState::VsLoading,
                _phantom: PhantomData,
            });
        }
    }
}

#[derive(Event)]
struct SetState<T: AVPlayer> {
    entity: ContainerEntity,
    state: VideoState,
    _phantom: PhantomData<T>,
}

fn set_state<T: AVPlayer>(
    trigger: Trigger<SetState<T>>,
    mut renderer_context: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
    let SetState {
        entity: container_entity,
        state,
        _phantom,
    } = trigger.event();

    let Ok(mut context) = renderer_context.get_mut(container_entity.root) else {
        return;
    };
    let tick_number = context.tick_number;

    if T::has_video() {
        context.update_crdt(
            SceneComponentId::VIDEO_EVENT,
            CrdtType::GO_ANY,
            container_entity.container_id,
            &PbVideoEvent {
                timestamp: frame.0,
                tick_number,
                current_offset: 0.,
                video_length: 0.,
                state: *state as i32,
            },
        );
    } else {
        context.update_crdt(
            SceneComponentId::AUDIO_EVENT,
            CrdtType::GO_ANY,
            container_entity.container_id,
            &PbAudioEvent {
                timestamp: frame.0,
                state: *state as i32,
            },
        );
    }
}
#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::*;

    use super::*;

    fn base_world() -> World {
        let mut world = World::new();

        world.insert_resource(AppConfig {
            max_videos: 1,
            ..Default::default()
        });

        world.spawn((
            Transform::from_translation(Vec3::ZERO),
            PrimaryUser::default(),
        ));

        world
    }

    #[test]
    fn none_initialy_with_should_be_playing() {
        let mut world = base_world();

        let screen_1 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                Transform::from_translation(Vec3::new(1., 0., 0.)),
            ))
            .id();
        let screen_2 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                Transform::from_translation(Vec3::new(0., 1., 0.)),
            ))
            .id();
        let screen_3 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                Transform::from_translation(Vec3::new(2., 0., 0.)),
            ))
            .id();

        world
            .run_system_cached(video_player_should_be_playing)
            .unwrap();

        assert!(world
            .entity(screen_1)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(!world
            .entity(screen_2)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(!world
            .entity(screen_3)
            .contains::<ShouldBePlaying<VideoPlayer>>());
    }

    #[test]
    fn closest_initialy_with_should_be_playing() {
        let mut world = base_world();

        let screen_1 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                ShouldBePlaying::<VideoPlayer>::default(),
                Transform::from_translation(Vec3::new(1., 0., 0.)),
            ))
            .id();
        let screen_2 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                Transform::from_translation(Vec3::new(0., 1., 0.)),
            ))
            .id();
        let screen_3 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                Transform::from_translation(Vec3::new(2., 0., 0.)),
            ))
            .id();

        world
            .run_system_cached(video_player_should_be_playing)
            .unwrap();

        assert!(world
            .entity(screen_1)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(!world
            .entity(screen_2)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(!world
            .entity(screen_3)
            .contains::<ShouldBePlaying<VideoPlayer>>());
    }

    #[test]
    fn ordering_shouldnt_change() {
        let mut world = base_world();

        let screen_1 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                ShouldBePlaying::<VideoPlayer>::default(),
                Transform::from_translation(Vec3::new(1., 0., 0.)),
            ))
            .id();
        let screen_2 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                Transform::from_translation(Vec3::new(0., 1., 0.)),
            ))
            .id();
        let screen_3 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                Transform::from_translation(Vec3::new(2., 0., 0.)),
            ))
            .id();

        for _ in 0..1000 {
            world
                .run_system_cached(video_player_should_be_playing)
                .unwrap();

            assert!(world
                .entity(screen_1)
                .contains::<ShouldBePlaying<VideoPlayer>>());
            assert!(!world
                .entity(screen_2)
                .contains::<ShouldBePlaying<VideoPlayer>>());
            assert!(!world
                .entity(screen_3)
                .contains::<ShouldBePlaying<VideoPlayer>>());
        }
    }

    #[test]
    fn closest_paused() {
        let mut world = base_world();

        let screen_1 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: false,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                ShouldBePlaying::<VideoPlayer>::default(),
                Transform::from_translation(Vec3::new(1., 0., 0.)),
            ))
            .id();
        let screen_2 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                InScene,
                Transform::from_translation(Vec3::new(0., 1., 0.)),
            ))
            .id();
        let screen_3 = world
            .spawn((
                VideoPlayerSource("https://example.org".to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                Transform::from_translation(Vec3::new(2., 0., 0.)),
            ))
            .id();

        world
            .run_system_cached(video_player_should_be_playing)
            .unwrap();

        assert!(!world
            .entity(screen_1)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(world
            .entity(screen_2)
            .contains::<ShouldBePlaying<VideoPlayer>>());
        assert!(!world
            .entity(screen_3)
            .contains::<ShouldBePlaying<VideoPlayer>>());
    }

    #[test]
    fn stream_out_of_scene() {
        let mut world = base_world();

        let screen_1 = world
            .spawn((
                VideoPlayerSource(LIVEKIT_VIDEO_STREAM.to_owned()),
                VideoPlayerConfig {
                    playing: true,
                    volume: 1.,
                    playback_rate: 1.,
                    r#loop: false,
                },
                ShouldBePlaying::<VideoPlayer>::default(),
                Transform::from_translation(Vec3::new(1., 0., 0.)),
            ))
            .id();

        world
            .run_system_cached(video_player_should_be_playing)
            .unwrap();

        assert!(!world
            .entity(screen_1)
            .contains::<ShouldBePlaying<VideoPlayer>>());
    }

    #[test]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn test_candidate_ordering() {
        let mut set = BTreeSet::new();

        let entity_1 = ShouldBePlayingCandidate {
            in_scene: true,
            has_should_be_playing: true,
            distance_to_player: FloatOrd(1.),
            entity: Entity::from_bits(0x0000000100000001),
        };
        let entity_2 = ShouldBePlayingCandidate {
            in_scene: false,
            has_should_be_playing: false,
            distance_to_player: FloatOrd(2.),
            entity: Entity::from_bits(0x0000000100000002),
        };
        let entity_3 = ShouldBePlayingCandidate {
            in_scene: true,
            has_should_be_playing: true,
            distance_to_player: FloatOrd(3.),
            entity: Entity::from_bits(0x0000000100000003),
        };
        let entity_4 = ShouldBePlayingCandidate {
            in_scene: false,
            has_should_be_playing: false,
            distance_to_player: FloatOrd(4.),
            entity: Entity::from_bits(0x0000000100000004),
        };
        let entity_5 = ShouldBePlayingCandidate {
            in_scene: false,
            has_should_be_playing: false,
            distance_to_player: FloatOrd(4.),
            entity: Entity::from_bits(0x0000000100000005),
        };

        set.insert(entity_1);
        set.insert(entity_2);
        set.insert(entity_3);
        set.insert(entity_4);
        set.insert(entity_5);

        for (candidate, expected) in set
            .into_iter()
            .zip([entity_1, entity_3, entity_2, entity_4, entity_5])
        {
            assert_eq!(candidate, expected);
        }
    }

    #[test]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn test_equidistant_candidates() {
        let mut set = BTreeSet::new();

        let entity_1 = ShouldBePlayingCandidate {
            in_scene: true,
            has_should_be_playing: true,
            distance_to_player: FloatOrd(1.),
            entity: Entity::from_bits(0x0000000100000001),
        };
        let entity_2 = ShouldBePlayingCandidate {
            in_scene: true,
            has_should_be_playing: true,
            distance_to_player: FloatOrd(1.),
            entity: Entity::from_bits(0x0000000100000002),
        };
        let entity_3 = ShouldBePlayingCandidate {
            in_scene: false,
            has_should_be_playing: false,
            distance_to_player: FloatOrd(1.),
            entity: Entity::from_bits(0x0000000100000003),
        };

        set.insert(entity_1);
        set.insert(entity_2);
        set.insert(entity_3);

        assert_eq!(set.get(&entity_1).unwrap(), &entity_1);
        assert_eq!(set.get(&entity_2).unwrap(), &entity_2);
        assert_eq!(set.get(&entity_3).unwrap(), &entity_3);
    }
}
