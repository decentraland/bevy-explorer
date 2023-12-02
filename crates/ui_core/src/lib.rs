pub mod color_picker;
pub mod combo_box;
pub mod dialog;
pub mod focus;
pub mod interact_style;
pub mod nine_slice;
pub mod scrollable;
pub mod textentry;
pub mod ui_actions;
pub mod ui_builder;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use combo_box::update_comboboxen;
use nine_slice::Ui9SlicePlugin;
use once_cell::sync::OnceCell;

use common::sets::SetupSets;

use self::{
    color_picker::update_color_picker_components, focus::FocusPlugin,
    interact_style::InteractStylePlugin, scrollable::ScrollablePlugin,
    textentry::update_text_entry_components, ui_actions::UiActionPlugin,
};

pub static TEXT_SHAPE_FONT: OnceCell<Handle<Font>> = OnceCell::new();
pub static TITLE_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BODY_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BUTTON_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BUTTON_DISABLED_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static HOVER_TEXT_STYLE: OnceCell<[TextStyle; 10]> = OnceCell::new();

pub struct UiCorePlugin;

impl Plugin for UiCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin);
        app.add_plugins(UiActionPlugin);
        app.add_plugins(FocusPlugin);
        app.add_plugins(InteractStylePlugin);
        app.add_plugins(ScrollablePlugin);
        app.add_plugins(Ui9SlicePlugin);
        app.add_systems(Update, update_text_entry_components);
        app.add_systems(Update, update_color_picker_components);
        app.add_systems(Update, update_comboboxen);

        app.add_systems(Startup, setup.in_set(SetupSets::Init));
    }
}

#[allow(clippy::type_complexity)]
fn setup(asset_server: Res<AssetServer>) {
    TEXT_SHAPE_FONT
        .set(asset_server.load("fonts/FiraSans-Bold_shapecopy.ttf"))
        .unwrap();
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
    BUTTON_DISABLED_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/FiraSans-Bold.ttf"),
            font_size: 20.0,
            color: Color::GRAY,
        })
        .unwrap();
    HOVER_TEXT_STYLE
        .set(
            (0..10)
                .map(|i| TextStyle {
                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                    font_size: 25.0,
                    color: Color::rgba(1.0, 1.0, 1.0, i as f32 / 9.0),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
        .unwrap();
}
