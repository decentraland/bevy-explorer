use bevy::{prelude::*, ui::FocusPolicy};
use common::util::ModifyComponentExt;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        sdk::components::{self, PbUiInput, PbUiInputResult},
        Color4DclToBevy,
    },
    SceneComponentId,
};
use ui_core::{
    text_entry::{TextEntry, TextEntrySubmit, TextEntryValue},
    ui_actions::{DataChanged, On, Submit, UiCaller},
    user_font, FontName,
};

use crate::{renderer_context::RendererSceneContext, SceneEntity};

use super::UiLink;

#[derive(Component, Debug)]
pub struct UiInput(PbUiInput);

impl From<PbUiInput> for UiInput {
    fn from(value: PbUiInput) -> Self {
        Self(value)
    }
}

pub fn set_ui_input(
    mut commands: Commands,
    mut inputs: Query<
        (&SceneEntity, &UiInput, &mut UiLink),
        Or<(Changed<UiInput>, Changed<UiLink>)>,
    >,
    mut removed: RemovedComponents<UiInput>,
    mut links: Query<&mut UiLink, Without<UiInput>>,
) {
    for ent in removed.read() {
        if let Ok(mut link) = links.get_mut(ent) {
            link.interactors.remove("input");
            if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<TextEntry>();
            }
        }
    }

    for (scene_ent, input, mut link) in inputs.iter_mut() {
        let Ok(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let font_name = match input.0.font() {
            components::common::Font::FSansSerif => FontName::Sans,
            components::common::Font::FSerif => FontName::Serif,
            components::common::Font::FMonospace => FontName::Mono,
        };
        let font_size = input.0.font_size.unwrap_or(10).max(1) as f32;

        let ui_entity = link.ui_entity;
        let root = scene_ent.root;
        let scene_id = scene_ent.id;

        let data_handler = move |In(submit): In<bool>,
                                 change_entry: Query<&TextEntryValue>,
                                 submit_entry: Query<&TextEntrySubmit>,
                                 mut context: Query<&mut RendererSceneContext>,
                                 time: Res<Time>,
                                 caller: Res<UiCaller>| {
            debug!("callback on {:?} (submit = {})", caller.0, submit);
            let value = if submit {
                submit_entry.get(ui_entity).map(|v| v.0.clone())
            } else {
                change_entry.get(ui_entity).map(|v| v.0.clone())
            };

            let Ok(value) = value else {
                warn!("failed to get text node on UiInput update");
                return;
            };
            let Ok(mut context) = context.get_mut(root) else {
                warn!("failed to get context on UiInput update");
                return;
            };

            context.update_crdt(
                SceneComponentId::UI_INPUT_RESULT,
                CrdtType::LWW_ENT,
                scene_id,
                &PbUiInputResult {
                    value,
                    is_submit: Some(submit),
                },
            );
            context.last_action_event = Some(time.elapsed_secs());
        };

        commands.modify_component(move |style: &mut Node| {
            //ensure we use max width if not given
            if style.width == Val::Px(0.0) {
                style.width = Val::Percent(100.0);
            }
            //and some size if not given
            if style.height == Val::Px(0.0) {
                style.height = Val::Px(font_size * 1.3);
            }
        });

        commands.try_insert((
            FocusPolicy::Block,
            Interaction::default(),
            TextEntry {
                hint_text: input.0.placeholder.to_owned(),
                hint_text_color: input
                    .0
                    .placeholder_color
                    .map(Color4DclToBevy::convert_srgba),
                enabled: !input.0.disabled,
                content: input.0.value.clone().unwrap_or_default(),
                accept_line: true,
                text_style: Some((
                    TextFont {
                        font: user_font(font_name, ui_core::WeightName::Regular),
                        font_size,
                        ..Default::default()
                    },
                    TextColor(
                        input
                            .0
                            .color
                            .map(Color4DclToBevy::convert_srgba)
                            .unwrap_or(Color::BLACK),
                    ),
                )),
                ..Default::default()
            },
            On::<DataChanged>::new((|| false).pipe(data_handler)),
            On::<Submit>::new((|| true).pipe(data_handler)),
        ));

        link.interactors.insert("input");
    }
}
