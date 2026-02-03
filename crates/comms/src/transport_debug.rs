//! Debug overlay to show information about [`Transport`]s

#[cfg(feature = "livekit")]
use crate::livekit::room::LivekitRoom;
use bevy::{
    color::palettes,
    ecs::relationship::RelatedSpawnerCommands,
    picking::Pickable,
    prelude::*,
    text::{FontSmoothing, LineHeight},
};

use crate::{SceneRoom, Transport};

const DEFAULT_FONT: TextFont = TextFont {
    font: Handle::Weak(AssetId::Uuid {
        uuid: AssetId::<Font>::DEFAULT_UUID,
    }),
    font_size: 12.,
    line_height: LineHeight::RelativeToFont(1.2),
    font_smoothing: FontSmoothing::AntiAliased,
};

pub struct TransportDebugPlugin;

impl Plugin for TransportDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_av_player_debug_ui);
        app.add_systems(Update, check_button_press);
        app.add_observer(transport_on_add);
        app.add_observer(transport_on_remove);
        app.add_observer(transport_on_insert);
        app.add_observer(on_add_column::<SceneRoom, SceneRoomColumn>);
        app.add_observer(on_remove_column::<SceneRoom, SceneRoomColumn>);
        #[cfg(feature = "livekit")]
        {
            app.add_observer(on_add_column::<LivekitRoom, LivekitRoomColumn>);
            app.add_observer(on_remove_column::<LivekitRoom, LivekitRoomColumn>);
        }
    }
}

#[derive(Component)]
struct TransportDebugUi;

#[derive(Debug, Component)]
struct TransportRef(Entity);

#[derive(Component)]
struct TransportColumn;
const TRANSPORT_COLUMN_COLUMN: i16 = 1;

#[derive(Component)]
struct SceneRoomColumn;
const SCENE_ROOM_COLUMN_COLUMN: i16 = 2;

#[cfg(feature = "livekit")]
#[derive(Component)]
struct LivekitRoomColumn;
#[cfg(feature = "livekit")]
const LIVEKIT_ROOM_COLUMN_COLUMN: i16 = 3;

// #[cfg(feature = "ffmpeg")]
// #[derive(Component)]
// struct VideoSinkColumn;
// #[cfg(feature = "ffmpeg")]
// const VIDEO_SINK_COLUMN_COLUMN: i16 = 3;

// #[cfg(feature = "livekit")]
// #[derive(Component)]
// struct StreamViewerColumn;
// #[cfg(feature = "livekit")]
// const STREAM_VIEWER_COLUMN_COLUMN: i16 = 4;

// #[cfg(all(feature = "livekit", not(target_arch = "wasm32")))]
// #[derive(Component)]
// struct StreamImageColumn;
// #[cfg(all(feature = "livekit", not(target_arch = "wasm32")))]
// const STREAM_IMAGE_COLUMN_COLUMN: i16 = 5;

// #[derive(Component)]
// struct InSceneColumn;
// const IN_SCENE_COLUMN_COLUMN: i16 = 6;

// #[derive(Component)]
// struct ShouldPlayColumn;
// const SHOULD_PLAY_COLUMN_COLUMN: i16 = 7;

#[cfg(not(feature = "livekit"))]
type AnyColumn = Or<(With<TransportColumn>, With<SceneRoomColumn>)>;
#[cfg(feature = "livekit")]
type AnyColumn = Or<(
    With<TransportColumn>,
    With<SceneRoomColumn>,
    With<LivekitRoomColumn>,
)>;

fn setup_av_player_debug_ui(mut commands: Commands) {
    commands
        .spawn((
            TransportDebugUi,
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
                    "Transport",
                    "SceneRoom",
                    #[cfg(feature = "livekit")]
                    "LivekitRoom",
                ),
            );
        });
}

fn check_button_press(
    mut commands: Commands,
    buttons: Res<ButtonInput<KeyCode>>,
    av_players_debug_ui: Single<Entity, With<TransportDebugUi>>,
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

fn transport_on_add(
    trigger: Trigger<OnAdd, Transport>,
    mut commands: Commands,
    transport_debug_ui: Single<(Entity, &Children), With<TransportDebugUi>>,
    transport_columns: Query<&Node, With<TransportColumn>>,
    transports: Query<(&Transport, Has<SceneRoom>)>,
) {
    let entity = trigger.target();
    let (transport_debug_ui_entity, children) = transport_debug_ui.into_inner();
    let (transport, has_scene_room) = transports.get(entity).unwrap();

    let next_row = transport_columns
        .iter_many(children.collection())
        .map(|node| node.grid_row.get_start().unwrap())
        .max()
        .map(|i| i + 1)
        .unwrap();

    commands
        .entity(transport_debug_ui_entity)
        .with_children(|parent| {
            build_row(
                parent,
                next_row,
                entity,
                (
                    &format!("{:?}", transport.transport_type),
                    if has_scene_room { "Yes" } else { "No" },
                    #[cfg(feature = "livekit")]
                    "No",
                ),
            );
        });
}

fn transport_on_remove(
    trigger: Trigger<OnRemove, Transport>,
    mut commands: Commands,
    transport_debug_ui: Single<&Children, With<TransportDebugUi>>,
    nodes: Query<&mut Node, AnyColumn>,
    transport_refs: Query<(Entity, &TransportRef), AnyColumn>,
) {
    let entity = trigger.target();
    let children = transport_debug_ui.into_inner();
    // Get row number of the deleted
    let deleted_row = children
        .iter()
        .find_map(|child| {
            let node = nodes.get(child).unwrap();
            let (_, transport_ref) = transport_refs.get(child).unwrap();
            if transport_ref.0 == entity {
                node.grid_row.get_start()
            } else {
                None
            }
        })
        .unwrap();
    // Despawn nodes that reference entity that had
    for transport_ref_entity in children.iter().filter_map(|child| {
        let (node_entity, transport_ref) = transport_refs.get(child).unwrap();
        if transport_ref.0 == entity {
            Some(node_entity)
        } else {
            None
        }
    }) {
        commands.entity(transport_ref_entity).try_despawn();
    }
    // // Shift remaining nodes
    for node in nodes {
        let old_start = node.grid_row.get_start().unwrap();
        if old_start > deleted_row {
            node.grid_row.set_start(old_start - 1);
        }
    }
}

fn transport_on_insert(
    trigger: Trigger<OnInsert, Transport>,
    mut commands: Commands,
    transports: Query<&Transport>,
    transport_references: Query<(Entity, &TransportRef), With<TransportColumn>>,
) {
    let entity = trigger.target();
    let transport = transports.get(entity).unwrap();

    let Some(node) =
        transport_references
            .iter()
            .find_map(|(transport_ref_entity, transport_ref)| {
                if transport_ref.0 == entity {
                    Some(transport_ref_entity)
                } else {
                    None
                }
            })
    else {
        return;
    };

    commands.entity(node).despawn_related::<Children>();
    let transport_name = format!("{:?}", transport.transport_type);
    commands.spawn((
        Text::new(transport_name),
        DEFAULT_FONT,
        Pickable::IGNORE,
        ChildOf(node),
    ));
}

fn on_add_column<T: Component, C: Component>(
    trigger: Trigger<OnAdd, T>,
    mut commands: Commands,
    transport_references: Query<(Entity, &TransportRef), With<C>>,
) {
    let entity = trigger.target();

    let Some(node) =
        transport_references
            .iter()
            .find_map(|(transport_ref_entity, transport_ref)| {
                if transport_ref.0 == entity {
                    Some(transport_ref_entity)
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
    transport_references: Query<(Entity, &TransportRef), With<C>>,
    children: Query<&Children>,
) {
    let entity = trigger.target();

    let Some(node) =
        transport_references
            .iter()
            .find_map(|(transport_ref_entity, transport_ref)| {
                if transport_ref.0 == entity {
                    Some(transport_ref_entity)
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

#[cfg(not(feature = "livekit"))]
type RowTexts<'a> = (&'a str, &'a str);
#[cfg(feature = "livekit")]
type RowTexts<'a> = (&'a str, &'a str, &'a str);

fn build_row(
    parent: &mut RelatedSpawnerCommands<'_, ChildOf>,
    row: i16,
    transport: Entity,
    row_texts: RowTexts,
) {
    #[cfg(not(feature = "livekit"))]
    let (transport_name, scene_room) = row_texts;
    #[cfg(feature = "livekit")]
    let (transport_name, scene_room, livekit_room) = row_texts;

    let transport_name = if transport_name.len() >= 32 {
        &transport_name[..32]
    } else {
        transport_name
    };

    parent.spawn(build_cel(
        transport,
        TransportColumn,
        row,
        TRANSPORT_COLUMN_COLUMN,
        transport_name,
    ));
    parent.spawn(build_cel(
        transport,
        SceneRoomColumn,
        row,
        SCENE_ROOM_COLUMN_COLUMN,
        scene_room,
    ));
    #[cfg(feature = "livekit")]
    parent.spawn(build_cel(
        transport,
        LivekitRoomColumn,
        row,
        LIVEKIT_ROOM_COLUMN_COLUMN,
        livekit_room,
    ));
}

fn build_cel<C: Component>(
    transport: Entity,
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
        TransportRef(transport),
        Pickable::IGNORE,
    )
}
