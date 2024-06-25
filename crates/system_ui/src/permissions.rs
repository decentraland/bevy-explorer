use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    structs::{
        AppConfig, PermissionTarget, PermissionType, PermissionValue, PrimaryPlayerRes, SettingsTab,
    },
    util::FireEventEx,
};
use ipfs::CurrentRealm;
use scene_runner::{
    permissions::{PermissionLevel, PermissionStrings},
    renderer_context::RendererSceneContext,
    ContainingScene,
};
use ui_core::{
    bound_node::BoundedNode,
    interact_style::set_interaction_style,
    scrollable::{ScrollTarget, ScrollTargetEvent},
    ui_actions::{Click, HoverEnter, On, UiCaller},
    ModifyComponentExt,
};

use crate::profile::SettingsDialog;

pub struct PermissionSettingsPlugin;

impl Plugin for PermissionSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PermissionTarget>().add_systems(
            Update,
            set_permission_settings_content.before(set_interaction_style),
        );
    }
}

#[derive(Component)]
pub struct PermissionSettingsDetail(pub AppConfig);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn set_permission_settings_content(
    mut commands: Commands,
    dialog: Query<(Entity, Option<&PermissionSettingsDetail>), With<SettingsDialog>>,
    q: Query<(Entity, &SettingsTab), Changed<SettingsTab>>,
    current_settings: Res<AppConfig>,
    mut prev_tab: Local<Option<SettingsTab>>,
    dui: Res<DuiRegistry>,
    asset_server: Res<AssetServer>,
    containing_scene: ContainingScene,
    player: Res<PrimaryPlayerRes>,
    realm: Res<CurrentRealm>,
    scenes: Query<&RendererSceneContext>,
    target: Res<PermissionTarget>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab) in q.iter() {
        let Ok((settings_entity, maybe_settings)) = dialog.get_single() else {
            return;
        };

        if prev_tab.as_ref() == Some(tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Permissions {
            return;
        }

        let config = match maybe_settings {
            Some(s) => s.0.clone(),
            None => {
                commands
                    .entity(settings_entity)
                    .insert(PermissionSettingsDetail(current_settings.clone()));
                current_settings.clone()
            }
        };

        let scene_ent = match &target.scene {
            None => containing_scene.get_parcel(player.0),
            Some(scene) => {
                if scenes.get(*scene).is_ok() {
                    Some(*scene)
                } else {
                    containing_scene.get_parcel(player.0)
                }
            }
        };
        let (scene_hash, scene_name, is_portable) = scene_ent
            .and_then(|scene_ent| scenes.get(scene_ent).ok())
            .map(|ctx| {
                (
                    Some(ctx.hash.clone()),
                    Some(ctx.title.clone()),
                    ctx.is_portable,
                )
            })
            .unwrap_or((None, None, false));

        let realm_address = if is_portable {
            "<portable>".to_owned()
        } else {
            realm.address.clone()
        };

        commands.entity(ent).despawn_descendants();
        let components = commands
            .entity(ent)
            .apply_template(
                &dui,
                "permissions-tab",
                DuiProps::new()
                    .with_prop(
                        "scene",
                        scene_name.clone().unwrap_or_else(|| "<none>".to_owned()),
                    )
                    .with_prop(
                        "realm",
                        realm
                            .config
                            .realm_name
                            .clone()
                            .unwrap_or_else(|| "<unnamed>".into()),
                    ),
            )
            .unwrap();

        let spawn_setting = |props: DuiProps,
                             ty: PermissionType,
                             level: PermissionLevel,
                             enabled: bool|
         -> DuiProps {
            let current_value = match &level {
                PermissionLevel::Scene(_, hash) => config
                    .scene_permissions
                    .get(hash)
                    .and_then(|sp| sp.get(&ty))
                    .copied(),
                PermissionLevel::Realm(r) => config
                    .realm_permissions
                    .get(r)
                    .and_then(|sp| sp.get(&ty))
                    .copied(),
                PermissionLevel::Global => Some(
                    config
                        .default_permissions
                        .get(&ty)
                        .copied()
                        .unwrap_or(AppConfig::default_permission(ty)),
                ),
            };

            let image = match current_value {
                Some(PermissionValue::Allow) => "tick.png",
                Some(PermissionValue::Deny) => "redx.png",
                Some(PermissionValue::Ask) => "ask.png",
                None => "next.png",
            };

            let label = match &level {
                PermissionLevel::Scene(..) => "scene",
                PermissionLevel::Realm(_) => "realm",
                PermissionLevel::Global => "global",
            };

            props
                .with_prop(
                    format!("{label}-image"),
                    asset_server.load::<Image>(format!("images/{image}")),
                )
                .with_prop(format!("{label}-enabled"), enabled)
                .with_prop(
                    format!("{label}-click"),
                    On::<Click>::new(
                        move |mut config: Query<(
                            &mut SettingsDialog,
                            &mut PermissionSettingsDetail,
                        )>,
                              caller: Res<UiCaller>,
                              mut commands: Commands,
                              asset_server: Res<AssetServer>| {
                            let Ok((mut dialog, mut config)) = config.get_single_mut() else {
                                warn!("no config");
                                return;
                            };

                            let (current_value, dict) = match &level {
                                PermissionLevel::Scene(_, hash) => {
                                    let dict =
                                        config.0.scene_permissions.entry(hash.clone()).or_default();
                                    (dict.get(&ty).copied(), dict)
                                }
                                PermissionLevel::Realm(r) => {
                                    let dict =
                                        config.0.realm_permissions.entry(r.clone()).or_default();
                                    (dict.get(&ty).copied(), dict)
                                }
                                PermissionLevel::Global => {
                                    let dict = &mut config.0.default_permissions;
                                    (
                                        dict.get(&ty)
                                            .copied()
                                            .or(Some(AppConfig::default_permission(ty))),
                                        dict,
                                    )
                                }
                            };

                            let (next, image) = match (current_value, &level) {
                                (None, _)
                                | (Some(PermissionValue::Ask), PermissionLevel::Global) => {
                                    (Some(PermissionValue::Allow), "tick.png")
                                }
                                (Some(PermissionValue::Allow), _) => {
                                    (Some(PermissionValue::Deny), "redx.png")
                                }
                                (Some(PermissionValue::Deny), _) => {
                                    (Some(PermissionValue::Ask), "ask.png")
                                }
                                (Some(PermissionValue::Ask), _) => (None, "next.png"),
                            };

                            if let Some(next) = next {
                                dict.insert(ty, next);
                            } else {
                                dict.remove(&ty);
                            }

                            let new_image = asset_server.load::<Image>(format!("images/{image}"));
                            commands.entity(caller.0).modify_component(
                                move |node: &mut BoundedNode| {
                                    node.image = Some(new_image);
                                },
                            );

                            dialog.modified = true;
                        },
                    ),
                )
        };

        let mut target_entity = None;

        let mut spawn_row = |ty: PermissionType, commands: &mut Commands| -> Entity {
            let hilight = target.ty == Some(ty);
            let mut props = DuiProps::default().with_prop("permission-name", ty.title().to_owned());
            if let Some(hash) = scene_hash.as_ref() {
                props = spawn_setting(
                    props,
                    ty,
                    PermissionLevel::Scene(scene_ent.unwrap(), hash.clone()),
                    true,
                );
            } else {
                props = spawn_setting(
                    props,
                    ty,
                    PermissionLevel::Scene(Entity::PLACEHOLDER, String::default()),
                    false,
                );
            }
            if is_portable {
                props = spawn_setting(
                    props,
                    ty,
                    PermissionLevel::Realm("<portable>".to_owned()),
                    false,
                );
            } else {
                props = spawn_setting(
                    props,
                    ty,
                    PermissionLevel::Realm(realm_address.clone()),
                    true,
                );
            }
            props = spawn_setting(props, ty, PermissionLevel::Global, true);
            let ent = commands
                .spawn_template(&dui, "permission", props)
                .unwrap()
                .root;

            commands.entity(ent).insert((
                Interaction::default(),
                On::<HoverEnter>::new(
                    move |mut q: Query<&mut Text, With<PermissionSettingDescription>>| {
                        q.get_single_mut().unwrap().sections[0].value = ty.description();
                    },
                ),
            ));

            if hilight {
                commands
                    .entity(ent)
                    .insert(BackgroundColor(Color::rgba(1.0, 1.0, 1.0, 0.1)));
                target_entity = Some(ent);
            }

            ent
        };

        let spawn_header = |text: &str, commands: &mut Commands| -> Entity {
            commands
                .spawn_template(
                    &dui,
                    "settings-header",
                    DuiProps::new().with_prop("label", text.to_owned()),
                )
                .unwrap()
                .root
        };

        let children = vec![
            spawn_header("Gameplay", &mut commands),
            spawn_row(PermissionType::MovePlayer, &mut commands),
            spawn_row(PermissionType::ForceCamera, &mut commands),
            spawn_row(PermissionType::PlayEmote, &mut commands),
            spawn_row(PermissionType::SetLocomotion, &mut commands),
            spawn_row(PermissionType::HideAvatars, &mut commands),
            spawn_row(PermissionType::DisableVoice, &mut commands),
            spawn_header("Navigation", &mut commands),
            spawn_row(PermissionType::Teleport, &mut commands),
            spawn_row(PermissionType::ChangeRealm, &mut commands),
            spawn_header("Portable Experiences", &mut commands),
            spawn_row(PermissionType::SpawnPortable, &mut commands),
            spawn_row(PermissionType::KillPortables, &mut commands),
            spawn_header("Communication", &mut commands),
            spawn_row(PermissionType::Web3, &mut commands),
            spawn_row(PermissionType::Fetch, &mut commands),
            spawn_row(PermissionType::Websocket, &mut commands),
            spawn_row(PermissionType::OpenUrl, &mut commands),
        ];

        commands
            .entity(components.named("permissions-box"))
            .push_children(&children);

        commands
            .entity(components.named("permission-description"))
            .insert(PermissionSettingDescription);

        if let Some(target) = target_entity {
            commands.fire_event(ScrollTargetEvent {
                scrollable: components.named("scrollable"),
                position: ScrollTarget::Entity(target),
            });
        }
    }
}

#[derive(Component)]
pub struct PermissionSettingDescription;
