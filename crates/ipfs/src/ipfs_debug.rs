use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use crate::{IPFS_CACHED, IPFS_FAILED, IPFS_IN_FLIGHT, IPFS_NON_IPFS, IPFS_SUCCESS};

const FILE_GRID_ROWS: usize = 1024;
const CELL_PER_ROW: usize = 4;
const CELL_FONT_SIZE: f32 = 8.;
const SCROLL_FACTOR: f32 = 128.;

pub struct IpfsDebugPlugin;

impl Plugin for IpfsDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_systems(
            Update,
            (
                toggle_display.run_if(input_just_pressed(KeyCode::F1)),
                update_text_from_atomics,
                (
                    receive_debug,
                    trim_files_list,
                    scroll_bottom.run_if(in_state(IpfsFileGridState::Tail)),
                )
                    .chain()
                    .run_if(resource_exists::<IpfsDebugReceiver>),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        app.init_state::<IpfsFileGridState>();

        if !app.world().contains_resource::<IpfsDebugReceiver>() {
            warn!("IpfsDebugReceiver was not initialized. Debug overlay will not display per file info.");
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
enum IpfsFileGridState {
    Free,
    #[default]
    Tail,
}

#[derive(Resource, Deref, DerefMut)]
pub struct IpfsDebugReceiver(pub tokio::sync::mpsc::Receiver<IpfsDebug>);

#[derive(Debug)]
pub struct IpfsDebug {
    pub path: PathBuf,
    pub status: IpfsDebugStatus,
    pub length: usize,
}

#[derive(Debug)]
pub enum IpfsDebugStatus {
    Success,
    Failure,
    Cached,
    NonIpfs,
}

#[derive(Component)]
struct UiRoot;

#[derive(Component)]
struct FileGrid;

#[derive(Component)]
struct FileCell;

#[derive(Component, Deref)]
struct AtomicSource(&'static AtomicU32);

fn setup(mut commands: Commands) {
    commands.spawn((
        UiRoot,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            left: Val::Px(32.),
            top: Val::Px(32.),
            min_width: Val::Px(32.),
            min_height: Val::Px(32.),
            row_gap: Val::Px(4.),
            margin: UiRect::all(Val::Px(4.)),
            flex_direction: FlexDirection::Column,
            ..Default::default()
        },
        BackgroundColor(Color::BLACK.with_alpha(0.5)),
        GlobalZIndex(1_000_000_000),
        children![
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.),
                    ..Default::default()
                },
                children![
                    (Text::new("In Flight"),),
                    (Text::new("0"), AtomicSource(&IPFS_IN_FLIGHT)),
                    column_divider(),
                    (Text::new("Success"),),
                    (Text::new("0"), AtomicSource(&IPFS_SUCCESS)),
                    column_divider(),
                    (Text::new("Cached"),),
                    (Text::new("0"), AtomicSource(&IPFS_CACHED)),
                    column_divider(),
                    (Text::new("Non-ipfs"),),
                    (Text::new("0"), AtomicSource(&IPFS_NON_IPFS)),
                    column_divider(),
                    (Text::new("Failed"),),
                    (Text::new("0"), AtomicSource(&IPFS_FAILED)),
                ]
            ),
            (
                FileGrid,
                Node {
                    display: Display::Grid,
                    max_width: Val::Vw(75.),
                    max_height: Val::Vh(75.),
                    row_gap: Val::Px(4.),
                    column_gap: Val::Px(4.),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    ..Default::default()
                },
                Observer::new(scroll_file_grid)
            ),
            (
                Node {
                    margin: UiRect::all(Val::Px(4.)),
                    column_gap: Val::Px(4.),
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                },
                children![
                    (
                        Text::new("Clear"),
                        BackgroundColor(Color::BLACK.lighter(0.25)),
                        Observer::new(clear_file_grid)
                    ),
                    (
                        Text::new("Bottom"),
                        BackgroundColor(Color::BLACK.lighter(0.25)),
                        Observer::new(go_to_bottom)
                    )
                ]
            )
        ],
    ));
}

fn toggle_display(ui_root: Single<&mut Node, With<UiRoot>>) {
    debug!("Toggling Ipfs Debug overlay");
    let mut node = ui_root.into_inner();
    node.display = match &node.display {
        Display::None => Display::Flex,
        _ => Display::None,
    };
}

fn update_text_from_atomics(atomic_sources: Query<(&mut Text, &AtomicSource)>) {
    for (mut text, atomic_source) in atomic_sources {
        **text = atomic_source.load(Ordering::Relaxed).to_string();
    }
}

fn trim_files_list(
    mut commands: Commands,
    file_grid: Single<&Children, With<FileGrid>>,
    mut nodes: Query<&mut Node, With<FileCell>>,
) {
    let children = file_grid.into_inner();
    let excess = (children.len() / CELL_PER_ROW).saturating_sub(FILE_GRID_ROWS);

    if excess > 0 {
        trace!("Trimming {} excess rows", excess);

        for child in &children[0..(excess * CELL_PER_ROW)] {
            commands.entity(*child).despawn();
        }

        for child in &children[(excess * CELL_PER_ROW)..] {
            let Ok(mut node) = nodes.get_mut(*child) else {
                unreachable!("All children of FileGrid must be FileCell.");
            };

            node.grid_row =
                GridPlacement::start(node.grid_row.get_start().unwrap() - excess as i16);
        }
    }
}

fn receive_debug(
    mut commands: Commands,
    file_grid: Single<(Entity, Option<&Children>), With<FileGrid>>,
    mut ipfs_debug_receiver: ResMut<IpfsDebugReceiver>,
) {
    let (file_grid, children) = file_grid.into_inner();
    let mut next_row =
        (children.map(|children| children.len()).unwrap_or_default() / CELL_PER_ROW + 1) as i16;

    loop {
        let debug = match ipfs_debug_receiver.try_recv() {
            Ok(debug) => debug,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                commands.remove_resource::<IpfsDebugReceiver>();
                break;
            }
        };

        trace!("Spawning row {}", next_row);
        commands.spawn((
            FileCell,
            ChildOf(file_grid),
            Node {
                grid_row: GridPlacement::start(next_row),
                grid_column: GridPlacement::start(1),
                ..Default::default()
            },
            Text::new(debug.path.display().to_string()),
            TextFont {
                font_size: CELL_FONT_SIZE,
                ..Default::default()
            },
        ));
        commands.spawn((
            FileCell,
            ChildOf(file_grid),
            Node {
                grid_row: GridPlacement::start(next_row),
                grid_column: GridPlacement::start(2),
                ..Default::default()
            },
            Text::new(format!("{:?}", debug.status)),
            TextFont {
                font_size: CELL_FONT_SIZE,
                ..Default::default()
            },
        ));
        commands.spawn((
            FileCell,
            ChildOf(file_grid),
            Node {
                grid_row: GridPlacement::start(next_row),
                grid_column: GridPlacement::start(3),
                ..Default::default()
            },
            Text::new("0.0s"),
            TextFont {
                font_size: CELL_FONT_SIZE,
                ..Default::default()
            },
        ));
        commands.spawn((
            FileCell,
            ChildOf(file_grid),
            Node {
                grid_row: GridPlacement::start(next_row),
                grid_column: GridPlacement::start(4),
                ..Default::default()
            },
            Text::new(format!("{}b", debug.length)),
            TextFont {
                font_size: CELL_FONT_SIZE,
                ..Default::default()
            },
        ));

        next_row += 1;
    }
}

fn scroll_bottom(file_grid: Single<&mut ScrollPosition, With<FileGrid>>) {
    let mut scroll_position = file_grid.into_inner();
    scroll_position.offset_y += 512.;
}

fn scroll_file_grid(
    mut trigger: Trigger<Pointer<Scroll>>,
    mut commands: Commands,
    mut file_grid: Single<&mut ScrollPosition, With<FileGrid>>,
) {
    if trigger.target() != trigger.observer() {
        return;
    }
    trigger.propagate(false);

    let scroll = trigger.event();

    commands.set_state(IpfsFileGridState::Free);
    file_grid.offset_y += scroll.y * SCROLL_FACTOR;
}

fn clear_file_grid(
    mut trigger: Trigger<Pointer<Pressed>>,
    mut commands: Commands,
    file_grid: Single<Entity, With<FileGrid>>,
) {
    if trigger.target() != trigger.observer() {
        return;
    }
    trigger.propagate(false);

    commands.entity(*file_grid).despawn_related::<Children>();
}

fn go_to_bottom(mut trigger: Trigger<Pointer<Pressed>>, mut commands: Commands) {
    if trigger.target() != trigger.observer() {
        return;
    }
    trigger.propagate(false);

    commands.set_state(IpfsFileGridState::Tail);
}

fn column_divider() -> impl Bundle {
    (
        Node {
            width: Val::Px(2.),
            height: Val::Percent(100.),
            ..Default::default()
        },
        BackgroundColor(Color::WHITE),
    )
}
