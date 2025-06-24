use bevy::prelude::*;
use common::util::ModifyComponentExt;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        sdk::components::{self, PbUiDropdown, PbUiDropdownResult},
        Color4DclToBevy,
    },
    SceneComponentId,
};
use ui_core::{
    combo_box::ComboBox,
    ui_actions::{DataChanged, On},
    user_font, FontName,
};

use crate::{renderer_context::RendererSceneContext, update_world::scene_ui::SceneUiData, SceneEntity};

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
    dropdowns: Query<
        (&SceneEntity, &UiDropdown, &UiLink),
        Or<(Changed<UiDropdown>, Changed<UiLink>)>,
    >,
    scene: Query<&SceneUiData>,
    mut removed: RemovedComponents<UiDropdown>,
    links: Query<&UiLink>,
) {
    for ent in removed.read() {
        if let Ok(link) = links.get(ent) {
            if let Ok(mut commands) = commands.get_entity(link.ui_entity) {
                commands.remove::<ComboBox>();
            }
        }
    }

    for (scene_ent, dropdown, link) in dropdowns.iter() {
        let Ok(mut commands) = commands.get_entity(link.ui_entity) else {
            continue;
        };

        let initial_selection = if dropdown.0.accept_empty {
            dropdown.0.selected_index.map(|ix| ix as isize)
        } else {
            Some(dropdown.0.selected_index.unwrap_or(0) as isize)
        };

        commands.modify_component(|style: &mut Node| {
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
            components::common::Font::FSansSerif => FontName::Sans,
            components::common::Font::FSerif => FontName::Serif,
            components::common::Font::FMonospace => FontName::Mono,
        };
        let font_size = dropdown.0.font_size.unwrap_or(10) as f32;

        let root = scene_ent.root;
        let ui_entity = link.ui_entity;
        let scene_id = scene_ent.id;

        let Ok(scene_ui_data) = scene.get(scene_ent.root) else {
            warn!("no scene ui data!");
            continue;
        };

        commands.try_insert((
            ComboBox::new_scene(
                dropdown.0.empty_label.clone().unwrap_or_default(),
                &dropdown.0.options,
                dropdown.0.accept_empty,
                dropdown.0.disabled,
                initial_selection,
                Some((
                    TextFont {
                        font: user_font(font_name, ui_core::WeightName::Regular),
                        font_size,
                        ..Default::default()
                    },
                    TextColor(
                        dropdown
                            .0
                            .color
                            .map(Color4DclToBevy::convert_srgba)
                            .unwrap_or(Color::BLACK),
                    ),
                )),
                scene_ui_data.super_user,
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
                    context.last_action_event = Some(time.elapsed_secs());
                },
            ),
        ));
    }
}
