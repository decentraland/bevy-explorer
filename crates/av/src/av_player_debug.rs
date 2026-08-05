//! Debug overlay to show information about [`AVPlayer`]s

use bevy::{
    color::palettes,
    picking::Pickable,
    prelude::*,
    text::{FontSmoothing, LineHeight},
};
use common::util::{TryChildBuilder, TryPushChildrenEx};

#[cfg(feature = "ffmpeg")]
use crate::AVSinks;
use crate::{AVPlayer, InScene, ShouldBePlaying, VideoPlayer};

const DEFAULT_FONT: TextFont = TextFont {
    font: Handle::Weak(AssetId::Uuid {
        uuid: AssetId::<Font>::DEFAULT_UUID,
    }),
    font_size: 12.,
    line_height: LineHeight::RelativeToFont(1.2),
    font_smoothing: FontSmoothing::AntiAliased,
};

pub struct AvPlayerDebugPlugin;

impl Plugin for AvPlayerDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_av_player_debug_ui);
        app.add_systems(Update, check_button_press);
        app.add_observer(av_player_on_add::<VideoPlayer>);
        app.add_observer(av_player_on_remove::<VideoPlayer>);
        app.add_observer(av_player_on_insert::<VideoPlayer>);
        #[cfg(feature = "ffmpeg")]
        {
            app.add_observer(on_add_column::<AVSinks<VideoPlayer>, VideoPlayerSinksColumn>);
            app.add_observer(on_remove_column::<AVSinks<VideoPlayer>, VideoPlayerSinksColumn>);
        }
        app.add_observer(on_add_column::<InScene, InSceneColumn>);
        app.add_observer(on_remove_column::<InScene, InSceneColumn>);
        app.add_observer(on_add_column::<ShouldBePlaying<VideoPlayer>, ShouldPlayColumn>);
        app.add_observer(on_remove_column::<ShouldBePlaying<VideoPlayer>, ShouldPlayColumn>);
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
struct VideoPlayerSinksColumn;
#[cfg(feature = "ffmpeg")]
const VIDEO_PLAYER_SINKS_COLUMN_COLUMN: i16 = 2;

#[derive(Component)]
struct InSceneColumn;
const IN_SCENE_COLUMN_COLUMN: i16 = 5;

#[derive(Component)]
struct ShouldPlayColumn;
const SHOULD_PLAY_COLUMN_COLUMN: i16 = 6;

#[cfg(not(feature = "ffmpeg"))]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<InSceneColumn>,
    With<ShouldPlayColumn>,
)>;
#[cfg(feature = "ffmpeg")]
type AnyColumn = Or<(
    With<AvPlayerColumn>,
    With<VideoPlayerSinksColumn>,
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
        .try_with_children(|parent| {
            build_row(
                parent,
                1,
                Entity::PLACEHOLDER,
                (
                    "Source",
                    #[cfg(feature = "ffmpeg")]
                    "VideoPlayerSink",
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

fn av_player_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, T>,
    mut commands: Commands,
    av_player_debug_ui: Single<(Entity, &Children), With<AvPlayerDebugUi>>,
    av_player_columns: Query<&Node, With<AvPlayerColumn>>,
    av_players: Query<&T>,
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
        .try_with_children(|parent| {
            build_row(
                parent,
                next_row,
                entity,
                (
                    av_player.url(),
                    #[cfg(feature = "ffmpeg")]
                    "No",
                    "No",
                    "No",
                ),
            );
        });
}

fn av_player_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, T>,
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

fn av_player_on_insert<T: AVPlayer>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    av_players: Query<&T>,
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
    let source = av_player.url();
    let av_player_name = if source.len() >= 32 {
        &source[..32]
    } else {
        source
    };
    commands.spawn((
        Text::new(av_player_name),
        DEFAULT_FONT,
        Pickable::IGNORE,
        ChildOf(node),
    ));
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
    commands.spawn((
        Text::new("Yes"),
        DEFAULT_FONT,
        Pickable::IGNORE,
        ChildOf(node),
    ));
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
    commands.spawn((
        Text::new("No"),
        DEFAULT_FONT,
        Pickable::IGNORE,
        ChildOf(node),
    ));
}

#[cfg(not(feature = "ffmpeg"))]
type RowTexts<'a> = (&'a str, &'a str, &'a str);
#[cfg(feature = "ffmpeg")]
type RowTexts<'a> = (&'a str, &'a str, &'a str, &'a str);

fn build_row(parent: &mut TryChildBuilder, row: i16, av_player: Entity, row_texts: RowTexts) {
    #[cfg(not(feature = "ffmpeg"))]
    let (av_player_name, in_scene, should_play) = row_texts;
    #[cfg(feature = "ffmpeg")]
    let (av_player_name, video_player_sinks, in_scene, should_play) = row_texts;

    let av_player_name = if av_player_name.len() >= 32 {
        &av_player_name[..32]
    } else {
        av_player_name
    };

    parent.spawn(build_cel(
        av_player,
        AvPlayerColumn,
        row,
        AV_PLAYER_COLUMN_COLUMN,
        av_player_name,
    ));
    #[cfg(feature = "ffmpeg")]
    parent.spawn(build_cel(
        av_player,
        VideoPlayerSinksColumn,
        row,
        VIDEO_PLAYER_SINKS_COLUMN_COLUMN,
        video_player_sinks,
    ));
    parent.spawn(build_cel(
        av_player,
        InSceneColumn,
        row,
        IN_SCENE_COLUMN_COLUMN,
        in_scene,
    ));
    parent.spawn(build_cel(
        av_player,
        ShouldPlayColumn,
        row,
        SHOULD_PLAY_COLUMN_COLUMN,
        should_play,
    ));
}

fn build_cel<C: Component>(
    av_player: Entity,
    column_marker: C,
    row: i16,
    column: i16,
    text: &str,
) -> impl Bundle {
    (
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(column),
            ..Default::default()
        },
        children![(Text::new(text), DEFAULT_FONT, Pickable::IGNORE)],
        column_marker,
        AvPlayerRef(av_player),
        Pickable::IGNORE,
    )
}
