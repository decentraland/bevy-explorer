// automatically set style components when interaction state changes
// note: active must be added, this is a bit rubbish
// todo: add more components (maybe make the component generic and change to InteractStylePlugin<T> ?)
use bevy::prelude::*;

#[derive(Clone)]
pub struct InteractStyle {
    pub background: Option<Color>,
}

#[derive(Component, Clone)]
pub struct InteractStyles {
    pub active: InteractStyle,
    pub hover: InteractStyle,
    pub inactive: InteractStyle,
}

#[derive(Component)]
pub struct Active(pub bool);

pub struct InteractStylePlugin;

impl Plugin for InteractStylePlugin {
    fn build(&self, app: &mut App) {
        app.add_system(set_interaction_style);
    }
}

#[allow(clippy::type_complexity)]
fn set_interaction_style(
    mut q: Query<
        (
            &InteractStyles,
            Option<&mut BackgroundColor>,
            Option<&mut Style>,
            Option<&Interaction>,
            Option<&Active>,
        ),
        Or<(Changed<Active>, Changed<Interaction>)>,
    >,
) {
    for (styles, maybe_bg, _maybe_style, maybe_interaction, active) in q.iter_mut() {
        let style = if active.map_or(false, |active| active.0) {
            &styles.active
        } else if maybe_interaction == Some(&Interaction::Hovered) {
            &styles.hover
        } else {
            &styles.inactive
        };

        if let (Some(mut bg), Some(req_bg)) = (maybe_bg, style.background) {
            *bg = BackgroundColor(req_bg);
        }
    }
}
