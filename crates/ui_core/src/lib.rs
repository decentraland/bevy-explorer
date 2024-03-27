pub mod bound_node;
pub mod button;
pub mod color_picker;
pub mod combo_box;
pub mod focus;
pub mod interact_style;
pub mod nine_slice;
pub mod scrollable;
pub mod spinner;
pub mod stretch_uvs_image;
pub mod text_size;
pub mod textentry;
pub mod toggle;
pub mod ui_actions;
pub mod ui_builder;

use std::{any::type_name, marker::PhantomData};

use bevy::{
    asset::{DependencyLoadState, LoadState, RecursiveDependencyLoadState},
    ecs::{
        schedule::SystemConfigs,
        system::{EntityCommand, EntityCommands},
    },
    prelude::*,
    utils::{HashMap, HashSet},
};
use bevy_dui::{DuiPlugin, DuiRegistry};
use bevy_egui::EguiPlugin;
use bound_node::BoundedNodePlugin;
use button::{DuiButtonSetTemplate, DuiButtonTemplate, DuiTabGroupTemplate};
use color_picker::ColorPickerPlugin;
use combo_box::ComboBoxPlugin;
use nine_slice::Ui9SlicePlugin;
use once_cell::sync::OnceCell;

use common::sets::SetupSets;
use spinner::SpinnerPlugin;
use stretch_uvs_image::StretchUvsImagePlugin;
use text_size::TextSizePlugin;
use textentry::TextEntryPlugin;
use toggle::TogglePlugin;

use self::{
    focus::FocusPlugin, interact_style::InteractStylePlugin, scrollable::ScrollablePlugin,
    ui_actions::UiActionPlugin,
};

pub static TEXT_SHAPE_FONT_SANS: OnceCell<Handle<Font>> = OnceCell::new();
pub static TEXT_SHAPE_FONT_SERIF: OnceCell<Handle<Font>> = OnceCell::new();
pub static TEXT_SHAPE_FONT_MONO: OnceCell<Handle<Font>> = OnceCell::new();
pub static TITLE_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BODY_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BUTTON_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static BUTTON_DISABLED_TEXT_STYLE: OnceCell<TextStyle> = OnceCell::new();
pub static HOVER_TEXT_STYLE: OnceCell<[TextStyle; 10]> = OnceCell::new();

pub struct UiCorePlugin;

impl Plugin for UiCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DuiPlugin);
        app.add_plugins(BoundedNodePlugin);
        app.add_plugins(EguiPlugin);
        app.add_plugins(UiActionPlugin);
        app.add_plugins(FocusPlugin);
        app.add_plugins(InteractStylePlugin);
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
    tracker.load_asset(asset_server.load_folder("ui"));
    tracker.load_asset(asset_server.load_folder("images"));

    dui.register_template("button", DuiButtonTemplate);
    dui.register_template("button-set", DuiButtonSetTemplate);
    dui.register_template("tab-group", DuiTabGroupTemplate);

    TEXT_SHAPE_FONT_SANS
        .set(asset_server.load("fonts/NotoSans-Regular.ttf"))
        .unwrap();
    TEXT_SHAPE_FONT_SERIF
        .set(asset_server.load("fonts/NotoSerif-Regular.ttf"))
        .unwrap();
    TEXT_SHAPE_FONT_MONO
        .set(asset_server.load("fonts/NotoMono-Regular.ttf"))
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
            font: asset_server.load("fonts/NotoSans-Regular.ttf"),
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

#[derive(States, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Loading,
    Ready,
}

// state tracker, allows running several systems until completed, then switching to a new state
#[derive(Resource, Default)]
pub struct StateTracker<S: States> {
    assets: HashSet<UntypedHandle>,
    systems: HashMap<&'static str, bool>,
    _p: PhantomData<fn() -> S>,
}

impl<S: States> StateTracker<S> {
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

pub struct ModifyComponent<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> {
    func: F,
    _p: PhantomData<fn() -> C>,
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> EntityCommand
    for ModifyComponent<C, F>
{
    fn apply(self, id: Entity, world: &mut World) {
        if let Some(mut c) = world.get_mut::<C>(id) {
            (self.func)(&mut *c)
        }
    }
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> ModifyComponent<C, F> {
    fn new(func: F) -> Self {
        Self {
            func,
            _p: PhantomData,
        }
    }
}

pub trait ModifyComponentExt {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self;
}

impl<'a> ModifyComponentExt for EntityCommands<'a> {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self {
        self.add(ModifyComponent::new(func))
    }
}

// blocker for egui elements to prevent interaction fallthrough
#[derive(Component)]
struct Blocker;
