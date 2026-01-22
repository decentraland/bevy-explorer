use bevy::{
    color::palettes,
    ecs::relationship::Relationship,
    prelude::*,
    text::{FontSmoothing, LineHeight},
};

use crate::{
    livekit::{
        participant::{ConnectionQuality, HostedBy, LivekitParticipant},
        room::LivekitRoom,
    },
    SceneRoom,
};

const DEFAULT_FONT: TextFont = TextFont {
    font: Handle::Weak(AssetId::Uuid {
        uuid: AssetId::<Font>::DEFAULT_UUID,
    }),
    font_size: 12.,
    line_height: LineHeight::RelativeToFont(1.2),
    font_smoothing: FontSmoothing::AntiAliased,
};

/// Overlay to show connected rooms
pub struct RoomDebugPlugin;

impl Plugin for RoomDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomDebugOverlayDefaultFont>();

        app.add_systems(Startup, setup);

        app.add_observer(room_connected);
        app.add_observer(room_disconnected);
        app.add_observer(connection_quality_changed);
    }
}

#[derive(Resource, Deref)]
struct RoomDebugOverlayDefaultFont(TextFont);

impl Default for RoomDebugOverlayDefaultFont {
    fn default() -> Self {
        Self(DEFAULT_FONT)
    }
}

#[derive(Component)]
struct RoomDebugOverlay;

#[derive(Component)]
struct RoomContainer;

#[derive(Component)]
struct SceneRoomContainer;

#[derive(Component, Deref)]
struct RoomRef(Entity);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Node {
            width: Val::Percent(100.),
            flex_direction: FlexDirection::RowReverse,
            ..Default::default()
        },
        RoomDebugOverlay,
        BackgroundColor(Color::BLACK.with_alpha(0.5)),
        GlobalZIndex(-1_000_000),
        children![
            (
                Node {
                    flex_direction: FlexDirection::RowReverse,
                    ..Default::default()
                },
                RoomContainer
            ),
            (
                Node {
                    flex_direction: FlexDirection::RowReverse,
                    ..Default::default()
                },
                SceneRoomContainer
            )
        ],
    ));

    commands.insert_resource(RoomDebugOverlayDefaultFont(
        DEFAULT_FONT.with_font(asset_server.load("embedded://fonts/NotoSans-Regular.ttf")),
    ));
}

fn room_connected(
    trigger: Trigger<OnAdd, LivekitRoom>,
    mut commands: Commands,
    rooms: Query<(&LivekitRoom, Has<SceneRoom>)>,
    room_container: Single<Entity, With<RoomContainer>>,
    scene_room_container: Single<Entity, With<SceneRoomContainer>>,
    default_font: Res<RoomDebugOverlayDefaultFont>,
) {
    let entity = trigger.target();
    let Ok((livekit_room, has_scene_room)) = rooms.get(entity) else {
        unreachable!("LivekitRoom must be available.");
    };

    let container = if has_scene_room {
        *scene_room_container
    } else {
        *room_container
    };

    commands.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            padding: UiRect::all(Val::Px(4.)),
            border: UiRect::left(Val::Px(2.)),
            column_gap: Val::Px(4.),
            ..Default::default()
        },
        BorderColor {
            left: Color::WHITE.with_alpha(0.5),
            ..Default::default()
        },
        RoomRef(entity),
        ChildOf(container),
        children![(Text::new(livekit_room.name()), default_font.clone())],
    ));
}

fn room_disconnected(
    trigger: Trigger<OnRemove, LivekitRoom>,
    mut commands: Commands,
    room_refs: Query<(Entity, &RoomRef)>,
) {
    let entity = trigger.target();
    let Some((room_ref_entity, _)) = room_refs.iter().find(|(_, room_ref)| ***room_ref == entity)
    else {
        unreachable!("Room must have a overlay referencing it.");
    };

    commands.entity(room_ref_entity).despawn();
}

fn connection_quality_changed(
    trigger: Trigger<OnInsert, ConnectionQuality>,
    mut commands: Commands,
    room_refs: Query<(&RoomRef, &Children)>,
    participants: Query<(&ConnectionQuality, &HostedBy), With<LivekitParticipant>>,
) {
    let entity = trigger.target();
    let Ok((connection_quality, hosted_by)) = participants.get(entity) else {
        unreachable!("Participant was not being hosted by a room.");
    };

    let Some((_, children)) = room_refs
        .iter()
        .find(|(room_ref, _)| ***room_ref == hosted_by.get())
    else {
        unreachable!("Room must have a overlay referencing it.");
    };

    let color = match connection_quality {
        ConnectionQuality::Excellent => palettes::tailwind::GREEN_500,
        ConnectionQuality::Good => palettes::tailwind::YELLOW_500,
        ConnectionQuality::Poor => palettes::tailwind::ORANGE_500,
        ConnectionQuality::Lost => palettes::tailwind::RED_500,
    };
    match children.len() {
        1 => {
            commands.entity(children[0]).insert(TextColor(color.into()));
        }
        _ => unreachable!("Invalid number of children."),
    }
}
