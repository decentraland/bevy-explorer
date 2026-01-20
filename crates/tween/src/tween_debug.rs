use std::fmt::Debug;

use bevy::{
    color::palettes,
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

        if app.is_plugin_added::<MeshPickingPlugin>() {
            app.add_systems(
                PostUpdate,
                update_plate_position.after(TransformSystem::TransformPropagate),
            );

            app.add_observer(tween_picking);
            app.add_observer(tween_out);

            app.add_systems(
                PostUpdate,
                axis_gizmos.after(TransformSystem::TransformPropagate),
            );
        } else {
            error!("MeshPickingPlugin not added to the app. Tween picking systems are disabled.")
        }
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

fn update_plate_position(
    mut commands: Commands,
    primary_camera: Single<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    tweens: Query<(&GlobalTransform, &TweenedEntity), With<Tween>>,
    mut tween_plates: Query<(&mut Node, &ComputedNode), With<TweenDebugPlate>>,
) {
    let (camera, camera_global_transform) = primary_camera.into_inner();
    for (global_transform, tweened_entity) in tweens {
        let tween_debug_plate_entity = *tweened_entity.collection();
        let Ok((mut node, computed_node)) = tween_plates.get_mut(tween_debug_plate_entity) else {
            unreachable!("TweenDebugPlate without Node.");
        };
        if let Ok(viewport_position) = camera
            .world_to_viewport_with_depth(camera_global_transform, global_transform.translation())
        {
            node.display = Display::Grid;
            node.left = Val::Px(viewport_position.x - computed_node.content_size.x / 2.);
            node.top = Val::Px(viewport_position.y - computed_node.content_size.y / 2.);
            commands
                .entity(tween_debug_plate_entity)
                .insert(GlobalZIndex(-viewport_position.z as i32));
        } else {
            node.display = Display::None;
        }
    }
}
fn tween_picking(
    mut trigger: Trigger<Pointer<Over>>,
    mut commands: Commands,
    tweens: Query<(&Tween, Has<TweenDebugPlate>)>,
    tween_debug_plane: Single<Entity, With<TweenDebugPlane>>,
) {
    let entity = trigger.target();
    if let Ok((tween, has_tween_debug_plate)) = tweens.get(entity) {
        trigger.propagate(false);

        if has_tween_debug_plate {
            return;
        }

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
                BackgroundColor(Color::BLACK.with_alpha(0.75)),
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
            Some(Mode::RotateContinuous(data)) => {
                plate_head(&mut commands, 1, root, "Continuous rotation");
                plate_display_row(&mut commands, 2, root, "Duration", &tween.0.duration);
                plate_display_row(
                    &mut commands,
                    3,
                    root,
                    "Easing function",
                    &tween.0.easing_function(),
                );
                plate_display_row(&mut commands, 4, root, "Direction", &data.direction);
                plate_display_row(&mut commands, 5, root, "Speed", &data.speed);
                plate_display_row(&mut commands, 6, root, "Playing", &tween.0.playing);
                plate_display_row(
                    &mut commands,
                    7,
                    root,
                    "Current time",
                    &tween.0.current_time,
                );
            }
            _ => {}
        }
    }
}

fn tween_out(
    mut trigger: Trigger<Pointer<Out>>,
    mut commands: Commands,
    tweens: Query<&TweenedEntity>,
) {
    let entity = trigger.target();
    if let Ok(tweened_entity) = tweens.get(entity) {
        trigger.propagate(false);

        commands.entity(*tweened_entity.collection()).despawn();
    };
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

fn axis_gizmos(mut gizmos: Gizmos, tweens: Query<(&Tween, &GlobalTransform)>) {
    for (tween, global_transform) in tweens {
        match &tween.0.mode {
            #[cfg(not(feature = "alt_rotate_continuous"))]
            Some(Mode::RotateContinuous(data)) => {
                let direction = data.direction.unwrap().to_bevy_normalized();
                gizmos.axes(
                    Isometry3d::new(
                        global_transform.translation(),
                        Quat::from_axis_angle(Vec3::X, -90.0f32.to_radians()),
                    ),
                    2.5,
                );
                gizmos.arrow(
                    global_transform.translation(),
                    global_transform.translation() + direction * Vec3::NEG_Y * 2.5,
                    palettes::tailwind::RED_700,
                );
            }
            #[cfg(feature = "alt_rotate_continuous")]
            Some(Mode::RotateContinuous(data)) => {
                let direction = data.direction.unwrap();
                let (axis, _) = direction.to_bevy_normalized().to_axis_angle();
                let correction = Quat::from_axis_angle(Vec3::X, -90.0f32.to_radians());
                let corrected_axis = Vec3::new(axis.x, -axis.z, -axis.y);
                gizmos.axes(
                    Isometry3d::new(global_transform.translation(), correction),
                    2.5,
                );
                gizmos.arrow(
                    global_transform.translation(),
                    global_transform.translation() + corrected_axis * 2.5,
                    palettes::tailwind::RED_700,
                );
            }
            _ => {}
        }
    }
}
