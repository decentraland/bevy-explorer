use anyhow::anyhow;
use bevy::{prelude::*, ui::FocusPolicy};
use bevy_dui::{DuiRegistry, DuiTemplate};
use common::{structs::SystemAudio, util::FireEventEx};

use crate::ui_actions::{Click, DataChanged, On};

#[derive(Component)]
pub struct Toggled(pub bool);

pub struct TogglePlugin;

impl Plugin for TogglePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

pub fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("toggle", ToggleTemplate);
}

pub struct ToggleTemplate;
impl DuiTemplate for ToggleTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let on_toggle = props
            .take::<On<DataChanged>>("ontoggle")?
            .ok_or(anyhow!("missing ontoggle"))?;
        let on = props.take::<bool>("toggled")?.unwrap_or_default();
        let id = commands.id();

        commands.insert((
            ImageBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                image: ctx
                    .asset_server()
                    .load(if on {
                        "images/toggle-on.png"
                    } else {
                        "images/toggle-off.png"
                    })
                    .into(),
                focus_policy: FocusPolicy::Block,
                ..Default::default()
            },
            Interaction::default(),
            On::<Click>::new(
                move |mut commands: Commands,
                      asset_server: Res<AssetServer>,
                      mut q: Query<(&mut Toggled, &mut UiImage)>| {
                    let Ok((mut toggle, mut image)) = q.get_mut(id) else {
                        warn!("toggle not found");
                        return;
                    };

                    if toggle.0 {
                        toggle.0 = false;
                        image.texture = asset_server.load("images/toggle-off.png");
                        commands.fire_event(SystemAudio("sounds/toggle_disable.wav".to_owned()));
                    } else {
                        toggle.0 = true;
                        image.texture = asset_server.load("images/toggle-on.png");
                        commands.fire_event(SystemAudio("sounds/toggle_enable.wav".to_owned()));
                    }

                    commands.entity(id).try_insert(DataChanged);
                },
            ),
            Toggled(on),
            on_toggle,
        ));

        Ok(Default::default())
    }
}
