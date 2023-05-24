use bevy::{math::Vec3Swizzles, prelude::*};
use bevy_egui::{
    egui::{self, TextEdit},
    EguiContexts,
};

#[derive(Component, Default)]
pub struct TextBox {
    content: String,
    pub messages: Vec<String>,
}

pub fn update_textboxes(
    mut contexts: EguiContexts,
    mut q: Query<(Entity, &mut TextBox, &Node, &GlobalTransform)>,
) {
    let ctx = contexts.ctx_mut();
    for (entity, mut textbox, node, transform) in q.iter_mut() {
        let center = transform.translation().xy();
        let size = node.size();
        let topleft = center - size / 2.0;

        egui::Window::new(format!("{entity:?}"))
            .fixed_pos(topleft.to_array())
            .fixed_size(size.to_array())
            .title_bar(false)
            .show(ctx, |ui| {
                let response = ui.add(
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
            });
    }
}
