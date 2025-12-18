pub(super) mod plugin;

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    prelude::*,
};
use common::structs::AudioDecoderError;
use kira::sound::streaming::StreamingSoundData;
use livekit::prelude::{Participant, RemoteTrackPublication};
use tokio::sync::{mpsc, oneshot};
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;

use crate::{
    livekit::{livekit_video_bridge::LivekitVideoFrame, LivekitRuntime},
    make_hooks,
};

#[derive(Clone, Component, Deref, DerefMut)]
pub struct LivekitTrack {
    track: RemoteTrackPublication,
}

#[derive(Component)]
#[component(on_replace=Self::on_replace)]
struct LivekitTrackTask(JoinHandle<()>);

impl LivekitTrackTask {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<LivekitTrackTask>()
            .expect("LivekitTrackTask must be valid inside its own hook.");
        task.0.abort();
    }
}

#[derive(Component)]
#[relationship(relationship_target=Publishing)]
pub struct PublishedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship=PublishedBy, linked_spawn)]
pub struct Publishing(Vec<Entity>);

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Subscribed;
make_hooks!(Subscribed, (Unsubscribed, Subscribing, Unsubscribing));

#[derive(Component)]
#[component(on_add=Self::on_add)]
pub struct Unsubscribed;
make_hooks!(Unsubscribed, (Subscribed, Subscribing, Unsubscribing));

#[derive(Component)]
#[component(on_add=Self::on_add, on_replace=Self::on_replace)]
pub struct Subscribing {
    #[cfg(not(target_arch = "wasm32"))]
    task: JoinHandle<()>,
}
make_hooks!(Subscribing, (Subscribed, Unsubscribed, Unsubscribing));

impl Subscribing {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<Subscribing>()
            .expect("Subscribing must be valid inside its own hook.");
        task.task.abort();
    }
}

#[derive(Component)]
#[component(on_add=Self::on_add, on_replace=Self::on_replace)]
pub struct Unsubscribing {
    #[cfg(not(target_arch = "wasm32"))]
    task: JoinHandle<()>,
}
make_hooks!(Unsubscribing, (Subscribed, Unsubscribed, Subscribing));

impl Unsubscribing {
    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        let mut entity_mut = deferred_world.entity_mut(entity);
        let task = entity_mut
            .get_mut::<Unsubscribing>()
            .expect("Unsubscribing must be valid inside its own hook.");
        task.task.abort();
    }
}

#[derive(Component)]
struct OpenAudioSender {
    runtime: LivekitRuntime,
    sender: oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
}

#[derive(Component)]
struct OpenVideoSender {
    runtime: LivekitRuntime,
    sender: mpsc::Sender<LivekitVideoFrame>,
}

#[derive(Component)]
pub struct Audio;

#[derive(Component)]
pub struct Video;

#[derive(Component)]
pub struct Microphone;

#[derive(Component)]
pub struct Camera;

#[derive(Component)]
pub struct LivekitFrame {
    pub handle: Handle<Image>,
}

#[derive(Event)]
pub struct TrackPublished {
    pub participant: Participant,
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackUnpublished {
    pub participant: Participant,
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackSubscribed {
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct TrackUnsubscribed {
    pub track: RemoteTrackPublication,
}

#[derive(Event)]
pub struct SubscribeToAudioTrack {
    pub runtime: LivekitRuntime,
    pub sender: oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
}

#[derive(Event)]
pub struct SubscribeToVideoTrack {
    pub runtime: LivekitRuntime,
    pub sender: mpsc::Sender<LivekitVideoFrame>,
}

#[derive(Event)]
pub struct UnsubscribeToTrack {
    pub runtime: LivekitRuntime,
}
