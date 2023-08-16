use bevy::{math::Vec3Swizzles, prelude::*, utils::HashSet, window::PrimaryWindow};
use bevy_egui::{
    egui::{self, TextEdit},
    EguiContext,
};
use common::util::TryInsertEx;

use crate::ui_actions::DataChanged;

use super::focus::Focus;

#[derive(Component)]
pub struct TextEntry {
    pub font_size: i32,
    pub hint_text: String,
    pub content: String,
    pub enabled: bool,
    pub messages: Vec<String>,
    pub accept_line: bool,
    pub id_entity: Option<Entity>,
}

impl Default for TextEntry {
    fn default() -> Self {
        Self {
            font_size: 12,
            hint_text: Default::default(),
            content: Default::default(),
            enabled: true,
            messages: Default::default(),
            accept_line: true,
            id_entity: None,
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update_text_entry_components(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut text_entries: Query<(
        Entity,
        &mut TextEntry,
        &Style,
        &Node,
        &GlobalTransform,
        Option<&mut Interaction>,
        Option<&Focus>,
    )>,
    mut lost_focus: RemovedComponents<Focus>,
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else { return; };
    let ctx = ctx.get_mut();

    let lost_focus = lost_focus.iter().collect::<HashSet<_>>();

    for (entity, mut textbox, style, node, transform, maybe_interaction, maybe_focus) in
        text_entries.iter_mut()
    {
        let center = transform.translation().xy();
        let size = node.size();
        let topleft = center - size / 2.0;

        if matches!(style.display, Display::Flex) {
            egui::Window::new(format!("{:?}", textbox.id_entity.unwrap_or(entity)))
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .frame(egui::Frame::none())
                .title_bar(false)
                .show(ctx, |ui| {
                    // destructure to split borrow
                    let TextEntry {
                        ref hint_text,
                        ref mut content,
                        ref enabled,
                        ref font_size,
                        ..
                    } = &mut *textbox;
                    let enabled = *enabled;

                    let response = ui.add_enabled(
                        enabled,
                        TextEdit::singleline(content)
                            .frame(false)
                            .desired_width(f32::INFINITY)
                            .text_color(egui::Color32::WHITE)
                            .hint_text(hint_text)
                            .font(egui::FontId::new(
                                *font_size as f32,
                                egui::FontFamily::Proportional,
                            )),
                    );

                    if response.changed() && !textbox.accept_line {
                        commands.entity(entity).try_insert(DataChanged);
                    }

                    // pass through focus and interaction
                    let mut defocus = false;
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if textbox.accept_line && !textbox.content.is_empty() {
                            let message = std::mem::take(&mut textbox.content);
                            response.request_focus();
                            textbox.messages.push(message);
                            commands.entity(entity).try_insert(DataChanged);
                        } else {
                            commands.entity(entity).remove::<Focus>();
                            defocus = true;
                        }
                    }
                    if let Some(mut interaction) = maybe_interaction {
                        if response.has_focus() {
                            *interaction = Interaction::Pressed;
                        } else if response.hovered() {
                            *interaction = Interaction::Hovered;
                        } else {
                            *interaction = Interaction::None;
                        }
                    }
                    if maybe_focus.is_some() && !response.has_focus() && !defocus && enabled {
                        debug!("Focus -> tb focus");
                        response.request_focus();
                    }
                    if maybe_focus.is_none() {
                        if lost_focus.contains(&entity) {
                            debug!("!Focus -> tb surrender focus");
                            response.surrender_focus()
                        } else if response.has_focus() {
                            debug!("tb focus -> Focus");
                            commands.entity(entity).try_insert(Focus);
                        }
                    }
                });
        }
    }
}
