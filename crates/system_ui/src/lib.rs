pub mod chat;
pub mod profile;
pub mod sysinfo;

use bevy::prelude::*;

use common::{structs::UiRoot, sets::SetupSets};

use self::{chat::ChatPanelPlugin, profile::ProfileEditPlugin, sysinfo::SysInfoPlanelPlugin};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER));
        app.add_startup_system(setup.in_set(SetupSets::Init));

        app.add_plugin(SysInfoPlanelPlugin);
        app.add_plugin(ChatPanelPlugin);
        app.add_plugin(ProfileEditPlugin);
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
                    size: Size {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            Interaction::default(),
            UiRoot,
        ))
        .id();

    ui_root.0 = root;
}
