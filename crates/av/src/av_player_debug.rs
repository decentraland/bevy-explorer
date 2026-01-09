//! Debug overlay to show information about [`AVPlayer`]s

use bevy::{
    color::palettes, ecs::relationship::RelatedSpawnerCommands, picking::Pickable, prelude::*,
};

#[cfg(feature = "ffmpeg")]
use crate::{audio_sink::AudioSink, video_stream::VideoSink};
use crate::{AVPlayer, InScene, ShouldBePlaying};
#[cfg(feature = "livekit")]
use comms::livekit::StreamImage;

pub struct AvPlayerDebugPlugin;

impl Plugin for AvPlayerDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_av_player_debug_ui);
        app.add_systems(Update, check_button_press);
        app.add_observer(av_player_on_add);
        app.add_observer(av_player_on_remove);
        app.add_observer(av_player_on_insert);
        #[cfg(feature = "ffmpeg")]
        {
            app.add_observer(on_add_column::<AudioSink, AudioSinkColumn>);
            app.add_observer(on_remove_column::<AudioSink, AudioSinkColumn>);
            app.add_observer(on_add_column::<VideoSink, VideoSinkColumn>);
            app.add_observer(on_remove_column::<VideoSink, VideoSinkColumn>);
        }
        #[cfg(feature = "livekit")]
        {
            app.add_observer(on_add_column::<StreamImage, StreamerColumn>);
            app.add_observer(on_remove_column::<StreamImage, StreamerColumn>);
        }
        app.add_observer(on_add_column::<InScene, InSceneColumn>);
        app.add_observer(on_remove_column::<InScene, InSceneColumn>);
        app.add_observer(on_add_column::<ShouldBePlaying, ShouldPlayColumn>);
        app.add_observer(on_remove_column::<ShouldBePlaying, ShouldPlayColumn>);
    }
}

#[derive(Component)]
struct AvPlayerDebugUi;

#[derive(Debug, Component)]
struct AvPlayerRef(Entity);

#[derive(Component)]
struct AvPlayerColumn;
const AV_PLAYER_COLUMN_COLUMN: i16 = 1;

#[cfg(feature = "ffmpeg")]
#[derive(Component)]
struct AudioSinkColumn;
#[cfg(feature = "ffmpeg")]
const AUDIO_SINK_COLUMN_COLUMN: i16 = 2;

#[cfg(feature = "ffmpeg")]
#[derive(Component)]
struct VideoSinkColumn;
#[cfg(feature = "ffmpeg")]
const VIDEO_SINK_COLUMN_COLUMN: i16 = 3;

#[cfg(feature = "livekit")]
#[derive(Component)]
struct StreamerColumn;
#[cfg(feature = "livekit")]
const STREAMER_COLUMN_COLUMN: i16 = 4;

#[derive(Component)]
struct InSceneColumn;
const IN_SCENE_COLUMN_COLUMN: i16 = 5;

#[derive(Component)]
struct ShouldPlayColumn;
const SHOULD_PLAY_COLUMN_COLUMN: i16 = 6;

#[cfg(all(not(feature = "ffmpeg"), not(feature = "livekit")))]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<InSceneColumn>,
    With<ShouldPlayColumn>,
)>;
#[cfg(all(feature = "ffmpeg", not(feature = "livekit")))]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<AudioSinkColumn>,
    With<VideoSinkColumn>,
    With<InSceneColumn>,
    With<ShouldPlayColumn>,
)>;
#[cfg(all(not(feature = "ffmpeg"), feature = "livekit"))]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<StreamerColumn>,
    With<InSceneColumn>,
    With<ShouldPlayColumn>,
)>;
#[cfg(all(feature = "ffmpeg", feature = "livekit"))]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<AudioSinkColumn>,
    With<VideoSinkColumn>,
    With<StreamerColumn>,
    With<InSceneColumn>,
    With<ShouldPlayColumn>,
)>;

fn setup_av_player_debug_ui(mut commands: Commands) {
    commands
        .spawn((
            AvPlayerDebugUi,
            Node {
                display: Display::Grid,
                min_width: Val::Px(200.),
                min_height: Val::Px(50.),
                margin: UiRect::all(Val::Px(20.)),
                padding: UiRect::all(Val::Px(8.)),
                row_gap: Val::Px(4.),
                column_gap: Val::Px(4.),
                ..Default::default()
            },
            BackgroundColor(Color::from(palettes::tailwind::GRAY_700).with_alpha(0.75)),
            GlobalZIndex(1000),
            Pickable::IGNORE,
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            build_row(
                parent,
                1,
                Entity::PLACEHOLDER,
                (
                    "Source",
                    #[cfg(feature = "ffmpeg")]
                    "AudioSink",
                    #[cfg(feature = "ffmpeg")]
                    "VideoSink",
                    #[cfg(feature = "livekit")]
                    "Streamer",
                    "InScene",
                    "ShouldBePlaying",
                ),
            );
        });
}

fn check_button_press(
    mut commands: Commands,
    buttons: Res<ButtonInput<KeyCode>>,
    av_players_debug_ui: Single<Entity, With<AvPlayerDebugUi>>,
    mut toggle: Local<bool>,
) {
    if buttons.just_pressed(KeyCode::F1) {
        *toggle = !*toggle;
        if *toggle {
            commands
                .entity(*av_players_debug_ui)
                .insert(Visibility::Inherited);
        } else {
            commands
                .entity(*av_players_debug_ui)
                .insert(Visibility::Hidden);
        }
    }
}

fn av_player_on_add(
    trigger: Trigger<OnAdd, AVPlayer>,
    mut commands: Commands,
    av_player_debug_ui: Single<(Entity, &Children), With<AvPlayerDebugUi>>,
    av_player_columns: Query<&Node, With<AvPlayerColumn>>,
    av_players: Query<&AVPlayer>,
) {
    let entity = trigger.target();
    let (av_player_debug_ui_entity, children) = av_player_debug_ui.into_inner();
    let av_player = av_players.get(entity).unwrap();

    let next_row = av_player_columns
        .iter_many(children.collection())
        .map(|node| node.grid_row.get_start().unwrap())
        .max()
        .map(|i| i + 1)
        .unwrap();

    commands
        .entity(av_player_debug_ui_entity)
        .with_children(|parent| {
            build_row(
                parent,
                next_row,
                entity,
                (
                    &av_player.source.src,
                    #[cfg(feature = "ffmpeg")]
                    "No",
                    #[cfg(feature = "ffmpeg")]
                    "No",
                    #[cfg(feature = "livekit")]
                    "No",
                    "No",
                    "No",
                ),
            );
        });
}

fn av_player_on_remove(
    trigger: Trigger<OnRemove, AVPlayer>,
    mut commands: Commands,
    av_player_debug_ui: Single<&Children, With<AvPlayerDebugUi>>,
    nodes: Query<&mut Node, AnyColumn>,
    av_player_refs: Query<(Entity, &AvPlayerRef), AnyColumn>,
) {
    let entity = trigger.target();
    let children = av_player_debug_ui.into_inner();
    // Get row number of the deleted
    let deleted_row = children
        .iter()
        .find_map(|child| {
            let node = nodes.get(child).unwrap();
            let (_, av_player_ref) = av_player_refs.get(child).unwrap();
            if av_player_ref.0 == entity {
                node.grid_row.get_start()
            } else {
                None
            }
        })
        .unwrap();
    // Despawn nodes that reference entity that had
    for av_player_ref_entity in children.iter().filter_map(|child| {
        let (node_entity, av_player_ref) = av_player_refs.get(child).unwrap();
        if av_player_ref.0 == entity {
            Some(node_entity)
        } else {
            None
        }
    }) {
        commands.entity(av_player_ref_entity).try_despawn();
    }
    // // Shift remaining nodes
    for node in nodes {
        let old_start = node.grid_row.get_start().unwrap();
        if old_start > deleted_row {
            node.grid_row.set_start(old_start - 1);
        }
    }
}

fn av_player_on_insert(
    trigger: Trigger<OnInsert, AVPlayer>,
    mut commands: Commands,
    av_players: Query<&AVPlayer>,
    av_player_references: Query<(Entity, &AvPlayerRef), With<AvPlayerColumn>>,
) {
    let entity = trigger.target();
    let av_player = av_players.get(entity).unwrap();

    let Some(node) =
        av_player_references
            .iter()
            .find_map(|(av_player_ref_entity, av_player_ref)| {
                if av_player_ref.0 == entity {
                    Some(av_player_ref_entity)
                } else {
                    None
                }
            })
    else {
        return;
    };

    commands.entity(node).despawn_related::<Children>();
    let av_player_name = if av_player.source.src.len() >= 32 {
        &av_player.source.src[..32]
    } else {
        &av_player.source.src
    };
    commands.spawn((Text::new(av_player_name), Pickable::IGNORE, ChildOf(node)));
}

fn on_add_column<T: Component, C: Component>(
    trigger: Trigger<OnAdd, T>,
    mut commands: Commands,
    av_player_references: Query<(Entity, &AvPlayerRef), With<C>>,
) {
    let entity = trigger.target();

    let Some(node) =
        av_player_references
            .iter()
            .find_map(|(av_player_ref_entity, av_player_ref)| {
                if av_player_ref.0 == entity {
                    Some(av_player_ref_entity)
                } else {
                    None
                }
            })
    else {
        return;
    };

    commands.entity(node).despawn_related::<Children>();
    commands.spawn((Text::new("Yes"), Pickable::IGNORE, ChildOf(node)));
}

fn on_remove_column<T: Component, C: Component>(
    trigger: Trigger<OnRemove, T>,
    mut commands: Commands,
    av_player_references: Query<(Entity, &AvPlayerRef), With<C>>,
    children: Query<&Children>,
) {
    let entity = trigger.target();

    let Some(node) =
        av_player_references
            .iter()
            .find_map(|(av_player_ref_entity, av_player_ref)| {
                if av_player_ref.0 == entity {
                    Some(av_player_ref_entity)
                } else {
                    None
                }
            })
    else {
        return;
    };

    // If the reason for the removal is despawn
    // using `despawn_related` will crash
    if let Ok(children) = children.get(node) {
        for child in children {
            commands.entity(*child).try_despawn();
        }
    }
    commands.spawn((Text::new("No"), Pickable::IGNORE, ChildOf(node)));
}

#[cfg(all(not(feature = "ffmpeg"), not(feature = "livekit")))]
type RowTexts<'a> = (&'a str, &'a str, &'a str);
#[cfg(all(feature = "ffmpeg", not(feature = "livekit")))]
type RowTexts<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str);
#[cfg(all(not(feature = "ffmpeg"), feature = "livekit"))]
type RowTexts<'a> = (&'a str, &'a str, &'a str, &'a str);
#[cfg(all(feature = "ffmpeg", feature = "livekit"))]
type RowTexts<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str, &'a str);

fn build_row(
    parent: &mut RelatedSpawnerCommands<'_, ChildOf>,
    row: i16,
    av_player: Entity,
    row_texts: RowTexts,
) {
    #[cfg(all(not(feature = "ffmpeg"), not(feature = "livekit")))]
    let (av_player_name, in_scene, should_play) = row_texts;
    #[cfg(all(feature = "ffmpeg", not(feature = "livekit")))]
    let (av_player_name, audio_sink, video_sink, in_scene, should_play) = row_texts;
    #[cfg(all(not(feature = "ffmpeg"), feature = "livekit"))]
    let (av_player_name, streamer, in_scene, should_play) = row_texts;
    #[cfg(all(feature = "ffmpeg", feature = "livekit"))]
    let (av_player_name, audio_sink, video_sink, streamer, in_scene, should_play) = row_texts;

    let av_player_name = if av_player_name.len() >= 32 {
        &av_player_name[..32]
    } else {
        av_player_name
    };
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(AV_PLAYER_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(av_player_name), Pickable::IGNORE)],
        AvPlayerColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
    #[cfg(feature = "ffmpeg")]
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(AUDIO_SINK_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(audio_sink), Pickable::IGNORE)],
        AudioSinkColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
    #[cfg(feature = "ffmpeg")]
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(VIDEO_SINK_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(video_sink), Pickable::IGNORE)],
        VideoSinkColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
    #[cfg(feature = "livekit")]
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(STREAMER_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(streamer), Pickable::IGNORE)],
        StreamerColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(IN_SCENE_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(in_scene), Pickable::IGNORE)],
        InSceneColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
    parent.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(SHOULD_PLAY_COLUMN_COLUMN),
            ..Default::default()
        },
        children![(Text::new(should_play), Pickable::IGNORE)],
        ShouldPlayColumn,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    ));
}
