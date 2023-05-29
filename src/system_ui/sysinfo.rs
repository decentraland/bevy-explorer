use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

use crate::AppConfig;

use super::SystemUiRoot;

pub struct SysInfoPlanelPlugin;

impl Plugin for SysInfoPlanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup.after(super::setup));
        app.add_system(update_fps);
    }
}

#[derive(Component)]
struct FpsLabel;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    root: Res<SystemUiRoot>,
    config: Res<AppConfig>,
) {
    // fps counter
    if config.graphics.log_fps {
        commands.entity(root.0).with_children(|commands| {
            // left vertical fill (border)
            commands
                .spawn(NodeBundle {
                    style: Style {
                        size: Size::new(Val::Px(80.), Val::Px(30.)),
                        align_self: AlignSelf::FlexStart,
                        border: UiRect::all(Val::Px(2.)),
                        ..default()
                    },
                    background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                    ..default()
                })
                .with_children(|parent| {
                    // text
                    parent.spawn((
                        TextBundle::from_section(
                            "Text Example",
                            TextStyle {
                                font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                                font_size: 20.0,
                                color: Color::GREEN,
                            },
                        )
                        .with_style(Style {
                            margin: UiRect::all(Val::Px(5.)),
                            ..default()
                        }),
                        FpsLabel,
                    ));
                });
        });
    }
}

fn update_fps(
    mut q: Query<&mut Text, With<FpsLabel>>,
    diagnostics: Res<Diagnostics>,
    mut last_update: Local<u32>,
    time: Res<Time>,
) {
    let tick = (time.elapsed_seconds() * 10.0) as u32;
    if tick == *last_update {
        return;
    }
    *last_update = tick;

    if let Ok(mut text) = q.get_single_mut() {
        if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
            let fps = fps.smoothed().unwrap_or_default();
            text.sections[0].value = format!("fps: {fps:.0}");
        }
    }
}
