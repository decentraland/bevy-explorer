pub mod chat;
pub mod ui_actions;
pub mod dialog;
pub mod focus;
pub mod interact_style;
pub mod profile;
pub mod scrollable;
pub mod sysinfo;
pub mod textentry;
pub mod ui_builder;
pub mod color_picker;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use once_cell::sync::OnceCell;

use self::{
    chat::ChatPanelPlugin, ui_actions::UiActionPlugin, focus::FocusPlugin,
    interact_style::InteractStylePlugin, profile::ProfileEditPlugin, scrollable::ScrollablePlugin,
    sysinfo::SysInfoPlanelPlugin, textentry::update_text_entry_components, color_picker::update_color_picker_components,
};

static TITLE_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
static BODY_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
static BUTTON_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER));
        app.add_startup_system(setup);
        app.add_plugin(EguiPlugin);
        app.add_plugin(UiActionPlugin);
        app.add_plugin(FocusPlugin);
        app.add_plugin(InteractStylePlugin);
        app.add_plugin(ScrollablePlugin);
        app.add_system(update_text_entry_components);
        app.add_system(update_color_picker_components);

        app.add_plugin(SysInfoPlanelPlugin);
        app.add_plugin(ChatPanelPlugin);
        app.add_plugin(ProfileEditPlugin);
    }
}

#[derive(Resource)]
struct SystemUiRoot(Entity);

#[derive(Component)]
pub struct UiRoot;

#[allow(clippy::type_complexity)]
fn setup(
    mut commands: Commands,
    mut ui_root: ResMut<SystemUiRoot>,
    asset_server: Res<AssetServer>,
) {
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

    TITLE_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
            font_size: 35.0,
            color: Color::BLACK,
        })
        .unwrap();
    BODY_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
            font_size: 15.0,
            color: Color::BLACK,
        })
        .unwrap();
    BUTTON_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
            font_size: 20.0,
            color: Color::BLACK,
        })
        .unwrap();
}
