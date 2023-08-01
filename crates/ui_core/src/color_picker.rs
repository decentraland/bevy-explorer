use bevy::{math::Vec3Swizzles, prelude::*, window::PrimaryWindow};
use bevy_egui::{egui, EguiContext};

use crate::ui_actions::DataChanged;
use common::util::TryInsertEx;

use super::focus::Focus;

#[derive(Component)]
pub struct ColorPicker {
    pub color: [f32; 3],
}

impl ColorPicker {
    pub fn new_linear(color: Color) -> Self {
        Self {
            color: color.as_linear_rgba_f32()[0..3].try_into().unwrap(),
        }
    }

    pub fn get_linear(&self) -> Color {
        Color::rgb_linear(self.color[0], self.color[1], self.color[2])
    }
}

#[allow(clippy::type_complexity)]
pub fn update_color_picker_components(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut color_pickers: Query<(
        Entity,
        &mut ColorPicker,
        &Style,
        &Node,
        &GlobalTransform,
        Option<&mut Interaction>,
        Option<&Focus>,
    )>,
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else { return; };
    let ctx = ctx.get_mut();

    for (entity, mut color_picker, style, node, transform, _maybe_interaction, _maybe_focus) in
        color_pickers.iter_mut()
    {
        let center = transform.translation().xy();
        let size = node.size();
        let topleft = center - size / 2.0;

        if matches!(style.display, Display::Flex) {
            egui::Window::new(format!("{entity:?}"))
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .frame(egui::Frame::none())
                .title_bar(false)
                .show(ctx, |ui| {
                    let response = ui.color_edit_button_rgb(&mut color_picker.color);

                    // pass through focus and interaction
                    if response.changed() {
                        commands.entity(entity).try_insert(DataChanged);
                    }
                });
        }
    }
}
