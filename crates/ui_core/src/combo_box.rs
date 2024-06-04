use std::{any::type_name, str::FromStr};

use anyhow::anyhow;
use bevy::{math::Vec3Swizzles, prelude::*, transform::TransformSystem, window::PrimaryWindow};
use bevy_dui::{DuiContext, DuiProps, DuiRegistry, DuiTemplate};
use bevy_egui::{egui, EguiContext};

use crate::{
    ui_actions::{DataChanged, On},
    Blocker,
};

#[derive(Component, Debug)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
    pub id_entity: Option<Entity>,
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
            id_entity: None,
        }
    }

    pub fn with_id(self, entity: Entity) -> Self {
        Self {
            id_entity: Some(entity),
            ..self
        }
    }

    pub fn selected(&self) -> Option<&String> {
        if self.selected == -1 {
            None
        } else {
            self.options.get(self.selected as usize)
        }
    }
}

pub struct ComboBoxPlugin;

impl Plugin for ComboBoxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            PostUpdate,
            update_comboboxen.after(TransformSystem::TransformPropagate),
        );
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("combo-box", DuiComboBoxTemplate);
}

#[allow(clippy::type_complexity)]
fn update_comboboxen(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut combos: Query<(Entity, &mut ComboBox, &Style, &Node, &GlobalTransform), Without<Blocker>>,
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
                    z_index: ZIndex::Global(100),
                    ..Default::default()
                },
                Blocker,
            ))
            .id()
    });
    let mut popup_active = false;

    for (entity, mut combo, style, node, transform) in combos.iter_mut() {
        let center = transform.translation().xy() / ctx.zoom_factor();
        let size = node.size() / ctx.zoom_factor();
        let topleft = center - size / 2.0;

        if matches!(style.display, Display::Flex) {
            let id = format!("{:?}", combo.id_entity.unwrap_or(entity));

            egui::Window::new(id.clone())
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .frame(egui::Frame::none())
                .title_bar(false)
                .enabled(!combo.disabled)
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
                    style.visuals.widgets.active.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 25);
                    style.visuals.widgets.hovered.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50);
                    style.visuals.widgets.inactive.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);

                    egui::ComboBox::from_id_source(id)
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

                    if ui.memory(|mem| mem.any_popup_open()) {
                        popup_active = true;
                    }

                    if combo.selected != initial_selection || combo.selected == -1 {
                        if combo.selected == -1 {
                            combo.selected = 0;
                        }
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

pub struct DuiComboBoxTemplate;

impl DuiTemplate for DuiComboBoxTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let combobox = ComboBox {
            empty_text: props.take::<String>("empty-text")?.unwrap_or_default(),
            options: props
                .take::<Vec<String>>("options")?
                .ok_or(anyhow!("no options for combobox"))?,
            selected: props.take::<isize>("selected")?.unwrap_or(-1),
            allow_null: props.take_as::<bool>(ctx, "allow-null")?.unwrap_or(false),
            disabled: props.take_as::<bool>(ctx, "disabled")?.unwrap_or(false),
            id_entity: None,
        };
        commands.insert(combobox);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}

pub trait DuiFromStr {
    fn from_str(ctx: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

macro_rules! impl_dui_str {
    ($T:ty) => {
        impl<'a> DuiFromStr for $T {
            fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error> {
                <Self as FromStr>::from_str(value)
                    .map_err(|_| anyhow!("failed to convert `{value}` to {}", type_name::<$T>()))
            }
        }
    };
}

impl_dui_str!(bool);
impl_dui_str!(u32);
impl_dui_str!(usize);

impl DuiFromStr for Val {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .val()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Val"))
    }
}

impl DuiFromStr for UiRect {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .rect()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Rect"))
    }
}

impl DuiFromStr for Color {
    fn from_str(_: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{a: {value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let Some(prop_value) = rule.properties.values().next() else {
            anyhow::bail!("no value?");
        };

        prop_value
            .color()
            .ok_or_else(|| anyhow!("failed to parse `{value}` as Color"))
    }
}

impl<T: Asset> DuiFromStr for Handle<T> {
    fn from_str(ctx: &DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        Ok(ctx.asset_server().load::<T>(value.to_owned()))
    }
}

pub trait PropsExt {
    fn take_as<T: DuiFromStr + 'static>(
        &mut self,
        ctx: &DuiContext,
        label: &str,
    ) -> Result<Option<T>, anyhow::Error>;
}

impl PropsExt for DuiProps {
    fn take_as<T: DuiFromStr + 'static>(
        &mut self,
        ctx: &DuiContext,
        label: &str,
    ) -> Result<Option<T>, anyhow::Error> {
        if let Ok(value) = self.take::<T>(label) {
            return Ok(value);
        }

        if let Ok(Some(value)) = self.take::<String>(label) {
            Ok(Some(<T as DuiFromStr>::from_str(ctx, &value)?))
        } else {
            Err(anyhow!("unrecognised type for key `{label}`"))
        }
    }
}
