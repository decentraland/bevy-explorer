use bevy::prelude::*;
use ui_core::ui_actions::{Click, On};

pub struct EmoteUiPlugin;

impl Plugin for EmoteUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // emote button
    commands.spawn((
        ImageBundle {
            image: asset_server.load("images/emote_button.png").into(),
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0 + 26.0 * 2.0),
                right: Val::Px(10.0),
                ..Default::default()
            },
            focus_policy: bevy::ui::FocusPolicy::Block,
            ..Default::default()
        },
        Interaction::default(),
        On::<Click>::new(show_emote_ui),
    ));
}

#[derive(Component)]
pub struct EmoteDialog;

// panel shows until button released or any click
fn show_emote_ui(mut commands: Commands, existing: Query<Entity, With<EmoteDialog>>) {
    commands.spawn((
        NodeBundle {
            ..Default::default()
        },
        EmoteDialog,
    ));

    for ent in existing.iter() {
        if let Some(commands) = commands.get_entity(ent) {
            commands.despawn_recursive();
        }
    }
}
