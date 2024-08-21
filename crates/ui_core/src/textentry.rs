use crate::{
    combo_box::PropsExt,
    ui_actions::{DataChanged, On, Submit},
};
use bevy::{math::Vec3Swizzles, prelude::*, transform::TransformSystem, window::PrimaryWindow};
use bevy_dui::{DuiRegistry, DuiTemplate};
use bevy_egui::{
    egui::{self, TextEdit},
    EguiContext, EguiSet,
};
use common::util::ModifyComponentExt;

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
    pub multiline: usize,
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
            multiline: 1,
        }
    }
}

pub struct TextEntryPlugin;

impl Plugin for TextEntryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            PostUpdate,
            (apply_deferred, update_text_entry_components)
                .chain()
                .after(TransformSystem::TransformPropagate)
                .before(EguiSet::ProcessOutput),
        );
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
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else {
        return;
    };
    let ctx = ctx.get_mut();

    for (entity, mut textbox, style, node, transform, maybe_interaction, maybe_focus) in
        text_entries.iter_mut()
    {
        let margin = if textbox.multiline > 1 { 5.0 } else { 0.0 };
        let center = transform.translation().xy() / ctx.zoom_factor();
        let size = node.unrounded_size() / ctx.zoom_factor() - Vec2::Y * margin;
        let topleft = center - size / 2.0;

        let id = textbox.id_entity.unwrap_or(entity);

        if matches!(style.display, Display::Flex) {
            egui::Window::new(format!("{:?}", id))
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .vscroll(textbox.multiline > 1)
                .resizable(false)
                .frame(egui::Frame::none())
                .title_bar(false)
                .show(ctx, |ui| {
                    // destructure to split borrow
                    let TextEntry {
                        ref hint_text,
                        ref mut content,
                        ref enabled,
                        ref font_size,
                        ref multiline,
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

                    let response = match multiline {
                        1 => ui.add_enabled(
                            enabled,
                            TextEdit::singleline(content)
                                .frame(false)
                                .desired_width(f32::INFINITY)
                                .text_color(egui::Color32::WHITE)
                                .hint_text(hint_text)
                                .font(egui::FontId::new(
                                    *font_size as f32,
                                    egui::FontFamily::Proportional,
                                ))
                                .id_source(id),
                        ),
                        many => ui.add_enabled(
                            enabled,
                            TextEdit::multiline(content)
                                .frame(false)
                                .desired_width(f32::INFINITY)
                                .desired_rows(*many)
                                .text_color(egui::Color32::WHITE)
                                .hint_text(hint_text)
                                .font(egui::FontId::new(
                                    *font_size as f32,
                                    egui::FontFamily::Proportional,
                                ))
                                .id_source(id),
                        ),
                    };

                    if response.changed() && !textbox.accept_line {
                        debug!("change on {:?}", entity);
                        commands.entity(entity).insert(DataChanged);
                    }

                    // pass through focus and interaction
                    let mut defocus = false;
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if textbox.accept_line && !textbox.content.is_empty() {
                            let message = std::mem::take(&mut textbox.content);
                            response.request_focus();
                            textbox.messages.push(message);
                            commands.entity(entity).try_insert(DataChanged);
                            debug!("accept -> stash {:?}", entity);
                        } else {
                            commands.entity(entity).remove::<Focus>();
                            defocus = true;
                            debug!("accept -> defocus {:?}", entity);
                        }
                        debug!("submit on {:?}", entity);
                        commands.entity(entity).try_insert(Submit);
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
                        if response.gained_focus() {
                            debug!("tb gained focus -> Focus");
                            commands.entity(entity).try_insert(Focus);
                        } else if response.has_focus() {
                            debug!("tb focus -> Focus? na");
                            response.surrender_focus();
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
        let multiline = props.take_as::<u32>(ctx, "multi-line")?.unwrap_or(1) as usize;

        let textentry = TextEntry {
            hint_text: props.take::<String>("hint-text")?.unwrap_or_default(),
            content: props.take::<String>("initial-text")?.unwrap_or_default(),
            enabled: !(props.take_as::<bool>(ctx, "disabled")?.unwrap_or(false)),
            accept_line: props.take_as::<bool>(ctx, "accept-line")?.unwrap_or(false),
            multiline,
            ..Default::default()
        };
        commands.insert(textentry);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        commands.modify_component(move |s: &mut Style| {
            s.height = Val::VMin(2.3 * multiline as f32);
        });

        Ok(Default::default())
    }
}
