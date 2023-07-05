use bevy::{
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use common::{sets::SceneSets, util::TryInsertEx};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbVideoPlayer, SceneComponentId};
use scene_runner::update_world::{AddCrdtInterfaceExt, material::VideoTextureOutput};

use crate::video_thread::{VideoCommand, VideoData, VideoInfo, VideoSink};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbVideoPlayer, VideoPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );
        app.add_startup_system(init_ffmpeg);
        app.add_system(play_videos);
        app.add_system(update_video_players.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct VideoPlayer(pub PbVideoPlayer);

impl From<PbVideoPlayer> for VideoPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self(value)
    }
}

fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
}

fn play_videos(mut images: ResMut<Assets<Image>>, mut q: Query<&mut VideoSink>) {
    for mut sink in q.iter_mut() {
        match sink.data_receiver.try_recv() {
            Ok(VideoData::Info(VideoInfo {
                width,
                height,
                rate,
                length,
            })) => {
                println!("resize");
                images.get_mut(&sink.image).unwrap().resize(Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                });
                sink.length = Some(length);
                sink.rate = Some(rate);
            }
            Ok(VideoData::Frame(frame, time)) => {
                println!("set frame on {:?}", sink.image);
                images
                    .get_mut(&sink.image)
                    .unwrap()
                    .data
                    .copy_from_slice(frame.data(0));
                sink.current_time = time;
            }
            Err(_) => (),
        }
    }
}

pub fn update_video_players(
    mut commands: Commands,
    video_players: Query<(Entity, &VideoPlayer, Option<&VideoSink>), Changed<VideoPlayer>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (ent, player, maybe_sink) in video_players.iter() {
        if maybe_sink.map(|sink| &sink.source) != Some(&player.0.src) {
            let mut image = Image::new_fill(
                bevy::render::render_resource::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &Color::PINK.as_rgba_u32().to_le_bytes(),
                TextureFormat::Rgba8UnormSrgb,
            );
            image.texture_descriptor.usage =
                TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;
            let image_handle = images.add(image);
            let video_sink = VideoSink::new(
                player.0.src.clone(),
                image_handle,
                player.0.playing.unwrap_or(true),
                player.0.r#loop.unwrap_or(false),
            );
            let video_output = VideoTextureOutput(video_sink.image.clone());
            commands.entity(ent).try_insert((video_sink,video_output));
        } else {
            let sink = maybe_sink.as_ref().unwrap();
            if player.0.playing.unwrap_or(true) {
                let _ = sink.command_sender.blocking_send(VideoCommand::Play);
            } else {
                let _ = sink.command_sender.blocking_send(VideoCommand::Pause);
            }
            let _ = sink
                .command_sender
                .blocking_send(VideoCommand::Repeat(player.0.r#loop.unwrap_or(false)));
        }
    }
}
