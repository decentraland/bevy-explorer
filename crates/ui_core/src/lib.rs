pub mod bound_node;
pub mod button;
pub mod color_picker;
pub mod combo_box;
pub mod dui_utils;
pub mod focus;
pub mod interact_style;
pub mod nine_slice;
pub mod scrollable;
pub mod spinner;
pub mod stretch_uvs_image;
pub mod text_size;
// pub mod textentry;
pub mod interact_sounds;
pub mod text_entry;
pub mod toggle;
pub mod ui_actions;
pub mod ui_builder;

use std::{any::type_name, marker::PhantomData};

use bevy::{
    asset::{DependencyLoadState, LoadState, RecursiveDependencyLoadState},
    ecs::schedule::SystemConfigs,
    prelude::*,
    state::state::FreelyMutableState,
    platform::collections::{HashMap, HashSet},
};
use bevy_dui::{DuiNodeList, DuiPlugin, DuiRegistry};
use bevy_egui::EguiPlugin;
use bound_node::BoundedNodePlugin;
use button::{DuiButtonSetTemplate, DuiButtonTemplate, DuiTabGroupTemplate};
use color_picker::ColorPickerPlugin;
use combo_box::ComboBoxPlugin;
use interact_sounds::InteractSoundsPlugin;
use nine_slice::Ui9SlicePlugin;
use once_cell::sync::OnceCell;

use common::sets::SetupSets;
use spinner::SpinnerPlugin;
use stretch_uvs_image::StretchUvsImagePlugin;
use text_entry::TextEntryPlugin;
use text_size::TextSizePlugin;
use toggle::TogglePlugin;

use self::{
    focus::FocusPlugin, interact_style::InteractStylePlugin, scrollable::ScrollablePlugin,
    ui_actions::UiActionPlugin,
};

pub static TITLE_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BODY_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static HOVER_TEXT_STYLE: OnceCell<[TextStyle; 10]> = OnceCell::new();

pub static FONTS: OnceCell<HashMap<(FontName, WeightName), Handle<Font>>> = OnceCell::new();

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
pub enum FontName {
    Mono,
    Sans,
    Serif,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
pub enum WeightName {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

pub fn user_font(name: FontName, weight: WeightName) -> Handle<Font> {
    FONTS.get().unwrap().get(&(name, weight)).unwrap().clone()
}

pub struct UiCorePlugin;

impl Plugin for UiCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DuiPlugin);
        app.add_plugins(BoundedNodePlugin);
        app.add_plugins(EguiPlugin);
        app.add_plugins(UiActionPlugin);
        app.add_plugins(FocusPlugin);
        app.add_plugins(InteractStylePlugin);
        app.add_plugins(InteractSoundsPlugin);
        app.add_plugins(ScrollablePlugin);
        app.add_plugins(Ui9SlicePlugin);
        app.add_plugins(StretchUvsImagePlugin);
        app.add_plugins(TogglePlugin);
        app.add_plugins(TextSizePlugin);
        app.add_plugins(ComboBoxPlugin);
        app.add_plugins(TextEntryPlugin);
        app.add_plugins(SpinnerPlugin);
        app.add_plugins(ColorPickerPlugin);
        app.init_state::<State>();
        app.init_resource::<StateTracker<State>>();
        app.add_systems(Startup, setup.in_set(SetupSets::Init));
        app.add_systems(
            Update,
            StateTracker::<State>::transition_when_finished(State::Ready)
                .run_if(in_state(State::Loading)),
        );
    }
}

#[allow(clippy::type_complexity)]
fn setup(
    asset_server: Res<AssetServer>,
    mut tracker: ResMut<StateTracker<State>>,
    mut dui: ResMut<DuiRegistry>,
) {
    // tracker.load_asset(asset_server.load_folder("ui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/app_settings.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/avatar.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/button.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/change-realm.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/chat.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/combo.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/dialog.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/discover.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/emote-select.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/emote.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/foreign-profile-dialog.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/fullscreen-block.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/login.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/map.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/minimap.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/motd.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/nft-dialog.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/oow.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/permissions-dialog.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/permissions.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/profile-detail.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/profile.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/spinner.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/toast.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/tracker.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/update.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/utils.dui"));
    tracker.load_asset(asset_server.load::<DuiNodeList>("ui/wearables.dui"));

    // tracker.load_asset(asset_server.load_folder("images"));

    dui.register_template("button", DuiButtonTemplate);
    dui.register_template("button-set", DuiButtonSetTemplate);
    dui.register_template("tab-group", DuiTabGroupTemplate);

    {
        use FontName::*;
        use WeightName::*;
        FONTS
            .set(HashMap::from_iter([
                (
                    (Mono, Regular),
                    asset_server.load("fonts/NotoSansMono-Regular.ttf"),
                ),
                (
                    (Mono, Bold),
                    asset_server.load("fonts/NotoSansMono-Bold.ttf"),
                ),
                (
                    (Mono, Italic),
                    asset_server.load("fonts/NotoSansMono-Regular.ttf"),
                ),
                (
                    (Mono, BoldItalic),
                    asset_server.load("fonts/NotoSansMono-Bold.ttf"),
                ),
                (
                    (Sans, Regular),
                    asset_server.load("fonts/NotoSans-Regular.ttf"),
                ),
                ((Sans, Bold), asset_server.load("fonts/NotoSans-Bold.ttf")),
                (
                    (Sans, Italic),
                    asset_server.load("fonts/NotoSans-Italic.ttf"),
                ),
                (
                    (Sans, BoldItalic),
                    asset_server.load("fonts/NotoSans-BoldItalic.ttf"),
                ),
                (
                    (Serif, Regular),
                    asset_server.load("fonts/NotoSerif-Regular.ttf"),
                ),
                ((Serif, Bold), asset_server.load("fonts/NotoSerif-Bold.ttf")),
                (
                    (Serif, Italic),
                    asset_server.load("fonts/NotoSerif-Italic.ttf"),
                ),
                (
                    (Serif, BoldItalic),
                    asset_server.load("fonts/NotoSerif-BoldItalic.ttf"),
                ),
            ]))
            .unwrap();
    }

    TITLE_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/NotoSans-Bold.ttf"),
            font_size: 35.0 / 1.3,
            color: Color::BLACK,
        })
        .unwrap();
    BODY_TEXT_STYLE
        .set(TextStyle {
            font: asset_server.load("fonts/NotoSans-Regular.ttf"),
            font_size: 25.0 / 1.3,
            color: Color::BLACK,
        })
        .unwrap();
    HOVER_TEXT_STYLE
        .set(
            (0..10)
                .map(|i| TextStyle {
                    font: asset_server.load("fonts/NotoSans-Bold.ttf"),
                    font_size: 25.0 / 1.3,
                    color: Color::srgba(1.0, 1.0, 1.0, i as f32 / 9.0),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        )
        .unwrap();
}

#[derive(States, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Loading,
    Ready,
}

// state tracker, allows running several systems until completed, then switching to a new state
#[derive(Resource, Default)]
pub struct StateTracker<S: States + FreelyMutableState> {
    assets: HashSet<UntypedHandle>,
    systems: HashMap<&'static str, bool>,
    _p: PhantomData<fn() -> S>,
}

impl<S: States + FreelyMutableState> StateTracker<S> {
    pub fn load_asset<A: Asset>(&mut self, h: Handle<A>) {
        self.assets.insert(h.untyped());
    }

    // run the system every tick until it returns false
    pub fn run_while<M, F: IntoSystem<(), bool, M>>(f: F) -> SystemConfigs {
        let update_cond = move |In(retain): In<bool>, mut slf: ResMut<Self>| {
            slf.systems.insert(type_name::<F>(), !retain);
        };

        let check_cond = |slf: Res<Self>| {
            !slf.systems
                .get(type_name::<F>())
                .copied()
                .unwrap_or_default()
        };

        f.pipe(update_cond).run_if(check_cond)
    }

    // transition when all assets are loaded and all systems are finished
    pub fn transition_when_finished(next: S) -> SystemConfigs {
        let system = move |slf: Res<StateTracker<S>>,
                           asset_server: Res<AssetServer>,
                           mut next_state: ResMut<NextState<S>>| {
            if slf.assets.iter().all(|a| {
                asset_server.get_load_states(a.id())
                    == Some((
                        LoadState::Loaded,
                        DependencyLoadState::Loaded,
                        RecursiveDependencyLoadState::Loaded,
                    ))
            }) && slf.systems.values().all(|v| *v)
            {
                next_state.set(next.clone())
            }
        };

        system.into_configs()
    }
}

// blocker for egui elements to prevent interaction fallthrough
#[derive(Component)]
struct Blocker;
