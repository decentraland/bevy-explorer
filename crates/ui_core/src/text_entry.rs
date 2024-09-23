use crate::{
    dui_utils::PropsExt,
    focus::{BlockKeyboard, Focusable},
    text_size::FontSize,
    ui_actions::{DataChanged, Defocus, On, Submit, UiCaller},
};
use bevy::{
    prelude::*,
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiRegistry, DuiTemplate};
use bevy_simple_text_input::{
    TextInputBundle, TextInputInactive, TextInputPlaceholder, TextInputPlugin,
    TextInputSelectionStyle, TextInputSettings, TextInputSubmitEvent, TextInputSystem,
    TextInputTextStyle, TextInputValue,
};
use common::sets::SceneSets;

use super::focus::Focus;

#[derive(Component)]
pub struct TextEntry {
    pub text_style: Option<TextStyle>,
    pub hint_text: String,
    pub hint_text_color: Option<Color>,
    pub content: String,
    pub enabled: bool,
    pub accept_line: bool,
    pub multiline: usize,
    pub retain_focus_on_submit: bool,
}

impl Default for TextEntry {
    fn default() -> Self {
        Self {
            text_style: None,
            hint_text: Default::default(),
            hint_text_color: None,
            content: Default::default(),
            enabled: true,
            accept_line: true,
            multiline: 1,
            retain_focus_on_submit: false,
        }
    }
}

pub struct TextEntryPlugin;

impl Plugin for TextEntryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TextInputPlugin);
        app.add_systems(Startup, setup).add_systems(
            Update,
            (
                update_text_entry_components,
                pipe_events,
                propagate_focus,
                update_fontsize,
            )
                .chain()
                .in_set(SceneSets::PostLoop)
                .after(TextInputSystem),
        );
    }
}

#[allow(clippy::type_complexity)]
pub fn update_text_entry_components(
    mut commands: Commands,
    text_entries: Query<(Entity, &TextEntry), Added<TextEntry>>,
) {
    for (entity, textbox) in text_entries.iter() {
        let text_lightness = Lcha::from(
            textbox
                .text_style
                .as_ref()
                .map(|s| s.color)
                .unwrap_or(Color::WHITE),
        )
        .lightness;
        let (select, select_bg) = if text_lightness > 0.5 {
            (Color::BLACK, Color::WHITE.with_alpha(0.85))
        } else {
            (Color::WHITE, Color::BLACK.with_alpha(0.85))
        };

        commands.entity(entity).with_children(|c| {
            let mut cmds = c.spawn((
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        min_width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                TextInputBundle {
                    settings: TextInputSettings {
                        multiline: textbox.multiline > 1,
                        retain_on_submit: !textbox.accept_line,
                        mask_character: None,
                    },
                    text_style: TextInputTextStyle(textbox.text_style.clone().unwrap_or_default()),
                    selection_style: TextInputSelectionStyle {
                        color: Some(select),
                        background: Some(select_bg),
                    },
                    inactive: TextInputInactive(true),
                    placeholder: TextInputPlaceholder {
                        value: textbox.hint_text.clone(),
                        text_style: Some(TextStyle {
                            color: textbox
                                .hint_text_color
                                .unwrap_or(Color::srgb(0.3, 0.3, 0.3)),
                            ..textbox.text_style.clone().unwrap_or_default()
                        }),
                    },
                    value: TextInputValue(textbox.content.clone()),
                    ..Default::default()
                },
                Focusable,
                On::<Focus>::new(
                    |caller: Res<UiCaller>, mut inactive: Query<&mut TextInputInactive>| {
                        inactive.get_mut(caller.0).unwrap().0 = false;
                    },
                ),
                On::<Defocus>::new(
                    |caller: Res<UiCaller>, mut inactive: Query<&mut TextInputInactive>| {
                        inactive.get_mut(caller.0).unwrap().0 = true;
                    },
                ),
                BlockKeyboard,
            ));

            if textbox.text_style.is_none() {
                cmds.insert(FontSize(0.03 / 1.3));
            }
        });
    }
}

pub fn update_fontsize(
    mut q: Query<(&mut TextInputTextStyle, Ref<FontSize>)>,
    mut resized: EventReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let resized = resized.read().last().is_some();
    let Ok(window) = window.get_single() else {
        return;
    };
    let win_size = window.width().min(window.height());
    for (mut text, size) in q.iter_mut().filter(|(_, sz)| resized || sz.is_changed()) {
        text.0.font_size = win_size * size.0;
    }
}

fn propagate_focus(
    q: Query<(&TextEntry, &Children), Changed<Focus>>,
    child: Query<Entity, With<TextInputSettings>>,
    focussed_text: Query<Entity, (With<TextInputSettings>, With<Focus>)>,
    key_events: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    for (textbox, children) in q.iter() {
        if !textbox.enabled {
            continue;
        }
        if let Some(child) = children.iter().find(|c| child.get(**c).is_ok()) {
            commands.entity(*child).insert(Focus);
        }
    }

    if let Ok(focussed_text) = focussed_text.get_single() {
        if key_events.just_pressed(KeyCode::Escape) {
            commands.entity(focussed_text).remove::<Focus>();
        }
    }
}

fn pipe_events(
    mut submit: EventReader<TextInputSubmitEvent>,
    changed: Query<(Entity, &TextInputValue), Changed<TextInputValue>>,
    parents: Query<&Parent>,
    settings: Query<&TextEntry>,
    mut commands: Commands,
) {
    for ev in submit.read() {
        debug!("{:?} submit", ev.entity);
        let Ok(parent) = parents.get(ev.entity) else {
            continue;
        };

        if let Some(mut commands) = commands.get_entity(parent.get()) {
            commands
                .try_insert(Submit)
                .insert(TextEntrySubmit(ev.value.clone()));
            debug!("{:?} submit to {}", commands.id(), ev.value);
        }

        if let Ok(settings) = settings.get(parent.get()) {
            if !settings.retain_focus_on_submit {
                debug!("{:?} defocus", ev.entity);
                commands.entity(ev.entity).remove::<Focus>();
            }
        }
    }

    for (entity, value) in changed.iter() {
        debug!("{:?} update", entity);
        if let Some(mut commands) = parents
            .get(entity)
            .ok()
            .and_then(|p| commands.get_entity(p.get()))
        {
            commands
                .try_insert(DataChanged)
                .insert(TextEntryValue(value.0.clone()));
            debug!("{:?} update to {}", commands.id(), value.0);
        }
    }
}

#[derive(Component)]
pub struct TextEntrySubmit(pub String);

#[derive(Component)]
pub struct TextEntryValue(pub String);

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("text-entry", DuiTextEntryTemplate);
}

// TODO handle screen resizing

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
            retain_focus_on_submit: props.take_as::<bool>(ctx, "retain-focus")?.unwrap_or(false),
            multiline,
            ..Default::default()
        };
        debug!("initial: {}", textentry.content);
        commands.insert(textentry);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
