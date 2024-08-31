use bevy::{prelude::*, ui::FocusPolicy};
use common::util::ModifyComponentExt;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{self, PbUiInput, PbUiInputResult},
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
    inputs: Query<(&SceneEntity, &UiInput, &UiLink), Or<(Changed<UiInput>, Changed<UiLink>)>>,
    mut removed: RemovedComponents<UiInput>,
    links: Query<&UiLink>,
) {
    for ent in removed.read() {
        if let Ok(link) = links.get(ent) {
            if let Some(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<TextEntry>();
            }
        }
    }

    for (scene_ent, input, link) in inputs.iter() {
        let Some(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let font_name = match input.0.font() {
            components::common::Font::FSansSerif => FontName::Serif,
            components::common::Font::FSerif => FontName::Sans,
            components::common::Font::FMonospace => FontName::Mono,
        };
        let font_size = input.0.font_size.unwrap_or(10) as f32;

        let ui_entity = link.ui_entity;
        let root = scene_ent.root;
        let scene_id = scene_ent.id;

        let data_handler = move |In(submit): In<bool>,
                                 change_entry: Query<&TextEntryValue>,
                                 submit_entry: Query<&TextEntrySubmit>,
                                 mut context: Query<&mut RendererSceneContext>,
                                 time: Res<Time>,
                                 caller: Res<UiCaller>| {
            debug!("callback on {:?}", caller.0);
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
            context.last_action_event = Some(time.elapsed_seconds());
        };

        commands.modify_component(move |style: &mut Style| {
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
                hint_text_color: input.0.placeholder_color.map(Into::into),
                enabled: !input.0.disabled,
                content: input.0.value.clone().unwrap_or_default(),
                accept_line: false,
                text_style: Some(TextStyle {
                    font: user_font(font_name, ui_core::WeightName::Regular),
                    font_size,
                    color: input.0.color.map(Into::into).unwrap_or(Color::BLACK),
                }),
                ..Default::default()
            },
            On::<DataChanged>::new((|| false).pipe(data_handler)),
            On::<Submit>::new((|| true).pipe(data_handler)),
        ));
    }
}
