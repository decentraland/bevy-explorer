pub mod plugin;

use bevy::prelude::*;

#[derive(Component)]
pub struct Presentation;

#[derive(Component)]
pub struct Screenshare;

#[derive(Component)]
pub struct VideoCast;

#[derive(Component)]
pub struct VideoStream;

#[derive(Component, Deref)]
pub struct ActiveTransmitter(Handle<Image>);

#[derive(Component)]
pub struct ActiveReceiver;

#[derive(Component, Deref)]
pub struct ReceiverImage(Handle<Image>);
