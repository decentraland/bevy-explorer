pub mod chat;
pub mod profile;
pub mod sysinfo;
pub mod toasts;

use bevy::prelude::*;

use common::{sets::SetupSets, structs::UiRoot};
use input_manager::MouseInteractionComponent;
use toasts::ToastsPlugin;

use self::{chat::ChatPanelPlugin, profile::ProfileEditPlugin, sysinfo::SysInfoPanelPlugin};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER));
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Init).before(SetupSets::Main),
        );

        app.add_plugins(SysInfoPanelPlugin);
        app.add_plugins(ChatPanelPlugin);
        app.add_plugins(ProfileEditPlugin);
        app.add_plugins(ToastsPlugin);
    }
}

#[derive(Resource)]
struct SystemUiRoot(Entity);

#[allow(clippy::type_complexity)]
fn setup(mut commands: Commands, mut ui_root: ResMut<SystemUiRoot>) {
    let root = commands
        .spawn((
            NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            Interaction::default(),
            MouseInteractionComponent,
            UiRoot,
        ))
        .id();

    ui_root.0 = root;
}
