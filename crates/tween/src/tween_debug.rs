use std::fmt::Debug;

use bevy::{
    prelude::*,
    text::{FontSmoothing, LineHeight},
};
use common::structs::PrimaryCamera;
use dcl_component::proto_components::sdk::components::pb_tween::Mode;

use crate::Tween;

const DEFAULT_FONT: TextFont = TextFont {
    font: Handle::Weak(AssetId::Uuid {
        uuid: AssetId::<Font>::DEFAULT_UUID,
    }),
    font_size: 8.,
    line_height: LineHeight::RelativeToFont(1.2),
    font_smoothing: FontSmoothing::AntiAliased,
};

pub struct TweenDebugPlugin;

impl Plugin for TweenDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);

        app.add_observer(new_tween);
        app.add_observer(tween_dropped);

        app.add_systems(
            PostUpdate,
            update_plate_position.after(TransformSystem::TransformPropagate),
        );
    }
}

#[derive(Component)]
struct TweenDebugPlane;

#[derive(Component)]
#[relationship(relationship_target = TweenedEntity)]
struct TweenDebugPlate(Entity);

#[derive(Component)]
#[relationship_target(relationship = TweenDebugPlate )]
struct TweenedEntity(Entity);

fn setup(mut commands: Commands) {
    commands.spawn((
        TweenDebugPlane,
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..Default::default()
        },
        GlobalZIndex(1_000_000),
    ));
}

fn new_tween(
    trigger: Trigger<OnInsert, Tween>,
    mut commands: Commands,
    tweens: Query<&Tween>,
    tween_debug_plane: Single<Entity, With<TweenDebugPlane>>,
) {
    let entity = trigger.target();
    let Ok(tween) = tweens.get(entity) else {
        unreachable!("Tween must be available.");
    };

    let root = commands
        .spawn((
            Node {
                display: Display::Grid,
                position_type: PositionType::Absolute,
                column_gap: Val::Px(4.),
                ..Default::default()
            },
            ChildOf(*tween_debug_plane),
            TweenDebugPlate(entity),
            BackgroundColor(Color::BLACK.with_alpha(0.5)),
        ))
        .id();
    match &tween.0.mode {
        Some(Mode::Move(data)) => {
            plate_head(&mut commands, 1, root, "Move");
            plate_display_row(&mut commands, 2, root, "Duration", &tween.0.duration);
            plate_display_row(
                &mut commands,
                3,
                root,
                "Easing function",
                &tween.0.easing_function(),
            );
            plate_display_row(&mut commands, 4, root, "Start", &data.start);
            plate_display_row(&mut commands, 5, root, "End", &data.end);
            plate_display_row(
                &mut commands,
                6,
                root,
                "Face direction",
                &data.face_direction,
            );
            plate_display_row(&mut commands, 7, root, "Playing", &tween.0.playing);
            plate_display_row(
                &mut commands,
                8,
                root,
                "Current time",
                &tween.0.current_time,
            );
        }
        Some(Mode::Rotate(data)) => {
            plate_head(&mut commands, 1, root, "Rotate");
            plate_display_row(&mut commands, 2, root, "Duration", &tween.0.duration);
            plate_display_row(
                &mut commands,
                3,
                root,
                "Easing function",
                &tween.0.easing_function(),
            );
            plate_display_row(&mut commands, 4, root, "Start", &data.start);
            plate_display_row(&mut commands, 5, root, "End", &data.end);
            plate_display_row(&mut commands, 6, root, "Playing", &tween.0.playing);
            plate_display_row(
                &mut commands,
                7,
                root,
                "Current time",
                &tween.0.current_time,
            );
        }
        Some(Mode::Scale(data)) => {
            plate_head(&mut commands, 1, root, "Scale");
            plate_display_row(&mut commands, 2, root, "Duration", &tween.0.duration);
            plate_display_row(
                &mut commands,
                3,
                root,
                "Easing function",
                &tween.0.easing_function(),
            );
            plate_display_row(&mut commands, 4, root, "Start", &data.start);
            plate_display_row(&mut commands, 5, root, "End", &data.end);
            plate_display_row(&mut commands, 6, root, "Playing", &tween.0.playing);
            plate_display_row(
                &mut commands,
                7,
                root,
                "Current time",
                &tween.0.current_time,
            );
        }
        Some(Mode::TextureMove(data)) => {
            plate_head(&mut commands, 1, root, "TextureMove");
            plate_display_row(&mut commands, 2, root, "Duration", &tween.0.duration);
            plate_display_row(
                &mut commands,
                3,
                root,
                "Easing function",
                &tween.0.easing_function(),
            );
            plate_display_row(&mut commands, 4, root, "Start", &data.start);
            plate_display_row(&mut commands, 5, root, "End", &data.end);
            plate_display_row(
                &mut commands,
                6,
                root,
                "MovementType",
                &data.movement_type(),
            );
            plate_display_row(&mut commands, 7, root, "Playing", &tween.0.playing);
            plate_display_row(
                &mut commands,
                8,
                root,
                "Current time",
                &tween.0.current_time,
            );
        }
        _ => {}
    }
}

fn tween_dropped(
    trigger: Trigger<OnReplace, Tween>,
    mut commands: Commands,
    tweens: Query<&TweenedEntity>,
) {
    let entity = trigger.target();
    let Ok(tweened_entity) = tweens.get(entity) else {
        unreachable!("Tween entity without a plate.");
    };

    commands.entity(*tweened_entity.collection()).despawn();
}

fn update_plate_position(
    primary_camera: Single<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    tweens: Query<(&GlobalTransform, &TweenedEntity), With<Tween>>,
    mut tween_plates: Query<(&mut Node, &ComputedNode), With<TweenDebugPlate>>,
) {
    let (camera, camera_global_transform) = primary_camera.into_inner();
    for (global_transform, tweened_entity) in tweens {
        let Ok((mut node, computed_node)) = tween_plates.get_mut(*tweened_entity.collection())
        else {
            unreachable!("TweenDebugPlate without Node.");
        };
        if let Ok(viewport_position) =
            camera.world_to_viewport(camera_global_transform, global_transform.translation())
        {
            node.display = Display::Grid;
            node.left = Val::Px(viewport_position.x - computed_node.content_size.x / 2.);
            node.top = Val::Px(viewport_position.y - computed_node.content_size.y / 2.);
        } else {
            node.display = Display::None;
        }
    }
}

fn plate_head(commands: &mut Commands, row: i16, parent: Entity, text: &str) -> impl Bundle {
    commands.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(1),
            ..Default::default()
        },
        Text::new(text),
        DEFAULT_FONT,
        ChildOf(parent),
    ));
}

fn plate_display_row<T: Debug>(
    commands: &mut Commands,
    row: i16,
    parent: Entity,
    text: &str,
    data: &T,
) -> impl Bundle {
    commands.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(1),
            ..Default::default()
        },
        Text::new(text),
        DEFAULT_FONT,
        ChildOf(parent),
    ));
    commands.spawn((
        Node {
            grid_row: GridPlacement::start(row),
            grid_column: GridPlacement::start(2),
            ..Default::default()
        },
        Text::new(format!("{:?}", data)),
        DEFAULT_FONT,
        ChildOf(parent),
    ));
}
