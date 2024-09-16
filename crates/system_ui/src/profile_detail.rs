use bevy::{color::palettes::css, prelude::*};
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{profile::SerializedProfile, structs::SettingsTab};
use comms::profile::CurrentUserProfile;
use ui_core::{
    button::{DuiButton, TabSelection},
    focus::Focus,
    interact_style::{InteractStyle, InteractStyles},
    text_entry::TextEntryValue,
    ui_actions::{DataChanged, On, UiCaller},
};

use crate::profile::SettingsDialog;

pub struct ProfileDetailPlugin;

impl Plugin for ProfileDetailPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_profile_detail_content);
    }
}

#[derive(Component)]
pub struct ProfileDetail(pub SerializedProfile);

#[allow(clippy::type_complexity)]
fn set_profile_detail_content(
    mut commands: Commands,
    dialog: Query<(Entity, Option<&ProfileDetail>), With<SettingsDialog>>,
    q: Query<(Entity, &SettingsTab), Changed<SettingsTab>>,
    current_profile: Res<CurrentUserProfile>,
    mut prev_tab: Local<Option<SettingsTab>>,
    dui: Res<DuiRegistry>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab) in q.iter() {
        let Ok((settings_entity, maybe_detail)) = dialog.get_single() else {
            return;
        };

        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::ProfileDetail {
            return;
        }

        let default = SerializedProfile::default();
        let detail = maybe_detail.map(|d| &d.0).unwrap_or_else(|| {
            if let Some(profile) = current_profile.profile.as_ref() {
                commands
                    .entity(settings_entity)
                    .insert(ProfileDetail(profile.content.clone()));
                &profile.content
            } else {
                error!("can't amend missing profile");
                &default
            }
        });

        macro_rules! category {
            ($label: expr, $init:expr, $multiline:expr, $set:expr) => {
                commands.spawn_template(
                    &dui,
                    "profile-detail-category",
                    DuiProps::new()
                        .with_prop("label", $label.to_owned())
                        .with_prop("initial", $init.clone())
                        .with_prop("multi-line", $multiline)
                        .with_prop("onchanged", On::<DataChanged>::new(|caller: Res<UiCaller>, data: Query<&TextEntryValue>, mut profile: Query<&mut ProfileDetail>, mut settings: Query<&mut SettingsDialog>| {
                            let Ok(data) = data.get(caller.0) else {
                                warn!("no entry");
                                return;
                            };
                            let Ok(mut profile) = profile.get_single_mut() else {
                                warn!("no profile");
                                return;
                            };
                            let Ok(mut settings) = settings.get_single_mut() else {
                                warn!("no settings");
                                return;
                            };
                            #[allow(clippy::redundant_closure_call)]
                            $set(&mut profile.0, data.0.clone());
                            settings.modified = true;
                        }))
                ).unwrap()
            }
        }

        let cat_items = vec![
            category!(
                "Name",
                detail.name,
                1u32,
                |p: &mut SerializedProfile, d: String| p.name = d
            ),
            category!(
                "Description",
                detail.description,
                10u32,
                |p: &mut SerializedProfile, d: String| p.description = d
            ),
            category!(
                "Email",
                detail.email.clone().unwrap_or_default(),
                1u32,
                |p: &mut SerializedProfile, d: String| p.email = Some(d)
            ),
        ];

        macro_rules! cat_button {
            ($label:expr, $enabled:expr) => {{
                DuiButton {
                    styles: Some(InteractStyles {
                        active: Some(InteractStyle {
                            background: Some(css::ORANGE.into()),
                            border: Some(Color::BLACK),
                            ..Default::default()
                        }),
                        inactive: Some(InteractStyle {
                            background: Some(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                            border: Some(Color::NONE),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    label: Some($label.to_owned()),
                    enabled: $enabled,
                    ..Default::default()
                }
            }};
        }

        let category_tabs = vec![
            cat_button!("Name", true),
            cat_button!("Description", true),
            cat_button!("Email", true),
            cat_button!("Blocked", false),
            cat_button!("Muted", false),
            cat_button!("Interests", false),
        ];

        let cat_items_sel = cat_items.clone();
        let components = commands
            .entity(ent)
            .despawn_descendants()
            .apply_template(
                &dui,
                "profile-detail",
                DuiProps::new()
                    .with_prop("category-tabs", category_tabs)
                    .with_prop(
                        "category-changed",
                        On::<DataChanged>::new(
                            move |mut commands: Commands,
                                  caller: Res<UiCaller>,
                                  tab: Query<&TabSelection>| {
                                let Ok(selection) = tab.get(caller.0) else {
                                    warn!("failed to get tab");
                                    return;
                                };
                                let Some(selected) = selection.selected else {
                                    return;
                                };

                                if let Some(mut commands) = cat_items_sel
                                    .get(selected)
                                    .and_then(|e| commands.get_entity(e.named("entry")))
                                {
                                    commands.try_insert(Focus);
                                }
                            },
                        ),
                    ),
            )
            .unwrap();

        commands
            .entity(components.named("items"))
            .push_children(&cat_items.iter().map(|de| de.root).collect::<Vec<_>>());
    }
}
