use bevy::{math::Vec3Swizzles, prelude::*, window::PrimaryWindow};
use bevy_egui::{egui, EguiContext};

use crate::ui_actions::DataChanged;

#[derive(Component, Debug)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
}

impl ComboBox {
    pub fn new(
        empty_text: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        allow_null: bool,
        disabled: bool,
        initial_selection: Option<isize>,
    ) -> Self {
        Self {
            empty_text,
            options: options.into_iter().map(Into::into).collect(),
            selected: initial_selection.unwrap_or(-1),
            allow_null,
            disabled,
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update_comboboxen(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut combos: Query<(Entity, &mut ComboBox, &Style, &Node, &GlobalTransform)>,
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else {
        return;
    };
    let ctx = ctx.get_mut();

    for (entity, mut combo, style, node, transform) in combos.iter_mut() {
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
                    let initial_selection = combo.selected;
                    let selected_text = if combo.selected == -1 {
                        &combo.empty_text
                    } else {
                        combo
                            .options
                            .get(combo.selected as usize)
                            .unwrap_or(&combo.empty_text)
                    };

                    let style = ui.style_mut();
                    style.visuals.widgets.active.weak_bg_fill = egui::Color32::TRANSPARENT;
                    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
                    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;

                    egui::ComboBox::from_id_source(entity)
                        .selected_text(selected_text)
                        .wrap(false)
                        .width(size.x)
                        .show_ui(ui, |ui| {
                            // split borrow
                            let ComboBox {
                                ref options,
                                ref mut selected,
                                ..
                            } = &mut *combo;

                            for (i, label) in options.iter().enumerate() {
                                ui.selectable_value(selected, i as isize, label);
                            }
                        });

                    if combo.selected != initial_selection || combo.selected == -1 {
                        if combo.selected == -1 {
                            combo.selected = 0;
                        }
                        commands.entity(entity).try_insert(DataChanged);
                    }
                });
        }
    }
}
