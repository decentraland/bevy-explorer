// automatically set style components when interaction state changes
// note: active must be added, this is a bit rubbish
// todo: add more components (maybe make the component generic and change to InteractStylePlugin<T> ?)
use bevy::prelude::*;

use crate::{nine_slice::Ui9Slice, ui_actions::Enabled};

#[derive(Clone, Default, Debug)]
pub struct InteractStyle {
    pub background: Option<Color>,
    pub image: Option<Handle<Image>>,
}

#[derive(Component, Clone, Default)]
pub struct InteractStyles {
    pub active: Option<InteractStyle>,
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
fn set_interaction_style(
    mut q: Query<
        (
            Entity,
            &InteractStyles,
            Option<&mut BackgroundColor>,
            Option<&mut Ui9Slice>,
            Option<&mut UiImage>,
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
        maybe_nineslice,
        maybe_image,
        maybe_interaction,
        maybe_active,
        maybe_enabled,
    ) in q.iter_mut()
    {
        let style = if !maybe_enabled.map_or(true, |enabled| enabled.0) {
            &styles.disabled
        } else if maybe_active.map_or(false, |active| active.0) {
            &styles.active
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
                nineslice.tint = Some(BackgroundColor(req_bg));
            }

            if let Some(image) = &style.image {
                nineslice.image = image.clone();
            }
        } else {
            if let (Some(mut bg), Some(req_bg)) = (maybe_bg, style.background) {
                *bg = BackgroundColor(req_bg);
            }

            if let (Some(mut ui_image), Some(image)) = (maybe_image, &style.image) {
                ui_image.texture = image.clone();
            }
        }
    }
}
