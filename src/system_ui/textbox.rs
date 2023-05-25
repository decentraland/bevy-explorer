use bevy::{math::Vec3Swizzles, prelude::*};
use bevy_egui::{
    egui::{self, TextEdit},
    EguiContexts,
};

#[derive(Component, Default)]
pub struct TextBox {
    pub content: String,
    pub enabled: bool,
    pub messages: Vec<String>,
}

#[allow(clippy::type_complexity)]
pub fn update_textboxes(
    mut contexts: EguiContexts,
    mut q: Query<(
        Entity,
        &mut TextBox,
        &Style,
        &Node,
        &GlobalTransform,
        Option<&mut Interaction>,
    )>,
) {
    let ctx = contexts.ctx_mut();
    for (entity, mut textbox, style, node, transform, maybe_interaction) in q.iter_mut() {
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
                    let response = ui.add_enabled(
                        textbox.enabled,
                        TextEdit::singleline(&mut textbox.content)
                            .frame(false)
                            .desired_width(f32::INFINITY)
                            .hint_text("say something"),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let message = std::mem::take(&mut textbox.content);
                        if !message.is_empty() {
                            response.request_focus();
                            textbox.messages.push(message);
                        }
                    }
                    if let Some(mut interaction) = maybe_interaction {
                        if response.has_focus() {
                            *interaction = Interaction::Clicked;
                        } else if response.hovered() {
                            *interaction = Interaction::Hovered;
                        } else {
                            *interaction = Interaction::None;
                        }
                    }
                });
        }
    }
}
