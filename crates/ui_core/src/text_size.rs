use bevy::{
    prelude::*,
    text::LineBreak,
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiEntityCommandsExt, DuiProps, DuiRegistry, DuiTemplate};
use bevy_egui::EguiContextSettings;
use common::util::ModifyComponentExt;

use crate::{
    dui_utils::PropsExt,
    ui_actions::{Click, HoverEnter, HoverExit, On},
    /*    user_font, FontName, WeightName, */
};

pub struct TextSizePlugin;

impl Plugin for TextSizePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_fontsize);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("small-text", TextTemplate(0.02 / 1.3));
    dui.register_template("smallish-text", TextTemplate(0.025 / 1.3));
    dui.register_template("med-text", TextTemplate(0.03 / 1.3));
    dui.register_template("large-text", TextTemplate(0.06 / 1.3));

    dui.register_template("small-link", LinkTemplate(0.02 / 1.3));
    dui.register_template("med-link", LinkTemplate(0.03 / 1.3));
    dui.register_template("large-link", LinkTemplate(0.06 / 1.3));
}

pub struct TextTemplate(f32);

impl DuiTemplate for TextTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        commands.insert(FontSize(self.0));
        let wrap = props.take_as::<bool>(ctx, "wrap")?.unwrap_or(true);
        commands.modify_component(move |text: &mut TextLayout| {
            text.linebreak = if wrap {
                LineBreak::WordBoundary
            } else {
                LineBreak::NoWrap
            };
        });

        Ok(Default::default())
    }
}

pub struct LinkTemplate(f32);

impl DuiTemplate for LinkTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let label = props.take::<String>("label").unwrap().unwrap();
        let link = props.take::<String>("href").unwrap().unwrap();
        let color = props
            .take_as::<Color>(ctx, "color")?
            .unwrap_or(Color::srgba(0.8, 0.8, 0.8, 1.0));

        let components = commands
            .apply_template(
                ctx.registry(),
                "link-base",
                DuiProps::default()
                    .with_prop("label", label)
                    .with_prop("color", color),
            )
            .unwrap();

        let line = components.named("line");
        let label = components.named("label");
        commands.commands().entity(components.root).insert((
            Interaction::default(),
            On::<Click>::new(move || {
                opener::open(&link).unwrap();
            }),
            On::<HoverEnter>::new(
                move |mut bg: Query<&mut BackgroundColor>, mut t: Query<&mut TextColor>| {
                    bg.get_mut(line).unwrap().0 = Color::WHITE;
                    t.get_mut(label).unwrap().0 = Color::WHITE;
                },
            ),
            On::<HoverExit>::new(
                move |mut bg: Query<&mut BackgroundColor>, mut t: Query<&mut TextColor>| {
                    bg.get_mut(line).unwrap().0 = color;
                    t.get_mut(label).unwrap().0 = color;
                },
            ),
        ));

        commands.commands().entity(label).insert(FontSize(self.0));
        commands
            .commands()
            .entity(line)
            .insert(BackgroundColor(color));

        Ok(Default::default())
    }
}

#[derive(Component)]
pub struct FontSize(pub f32);

pub fn update_fontsize(
    mut q: Query<(&mut TextFont, Ref<FontSize>)>,
    mut resized: EventReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut egui_settings: Query<&mut EguiContextSettings>,
) {
    let resized = resized.read().last().is_some();
    let Ok(window) = window.single() else {
        return;
    };
    let win_size = window.width().min(window.height());
    if win_size <= 0.0 {
        return;
    }
    for (mut text_font, size) in q.iter_mut().filter(|(_, sz)| resized || sz.is_changed()) {
        text_font.font_size = win_size * size.0;
    }
    if resized && win_size > 0.0 {
        for mut ctx in egui_settings.iter_mut() {
            ctx.scale_factor = win_size / 720.0;
        }
    }
}
/*
pub fn make_text_section(
    text: &str,
    font_name: FontName,
    font_size: f32,
    color: Color,
) -> impl Iterator<Item = impl Bundle> {
    let text = text.replace("\\n", "\n");

    // split by <b>s and <i>s
    let mut b_count = 0usize;
    let mut i_count = 0usize;
    let mut b_offset = text.find("<b>");
    let mut i_offset = text.find("<i>");
    let mut xb_offset = text.find("</b>");
    let mut xi_offset = text.find("</i>");
    let mut section_start = 0;

    let mut sections = Vec::default();

    loop {
        let section_end = [b_offset, i_offset, xb_offset, xi_offset]
            .iter()
            .fold(usize::MAX, |c, o| c.min(o.unwrap_or(c)));
        let weight = match (b_count, i_count) {
            (0, 0) => WeightName::Regular,
            (0, _) => WeightName::Italic,
            (_, 0) => WeightName::Bold,
            (_, _) => WeightName::BoldItalic,
        };

        if section_end == usize::MAX {
            sections.push((
                TextSpan::new(&text[section_start..]),
                TextFont {
                    font: user_font(font_name, weight),
                    font_size,
                    ..Default::default()
                },
                TextColor(color),
            ));
            break;
        }

        sections.push((
            TextSpan::new(&text[section_start..section_end]),
            TextFont {
                font: user_font(font_name, weight),
                font_size,
                ..Default::default()
            },
            TextColor(color),
        ));

        match &text[section_end..section_end + 3] {
            "<b>" => {
                b_count += 1;
                b_offset = text[section_end + 1..]
                    .find("<b>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 3;
            }
            "<i>" => {
                i_count += 1;
                i_offset = text[section_end + 1..]
                    .find("<i>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 3;
            }
            "</b" => {
                b_count = b_count.saturating_sub(1);
                xb_offset = text[section_end + 1..]
                    .find("</b>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 4;
            }
            "</i" => {
                i_count = i_count.saturating_sub(1);
                xi_offset = text[section_end + 1..]
                    .find("</i>")
                    .map(|v| v + section_end + 1);
                section_start = section_end + 4;
            }
            _ => {
                error!("{}", &text[section_end..=section_end + 2]);
                panic!()
            }
        }
    }

    sections.into_iter()
}
*/
