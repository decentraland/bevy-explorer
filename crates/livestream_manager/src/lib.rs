pub mod plugin;

use bevy::prelude::*;

#[derive(Component)]
pub struct Presentation;

#[derive(Component)]
pub struct VideoCast;

#[derive(Component)]
pub struct VideoStream;

#[derive(Component, Deref)]
pub struct ActiveTransmiter(Handle<Image>);

#[derive(Component, Deref)]
pub struct ActiveReceiver(Handle<Image>);
