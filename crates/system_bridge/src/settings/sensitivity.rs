use bevy::ecs::system::lifetimeless::SResMut;
use bevy::prelude::*;
use common::inputs::{InputDirectionSetLabel, InputMap};
use common::structs::AppConfig;

use super::{AppSetting, IntAppSetting};

macro_rules! sensitivity_setting {
    ($struct:ident, $label: expr, $name:expr, $description:expr, $base: expr, $scale:expr, ) => {
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $struct(i32);

        impl IntAppSetting for $struct {
            fn from_int(value: i32) -> Self {
                Self(value)
            }

            fn value(&self) -> i32 {
                self.0
            }

            fn min() -> i32 {
                1
            }

            fn max() -> i32 {
                100
            }

            fn scale() -> f32 {
                1.0
            }

            fn display(&self) -> String {
                format!("{}", self.0)
            }
        }

        #[allow(clippy::redundant_closure_call)]
        impl AppSetting for $struct {
            type Param = SResMut<InputMap>;

            fn title() -> String {
                format!("{}", $name)
            }

            fn description(&self) -> String {
                format!("{}\n\n{}", $name, $description)
            }

            fn apply(&self, mut input_map: ResMut<InputMap>, _: Commands, _: &bevy::platform::collections::HashSet<Entity>) {
                input_map
                    .sensitivities
                    .insert($label, $base * ($scale as f32).powf(self.0 as f32 - 50.0));
            }

            fn save(&self, config: &mut AppConfig) {
                config
                    .inputs
                    .1
                    .insert($label, $base * ($scale as f32).powf(self.0 as f32 - 50.0));
            }

            fn load(config: &AppConfig) -> Self {
                Self(
                    config
                        .inputs
                        .1
                        .get(&$label)
                        .map(|v| (v / $base).log($scale).round() as i32 + 50)
                        .unwrap_or(50),
                )
            }

            fn category() -> super::SettingCategory {
                super::SettingCategory::Controls
            }
        }
    };
}

sensitivity_setting!(
    MovementSensitivitySetting,
    InputDirectionSetLabel::Movement,
    "Movement sensitivity",
    "Controls the sensitivity of inputs (gamepad thumbsticks, mouse motion, etc) bound to avatar movement.",
    1.0,
    1.05,
);

sensitivity_setting!(
    ScrollSensitivitySetting,
    InputDirectionSetLabel::Scroll,
    "Scroll sensitivity",
    "Controls the sensitivity of scrolling UI panels.",
    0.1,
    1.10,
);

sensitivity_setting!(
    PointerSensitivitySetting,
    InputDirectionSetLabel::Pointer,
    "Pointer and Locked Camera sensitivity",
    "Controls the sensitivity of the camera when using \"locked\" camera mode.\n\nControls the sensitivity of the pointer when using non-mouse inputs (gamepad thumbsticks, etc).",
    1.0,
    1.05,
);

sensitivity_setting!(
    CameraSensitivitySetting,
    InputDirectionSetLabel::Camera,
    "Camera Sensitivity",
    "Controls the sensitivity of inputs for Camera movement controls.\n\nNOTE: This setting affects only explicit camera controls, it does not affect the speed of camera movement via pointer inputs when the camera is locked. For controlling locked camera movement speed, change the \"Pointer\" sensitivity",
    1.0,
    1.05,
);

sensitivity_setting!(
    CameraZoomSensitivitySetting,
    InputDirectionSetLabel::CameraZoom,
    "Camera Zoom",
    "Controls the sensitivity of Camera Zoom inputs.",
    0.3,
    1.1,
);
