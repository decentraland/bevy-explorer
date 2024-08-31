// automatically set style components when interaction state changes
// note: active must be added, this is a bit rubbish
// todo: add more components (maybe make the component generic and change to InteractStylePlugin<T> ?)
use bevy::prelude::*;

use crate::{
    bound_node::{BoundedNode, NodeBounds},
    dui_utils::DuiFromStr,
    nine_slice::Ui9Slice,
    ui_actions::Enabled,
};

#[derive(Clone, Default, Debug)]
pub struct InteractStyle {
    pub background: Option<Color>,
    pub image: Option<Handle<Image>>,
    pub border: Option<Color>,
}

#[derive(Component, Clone, Default, Debug)]
pub struct InteractStyles {
    pub active: Option<InteractStyle>,
    pub press: Option<InteractStyle>,
    pub hover: Option<InteractStyle>,
    pub inactive: Option<InteractStyle>,
    pub disabled: Option<InteractStyle>,
}

#[derive(Component)]
pub struct Active(pub bool);

pub struct InteractStylePlugin;

impl Plugin for InteractStylePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_interaction_style);
    }
}

#[allow(clippy::type_complexity)]
pub fn set_interaction_style(
    mut q: Query<
        (
            Entity,
            &InteractStyles,
            Option<&mut BackgroundColor>,
            Option<&mut BorderColor>,
            Option<&mut Ui9Slice>,
            Option<&mut UiImage>,
            Option<&mut BoundedNode>,
            Option<&mut NodeBounds>,
            Option<&Interaction>,
            Option<&Active>,
            Option<&Enabled>,
        ),
        Or<(
            Changed<Active>,
            Changed<Interaction>,
            Changed<Enabled>,
            Changed<InteractStyles>,
        )>,
    >,
) {
    for (
        _ent,
        styles,
        maybe_bg,
        maybe_border,
        maybe_nineslice,
        maybe_image,
        maybe_bounded,
        maybe_bounds,
        maybe_interaction,
        maybe_active,
        maybe_enabled,
    ) in q.iter_mut()
    {
        let style = if !maybe_enabled.map_or(true, |enabled| enabled.0) {
            &styles.disabled
        } else if maybe_active.map_or(false, |active| active.0) {
            &styles.active
        } else if maybe_interaction == Some(&Interaction::Pressed) {
            &styles.press
        } else if maybe_interaction == Some(&Interaction::Hovered) {
            &styles.hover
        } else {
            &styles.inactive
        };

        let Some(style) = style else {
            continue;
        };

        if let Some(mut nineslice) = maybe_nineslice {
            if let Some(req_bg) = style.background {
                nineslice.tint = Some(req_bg);
            }

            if let Some(image) = &style.image {
                nineslice.image = image.clone();
            }
        }

        if let Some(mut bounded) = maybe_bounded {
            if let Some(req_bg) = style.background {
                bounded.color = Some(req_bg);
            }
            if let Some(image) = &style.image {
                bounded.image = Some(image.clone());
            }
        }

        if let Some(mut bounds) = maybe_bounds {
            if let Some(border) = style.border {
                bounds.border_color = border;
            }
        }

        if let Some(mut ui_image) = maybe_image {
            if let Some(image) = &style.image {
                ui_image.texture = image.clone();
            }
            if let Some(req_bg) = style.background {
                ui_image.color = req_bg;
            }
        } else if let Some(mut bg) = maybe_bg {
            if let Some(req_bg) = style.background {
                bg.0 = req_bg;
            }
        }

        if let Some(mut border) = maybe_border {
            if let Some(border_color) = style.border {
                border.0 = border_color;
            }
        }
    }
}

impl DuiFromStr for InteractStyles {
    fn from_str(_: &bevy_dui::DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{{value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        let res = Self {
            active: rule
                .properties
                .get("active")
                .and_then(|v| v.color())
                .map(|c| InteractStyle {
                    background: Some(c),
                    ..Default::default()
                }),
            hover: rule
                .properties
                .get("hover")
                .and_then(|v| v.color())
                .map(|c| InteractStyle {
                    background: Some(c),
                    ..Default::default()
                }),
            press: rule
                .properties
                .get("press")
                .and_then(|v| v.color())
                .map(|c| InteractStyle {
                    background: Some(c),
                    ..Default::default()
                }),
            inactive: rule
                .properties
                .get("inactive")
                .and_then(|v| v.color())
                .map(|c| InteractStyle {
                    background: Some(c),
                    ..Default::default()
                }),
            disabled: rule
                .properties
                .get("disabled")
                .and_then(|v| v.color())
                .map(|c| InteractStyle {
                    background: Some(c),
                    ..Default::default()
                }),
        };
        Ok(res)
    }
}
