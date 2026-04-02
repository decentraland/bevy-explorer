#[cfg(not(target_arch = "wasm32"))]
use bevy::{ecs::system::lifetimeless::Write, window::PrimaryWindow};
use bevy::{
    ecs::system::{lifetimeless::SQuery, SystemParamItem},
    platform::collections::HashSet,
    prelude::*,
};
use common::structs::{AppConfig, WindowSetting};

use super::{AppSetting, EnumAppSetting};

impl EnumAppSetting for WindowSetting {
    fn variants() -> Vec<Self> {
        vec![
            Self::Windowed,
            Self::Fullscreen,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Borderless,
        ]
    }

    fn name(&self) -> String {
        match self {
            WindowSetting::Fullscreen => "Fullscreen",
            WindowSetting::Windowed => "Window",
            #[cfg(not(target_arch = "wasm32"))]
            WindowSetting::Borderless => "Borderless Fullscreen",
        }
        .to_owned()
    }
}

impl AppSetting for WindowSetting {
    #[cfg(not(target_arch = "wasm32"))]
    type Param = SQuery<Write<Window>, With<PrimaryWindow>>;
    #[cfg(target_arch = "wasm32")]
    type Param = SQuery<()>;

    fn title() -> String {
        "Window mode".to_owned()
    }

    fn description(&self) -> String {
        format!("Window mode.\n\n{}", 
            match self {
                WindowSetting::Fullscreen => "Fullscreen: Native fullscreen mode.",
                WindowSetting::Windowed => "Windowed: Not fullscreen.",
                #[cfg(not(target_arch = "wasm32"))]
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

    fn category() -> super::SettingCategory {
        super::SettingCategory::Graphics
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn apply(&self, mut window: SystemParamItem<Self::Param>, _: Commands, _: &HashSet<Entity>) {
        let mut window = window.single_mut().unwrap();
        window.mode = match self {
            WindowSetting::Fullscreen => bevy::window::WindowMode::Fullscreen(
                MonitorSelection::Current,
                VideoModeSelection::Current,
            ),
            WindowSetting::Windowed => bevy::window::WindowMode::Windowed,
            WindowSetting::Borderless => {
                bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            }
        };
    }

    #[cfg(target_arch = "wasm32")]
    fn apply(&self, _: SystemParamItem<Self::Param>, _: Commands, _: &HashSet<Entity>) {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();

        let already_fullscreen = document.fullscreen_element().is_some();
        match self {
            Self::Fullscreen => {
                if !already_fullscreen {
                    let Some(canvas) = document.get_element_by_id("mygame-canvas") else {
                        error!("No game canvas to make fullscreen.");
                        return;
                    };
                    canvas.request_fullscreen().unwrap();
                }
            }
            Self::Windowed => {
                if already_fullscreen {
                    document.exit_fullscreen();
                }
            }
        }
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
            .single()
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
        let mut window = window.single_mut().unwrap();
        window.resolution = WindowResolution::new(self.0.x as f32, self.0.y as f32);
    }
}
*/
