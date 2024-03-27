use bevy::{
    ecs::system::{
        lifetimeless::{SQuery, Write},
        SystemParamItem,
    },
    prelude::*,
    window::PrimaryWindow,
};
use bevy_dui::DuiRegistry;
use common::structs::{AppConfig, WindowSetting};

use super::{spawn_enum_setting_template, AppSetting, EnumAppSetting};

impl EnumAppSetting for WindowSetting {
    type VParam = ();
    fn variants(_: ()) -> Vec<Self> {
        vec![Self::Windowed, Self::Fullscreen, Self::Borderless]
    }

    fn name(&self) -> String {
        match self {
            WindowSetting::Fullscreen => "Fullscreen",
            WindowSetting::Windowed => "Window",
            WindowSetting::Borderless => "Borderless Fullscreen",
        }
        .to_owned()
    }
}

impl AppSetting for WindowSetting {
    type Param = SQuery<Write<Window>, With<PrimaryWindow>>;

    fn title() -> String {
        "Window mode".to_owned()
    }

    fn description(&self) -> String {
        format!("Window mode.\n\n{}", 
            match self {
                WindowSetting::Fullscreen => "Fullscreen: Native fullscreen mode.",
                WindowSetting::Windowed => "Windowed: Not fullscreen.",
                WindowSetting::Borderless => "Borderless Fullscreen: Use a fullscreen window at native resultion, changing resolution will have no effect.",
            }
        )
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.window = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.window
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, mut window: SystemParamItem<Self::Param>, _: Commands) {
        let mut window = window.get_single_mut().unwrap();
        window.mode = match self {
            WindowSetting::Fullscreen => bevy::window::WindowMode::Fullscreen,
            WindowSetting::Windowed => bevy::window::WindowMode::Windowed,
            WindowSetting::Borderless => bevy::window::WindowMode::BorderlessFullscreen,
        };
    }
}

/*

changing resolution while in SizedFullscreen is very buggy
disabled until this is fixed up

#[derive(Resource, Default)]
pub struct MonitorResolutions(Vec<UVec2>);

pub fn set_resolutions(
    winit_windows: NonSend<WinitWindows>,
    window_query: Query<Entity, With<PrimaryWindow>>,
    mut res: ResMut<MonitorResolutions>,
) {
    if winit_windows.is_changed() {
        println!("update resolutions!");
        if let Some(monitor) = window_query
            .get_single()
            .ok()
            .and_then(|entity| winit_windows.get_window(entity))
            .and_then(|winit_window| winit_window.current_monitor())
        {
            res.0 = monitor
                .video_modes()
                .map(|vm| {
                    let size = vm.size();
                    UVec2::new(size.width, size.height)
                })
                .collect();
            res.0.sort_by(|a, b| (a.x * a.y).cmp(&(b.x * b.y)));
            res.0.dedup();
        }
    }
}

impl EnumAppSetting for FullscreenResSetting {
    type VParam = SRes<MonitorResolutions>;
    fn variants(reses: Res<MonitorResolutions>) -> Vec<Self> {
        reses.0.iter().map(|res| Self(*res)).collect()
    }

    fn name(&self) -> String {
        format!("{} x {}", self.0.x, self.0.y)
    }
}

impl AppSetting for FullscreenResSetting {
    type Param = SQuery<Write<Window>, With<PrimaryWindow>>;

    fn title() -> String {
        "Fullscreen Resolution".to_owned()
    }

    fn description(&self) -> String {
        "Fullscreen Resolution.\n\nThe resolution to use when in native fullscreen mode. The setting has no effect in other modes.".to_string()
    }

    fn save(&self, config: &mut AppConfig) {
        config.graphics.fullscreen_res = *self;
    }

    fn load(config: &AppConfig) -> Self {
        config.graphics.fullscreen_res
    }

    fn spawn_template(commands: &mut Commands, dui: &DuiRegistry, config: &AppConfig) -> Entity {
        spawn_enum_setting_template::<Self>(commands, dui, config)
    }

    fn apply(&self, mut window: SystemParamItem<Self::Param>, _: Commands) {
        let mut window = window.get_single_mut().unwrap();
        window.resolution = WindowResolution::new(self.0.x as f32, self.0.y as f32);
    }
}
*/
