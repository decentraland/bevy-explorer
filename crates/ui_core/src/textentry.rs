use crate::{
    combo_box::PropsExt,
    ui_actions::{DataChanged, On},
};
use bevy::{math::Vec3Swizzles, prelude::*, utils::HashSet, window::PrimaryWindow};
use bevy_dui::{DuiRegistry, DuiTemplate};
use bevy_egui::{
    egui::{self, TextEdit},
    EguiContext,
};

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

pub struct TextEntryPlugin;

impl Plugin for TextEntryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_text_entry_components);
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
    let Ok(mut ctx) = egui_ctx.get_single_mut() else {
        return;
    };
    let ctx = ctx.get_mut();

    let lost_focus = lost_focus.read().collect::<HashSet<_>>();

    for (entity, mut textbox, style, node, transform, maybe_interaction, maybe_focus) in
        text_entries.iter_mut()
    {
        let center = transform.translation().xy() / ctx.zoom_factor();
        let size = node.size() / ctx.zoom_factor();
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

                    let style = ui.style_mut();
                    style.visuals.widgets.active.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 25);
                    style.visuals.widgets.hovered.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50);
                    style.visuals.widgets.inactive.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);

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

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("text-entry", DuiTextEntryTemplate);
}

pub struct DuiTextEntryTemplate;

impl DuiTemplate for DuiTextEntryTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let textentry = TextEntry {
            hint_text: props.take::<String>("hint-text")?.unwrap_or_default(),
            content: props.take::<String>("initial-text")?.unwrap_or_default(),
            accept_line: props.take_as::<bool>(ctx, "disabled")?.unwrap_or(false),
            ..Default::default()
        };
        commands.insert(textentry);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
