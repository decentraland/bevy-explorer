use anyhow::anyhow;
use bevy::{math::Vec3Swizzles, prelude::*, window::PrimaryWindow};
use bevy_dui::{DuiRegistry, DuiTemplate};
use bevy_egui::{egui, EguiContext};

use crate::{
    ui_actions::{DataChanged, On},
    Blocker,
};

use super::focus::Focus;

#[derive(Component)]
pub struct ColorPicker {
    pub color: [f32; 3],
}

impl ColorPicker {
    pub fn new_linear(color: Color) -> Self {
        Self {
            color: color.to_linear().to_f32_array()[0..3].try_into().unwrap(),
        }
    }

    pub fn get_linear(&self) -> Color {
        Color::srgb(self.color[0], self.color[1], self.color[2])
    }
}

pub struct ColorPickerPlugin;

impl Plugin for ColorPickerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_color_picker_components);
    }
}

#[allow(clippy::type_complexity)]
fn update_color_picker_components(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut color_pickers: Query<
        (
            Entity,
            &mut ColorPicker,
            &Style,
            &Node,
            &GlobalTransform,
            Option<&mut Interaction>,
            Option<&Focus>,
        ),
        Without<Blocker>,
    >,
    mut blocker: Local<Option<Entity>>,
    mut blocker_display: Query<&mut Style, With<Blocker>>,
    mut blocker_active: Local<bool>,
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else {
        return;
    };
    let ctx = ctx.get_mut();
    let blocker = *blocker.get_or_insert_with(|| {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        display: Display::None,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        ..Default::default()
                    },
                    focus_policy: bevy::ui::FocusPolicy::Block,
                    z_index: ZIndex::Global(i16::MAX as i32 + 5),
                    ..Default::default()
                },
                Blocker,
            ))
            .id()
    });

    let mut popup_active = false;

    for (entity, mut color_picker, style, node, transform, _maybe_interaction, _maybe_focus) in
        color_pickers.iter_mut()
    {
        let center = transform.translation().xy() / ctx.zoom_factor();
        let size = node.size() / ctx.zoom_factor();
        let topleft = center - size / 2.0;

        if matches!(style.display, Display::Flex) {
            egui::Window::new(format!("{entity:?}"))
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .frame(egui::Frame::none())
                .title_bar(false)
                .show(ctx, |ui| {
                    let response = ui.color_edit_button_rgb(&mut color_picker.color);

                    if ui.memory(|mem| mem.any_popup_open()) {
                        popup_active = true;
                    }

                    // pass through focus and interaction
                    if response.changed() {
                        commands.entity(entity).try_insert(DataChanged);
                    }
                });
        }
    }

    if popup_active != *blocker_active {
        blocker_display.get_mut(blocker).unwrap().display = if popup_active {
            Display::Flex
        } else {
            Display::None
        };
        *blocker_active = popup_active;
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("color-picker", DuiColorPicker);
}

pub struct DuiColorPicker;
impl DuiTemplate for DuiColorPicker {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        _: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let picker = ColorPicker::new_linear(
            props
                .take::<Color>("color")?
                .ok_or(anyhow!("no initial color"))?,
        );
        commands.insert(picker);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
