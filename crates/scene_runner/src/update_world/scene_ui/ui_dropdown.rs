use bevy::prelude::*;
use common::util::ModifyComponentExt;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{self, PbUiDropdown, PbUiDropdownResult},
    SceneComponentId,
};
use ui_core::{
    combo_box::ComboBox,
    ui_actions::{DataChanged, On},
    user_font, FontName,
};

use crate::{renderer_context::RendererSceneContext, SceneEntity};

use super::UiLink;

#[derive(Component, Debug)]
pub struct UiDropdown(PbUiDropdown);

impl From<PbUiDropdown> for UiDropdown {
    fn from(value: PbUiDropdown) -> Self {
        Self(value)
    }
}

pub fn set_ui_dropdown(
    mut commands: Commands,
    dropdowns: Query<(&SceneEntity, &UiDropdown, &UiLink), Or<(Added<UiDropdown>, Added<UiLink>)>>,
    mut removed: RemovedComponents<UiDropdown>,
    links: Query<&UiLink>,
) {
    for ent in removed.read() {
        if let Ok(link) = links.get(ent) {
            if let Some(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<ComboBox>();
            }
        }
    }

    for (scene_ent, dropdown, link) in dropdowns.iter() {
        let Some(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let initial_selection = if dropdown.0.accept_empty {
            dropdown.0.selected_index.map(|ix| ix as isize)
        } else {
            Some(dropdown.0.selected_index.unwrap_or(0) as isize)
        };

        commands.modify_component(|style: &mut Style| {
            //ensure we use max width if not given
            if style.width == Val::Px(0.0) || style.width == Val::Auto {
                style.width = Val::Percent(100.0);
            }
            //     //and some size if not given
            //     if style.height == Val::Px(0.0) || style.height == Val::Auto {
            //         style.height = Val::Px(16.0);
            //     }
        });

        let font_name = match dropdown.0.font() {
            components::common::Font::FSansSerif => FontName::Serif,
            components::common::Font::FSerif => FontName::Sans,
            components::common::Font::FMonospace => FontName::Mono,
        };
        let font_size = dropdown.0.font_size.unwrap_or(10) as f32;

        let root = scene_ent.root;
        let ui_entity = link.ui_entity;
        let scene_id = scene_ent.id;
        commands.try_insert((
            ComboBox::new(
                dropdown.0.empty_label.clone().unwrap_or_default(),
                &dropdown.0.options,
                dropdown.0.accept_empty,
                dropdown.0.disabled,
                initial_selection,
                Some(TextStyle {
                    font: user_font(font_name, ui_core::WeightName::Regular),
                    font_size,
                    color: dropdown.0.color.map(Into::into).unwrap_or(Color::BLACK),
                }),
            ),
            On::<DataChanged>::new(
                move |combo: Query<(Entity, &ComboBox)>,
                      mut context: Query<&mut RendererSceneContext>,
                      time: Res<Time>| {
                    let Ok((_, combo)) = combo.get(ui_entity) else {
                        warn!("failed to get combo node on UiDropdown update");
                        return;
                    };
                    let Ok(mut context) = context.get_mut(root) else {
                        warn!("failed to get context on UiInput update");
                        return;
                    };

                    context.update_crdt(
                        SceneComponentId::UI_DROPDOWN_RESULT,
                        CrdtType::LWW_ENT,
                        scene_id,
                        &PbUiDropdownResult {
                            value: combo.selected as i32,
                        },
                    );
                    context.last_action_event = Some(time.elapsed_seconds());
                },
            ),
        ));
    }
}
