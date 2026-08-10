pub mod plugin;
pub mod states;

use bevy::prelude::*;

/// Marker component for an entity streaming a presentation
#[derive(Component)]
pub struct Presentation;

/// Marker component for an entity screensharing
#[derive(Component)]
pub struct Screenshare;

/// Marker component for an entity with a cast camera stream
#[derive(Component)]
pub struct VideoCast;

/// Marker component for an entity with a stream from a source
/// that is not a cast. Stream entities can only be active
/// if there is no cast active.
#[derive(Component)]
pub struct VideoStream;

/// Component with the handle to the image that the transmitter
/// should write to
#[derive(Component, Deref)]
pub struct ActiveTransmitter(Handle<Image>);

/// An active receiver entity
#[derive(Component)]
pub struct ActiveReceiver;

/// Component with the handle to the image that the receiver
/// should read from
#[derive(Component, Deref)]
pub struct ReceiverImage(Handle<Image>);
