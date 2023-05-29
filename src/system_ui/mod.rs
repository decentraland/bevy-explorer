pub mod chat;
pub mod click_actions;
pub mod focus;
pub mod interact_style;
pub mod sysinfo;
pub mod textentry;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use self::{
    chat::ChatPanelPlugin, click_actions::UiActionPlugin, focus::FocusPlugin,
    interact_style::InteractStylePlugin, sysinfo::SysInfoPlanelPlugin,
    textentry::update_text_entry_components,
};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER));
        app.add_startup_system(setup);
        app.add_plugin(EguiPlugin);
        app.add_plugin(UiActionPlugin);
        app.add_plugin(FocusPlugin);
        app.add_plugin(InteractStylePlugin);
        app.add_system(update_text_entry_components);

        app.add_plugin(SysInfoPlanelPlugin);
        app.add_plugin(ChatPanelPlugin);
    }
}

#[derive(Resource)]
pub struct SystemUiRoot(Entity);

#[allow(clippy::type_complexity)]
fn setup(mut commands: Commands, mut ui_root: ResMut<SystemUiRoot>) {
    let root = commands
        .spawn(NodeBundle {
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
        })
        .id();

    ui_root.0 = root;
}
