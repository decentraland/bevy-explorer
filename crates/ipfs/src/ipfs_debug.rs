use std::sync::atomic::{AtomicU32, Ordering};

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use crate::{IPFS_CACHED, IPFS_FAILED, IPFS_IN_FLIGHT, IPFS_NON_IPFS, IPFS_SUCCESS};

pub struct IpfsDebugPlugin;

impl Plugin for IpfsDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_systems(
            Update,
            (
                toggle_visibility.run_if(input_just_pressed(KeyCode::F1)),
                update_text_from_atomics,
            ),
        );
    }
}

#[derive(Component)]
struct UiRoot;

#[derive(Component, Deref)]
struct AtomicSource(&'static AtomicU32);

fn setup(mut commands: Commands) {
    commands.spawn((
        UiRoot,
        Node {
            left: Val::Px(32.),
            top: Val::Px(32.),
            min_width: Val::Px(32.),
            min_height: Val::Px(32.),
            flex_direction: FlexDirection::Column,
            ..Default::default()
        },
        BackgroundColor(Color::BLACK.with_alpha(0.25)),
        Visibility::Hidden,
        GlobalZIndex(1_000_000_000),
        children![(
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
        )],
    ));
}

fn toggle_visibility(ui_root: Single<&mut Visibility, With<UiRoot>>) {
    let mut visibility = ui_root.into_inner();
    *visibility = match *visibility {
        Visibility::Hidden => Visibility::Inherited,
        _ => Visibility::Hidden,
    };
}

fn update_text_from_atomics(atomic_sources: Query<(&mut Text, &AtomicSource)>) {
    for (mut text, atomic_source) in atomic_sources {
        **text = atomic_source.load(Ordering::Relaxed).to_string();
    }
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
