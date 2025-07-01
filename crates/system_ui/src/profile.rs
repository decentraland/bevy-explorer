// kuruk 0x481bed8645804714Efd1dE3f25467f78E7Ba07d6

use avatar::{
    avatar_texture::{BoothInstance, PhotoBooth},
    AvatarShape,
};
use bevy::prelude::*;
use bevy_dui::{DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::{AvatarColor, AvatarEmote, SerializedProfile},
    rpc::{RpcCall, RpcResultSender},
    sets::SetupSets,
    structs::{
        ActiveDialog, AppConfig, PermissionTarget, SettingsTab, ShowSettingsEvent, SystemAudio,
        ZOrder, PROFILE_UI_RENDERLAYER,
    },
    util::TryPushChildrenEx,
};
use comms::profile::{CurrentUserProfile, ProfileDeployedEvent};
use ipfs::{ChangeRealmEvent, CurrentRealm};
use system_bridge::SystemApi;
use ui_core::{
    button::{DuiButton, TabSelection},
    ui_actions::{Click, DataChanged, EventCloneExt, EventDefaultExt, On, UiCaller},
};

use crate::{
    app_settings::{AppSettingsDetail, AppSettingsPlugin},
    change_realm::{ChangeRealmDialog, UpdateRealmText},
    chat::BUTTON_SCALE,
    discover::DiscoverSettingsPlugin,
    emotes::EmoteSettingsPlugin,
    permissions::{PermissionSettingsDetail, PermissionSettingsPlugin},
    profile_detail::ProfileDetail,
    wearables::WearableSettingsPlugin,
    SystemUiRoot,
};

pub struct ProfileEditPlugin;

impl Plugin for ProfileEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ShowSettingsEvent>();
        app.add_systems(Startup, setup.in_set(SetupSets::Main));
        app.add_systems(Update, (show_settings, process_profile));
        app.add_plugins((
            DiscoverSettingsPlugin,
            WearableSettingsPlugin,
            EmoteSettingsPlugin,
            AppSettingsPlugin,
            PermissionSettingsPlugin,
        ));
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, ui_root: Res<SystemUiRoot>) {
    // profile button
    let button = commands
        .spawn((
            ImageNode::new(asset_server.load("images/profile_button.png")),
            Node {
                position_type: PositionType::Absolute,
                top: Val::VMin(BUTTON_SCALE * 0.5),
                right: Val::VMin(BUTTON_SCALE * 0.5),
                width: Val::VMin(BUTTON_SCALE),
                height: Val::VMin(BUTTON_SCALE),
                ..Default::default()
            },
            bevy::ui::FocusPolicy::Block,
            Interaction::default(),
            On::<Click>::new(
                (move |mut target: ResMut<PermissionTarget>| {
                    target.scene = None;
                    target.ty = None;
                })
                .pipe(ShowSettingsEvent(SettingsTab::Discover).send_value())
                .pipe(SystemAudio("sounds/ui/mainmenu_widget_open.wav".to_owned()).send_value()),
            ),
        ))
        .id();

    commands.entity(ui_root.0).try_push_children(&[button]);
}

pub struct InfoDialog;

impl InfoDialog {
    pub fn click(title: String, body: String) -> On<Click> {
        On::<Click>::new(move |mut commands: Commands, dui: Res<DuiRegistry>| {
            commands
                .spawn(ZOrder::BackpackPopup.default())
                .apply_template(
                    &dui,
                    "text-dialog",
                    DuiProps::new()
                        .with_prop("title", title.clone())
                        .with_prop("body", body.clone())
                        .with_prop("buttons", vec![DuiButton::close_happy("Ok")]),
                )
                .unwrap();
        })
    }
}

#[derive(Component)]
pub struct SettingsDialog {
    pub modified: bool,
    pub profile: SerializedProfile,
    pub on_close: Option<OnCloseEvent>,
}

#[derive(Clone)]
pub enum OnCloseEvent {
    ChangeRealm(ChangeRealmEvent, RpcCall),
    SomethingElse,
}

#[allow(clippy::type_complexity)]
fn save_settings(
    mut commands: Commands,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut config: ResMut<AppConfig>,
    modified: Query<
        (
            Entity,
            Option<&AvatarShape>,
            Option<&ProfileDetail>,
            Option<&BoothInstance>,
            Option<&AppSettingsDetail>,
            Option<&PermissionSettingsDetail>,
        ),
        With<SettingsDialog>,
    >,
) {
    let Some(profile) = current_profile.profile.as_mut() else {
        error!("can't amend missing profile");
        return;
    };

    let Ok((dialog_ent, maybe_avatar, maybe_detail, maybe_booth, maybe_settings, maybe_perms)) =
        modified.single()
    else {
        error!("no dialog");
        return;
    };

    if let Some(settings) = maybe_settings {
        *config = settings.0.clone();
    }

    if let Some(perms) = maybe_perms {
        config
            .scene_permissions
            .clone_from(&perms.0.scene_permissions);
        config
            .realm_permissions
            .clone_from(&perms.0.realm_permissions);
        config
            .default_permissions
            .clone_from(&perms.0.default_permissions);
    }

    if maybe_detail.is_some() || maybe_avatar.is_some() {
        if let Some(detail) = maybe_detail {
            profile.content = detail.0.clone();
        }

        if let Some(avatar) = maybe_avatar {
            profile
                .content
                .avatar
                .body_shape
                .clone_from(&avatar.0.body_shape);
            profile.content.avatar.hair = avatar.0.hair_color.map(AvatarColor::new);
            profile.content.avatar.eyes = avatar.0.eye_color.map(AvatarColor::new);
            profile.content.avatar.skin = avatar.0.skin_color.map(AvatarColor::new);
            profile.content.avatar.wearables = avatar.0.wearables.to_vec();
            profile.content.avatar.emotes = Some(
                avatar
                    .0
                    .emotes
                    .iter()
                    .enumerate()
                    .flat_map(|(ix, e)| {
                        (!e.is_empty()).then_some(AvatarEmote {
                            slot: ix as u32,
                            urn: if e.starts_with("urn:decentraland:off-chain:base-emotes") {
                                e.rsplit_once(':').unwrap().1.to_string()
                            } else {
                                e.clone()
                            },
                        })
                    })
                    .collect(),
            );
        }

        profile.version += 1;
        profile.content.version = profile.version as i64;

        if let Some(booth) = maybe_booth {
            if let (Some(face), Some(body)) = (
                booth.snapshot_target.0.clone(),
                booth.snapshot_target.1.clone(),
            ) {
                current_profile.snapshots = Some((face, body));
            }
        }

        current_profile.is_deployed = false;
    }

    commands.entity(dialog_ent).despawn();
}

fn really_close_settings(
    mut commands: Commands,
    modified: Query<Entity, With<SettingsDialog>>,
    mut config: ResMut<AppConfig>,
) {
    let Ok(dialog_ent) = modified.single() else {
        error!("no dialog");
        return;
    };

    commands.entity(dialog_ent).despawn();

    // touch the app config so all settings get reverted
    config.set_changed();
}

pub fn close_settings(
    mut commands: Commands,
    mut q: Query<(Entity, &mut SettingsDialog)>,
    dui: Res<DuiRegistry>,
    mut cr: EventWriter<ChangeRealmEvent>,
    mut rpc: EventWriter<RpcCall>,
) {
    let Ok((settings_ent, mut settings)) = q.single_mut() else {
        warn!("no settings dialog");
        return;
    };

    let ev = settings.on_close.take();
    if settings.modified {
        let send_onclose =
            move |mut cr: EventWriter<ChangeRealmEvent>, mut rpc: EventWriter<RpcCall>| match &ev {
                Some(OnCloseEvent::ChangeRealm(cr_ev, rpc_ev)) => {
                    cr.write(cr_ev.clone());
                    rpc.write(rpc_ev.clone());
                }
                Some(OnCloseEvent::SomethingElse) => (),
                _ => (),
            };

        commands
            .spawn(ZOrder::BackpackPopup.default())
            .apply_template(
                &dui,
                "text-dialog",
                DuiProps::new()
                    .with_prop("title", "Unsaved Changes".to_owned())
                    .with_prop(
                        "body",
                        "You have unsaved changes, do you want to save them?".to_owned(),
                    )
                    .with_prop(
                        "buttons",
                        vec![
                            DuiButton::new_enabled_and_close_happy(
                                "Save Changes",
                                save_settings.pipe(send_onclose.clone()),
                            ),
                            DuiButton::new_enabled_and_close_sad(
                                "Discard",
                                really_close_settings.pipe(send_onclose),
                            ),
                            DuiButton::new_enabled_and_close_sad(
                                "Cancel",
                                |mut q: Query<&mut SettingsDialog>| {
                                    if let Ok(mut settings) = q.single_mut() {
                                        settings.on_close = None;
                                    }
                                },
                            ),
                        ],
                    ),
            )
            .unwrap();
    } else {
        commands.entity(settings_ent).despawn();
        match &ev {
            Some(OnCloseEvent::ChangeRealm(cr_ev, rpc_ev)) => {
                cr.write(cr_ev.clone());
                rpc.write(rpc_ev.clone());
                commands.send_event(SystemAudio("sounds/ui/toggle_enable.wav".to_owned()));
            }
            _ => {
                commands.send_event(SystemAudio("sounds/ui/toggle_disable.wav".to_owned()));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn show_settings(
    mut commands: Commands,
    dui: Res<DuiRegistry>,
    realm: Res<CurrentRealm>,
    current_profile: Res<CurrentUserProfile>,
    mut ev: EventReader<ShowSettingsEvent>,
    existing: Query<(), With<SettingsDialog>>,
    active_dialog: Res<ActiveDialog>,
    mut pending: Local<Option<SettingsTab>>,
) {
    let Some(tab) = ev.read().last().map(|ev| ev.0).or(pending.take()) else {
        return;
    };

    if existing.iter().next().is_some() {
        return;
    }

    let Some(permit) = active_dialog.try_acquire() else {
        // resend
        *pending = Some(tab);
        return;
    };

    let title_initial = match tab {
        SettingsTab::Discover => 0usize,
        SettingsTab::ProfileDetail => 1,
        SettingsTab::Wearables => 2,
        SettingsTab::Emotes => 3,
        SettingsTab::Map => 4,
        SettingsTab::Settings => 5,
        SettingsTab::Permissions => 6,
    };

    let Some(profile) = &current_profile.profile.as_ref() else {
        error!("can't edit missing profile");
        return;
    };

    let mut root = commands.spawn((
        SettingsDialog {
            modified: false,
            profile: profile.content.clone(),
            on_close: None,
        },
        permit,
        ZOrder::Backpack.default(),
    ));
    // let root_id = root.id();

    let mut props = DuiProps::new();

    props.insert_prop(
        "connect-wallet",
        InfoDialog::click("Not implemented".to_owned(), "Not implemented".to_owned()),
    );

    let tabs = vec![
        DuiButton {
            label: Some("Discover".to_owned()),
            enabled: true,
            ..Default::default()
        },
        DuiButton {
            label: Some("Profile".to_owned()),
            enabled: true,
            ..Default::default()
        },
        DuiButton {
            label: Some("Wearables".to_owned()),
            ..Default::default()
        },
        DuiButton {
            label: Some("Emotes".to_owned()),
            ..Default::default()
        },
        DuiButton {
            label: Some("Map".to_owned()),
            enabled: true,
            ..Default::default()
        },
        DuiButton {
            label: Some("Settings".to_owned()),
            enabled: true,
            ..Default::default()
        },
        DuiButton {
            label: Some("Permissions".to_owned()),
            enabled: true,
            ..Default::default()
        },
    ];

    props.insert_prop(
        "realm",
        format!(
            "Realm: {}",
            realm
                .config
                .realm_name
                .clone()
                .unwrap_or_else(|| String::from("<none>"))
        ),
    );
    props.insert_prop(
        "change-realm",
        ChangeRealmDialog::send_default_on::<Click>(),
    );
    props.insert_prop("profile-name", profile.content.name.clone());
    props.insert_prop("close-settings", On::<Click>::new(close_settings));
    props.insert_prop("title-tabs", tabs);
    props.insert_prop("title-initial", Some(title_initial));
    props.insert_prop(
        "title-onchanged",
        On::<DataChanged>::new(
            |caller: Res<UiCaller>,
             selected: Query<&TabSelection>,
             mut content: Query<&mut SettingsTab>| {
                *content.single_mut().unwrap() =
                    match selected.get(caller.0).unwrap().selected.unwrap() {
                        0 => SettingsTab::Discover,
                        1 => SettingsTab::ProfileDetail,
                        2 => SettingsTab::Wearables,
                        3 => SettingsTab::Emotes,
                        4 => SettingsTab::Map,
                        5 => SettingsTab::Settings,
                        6 => SettingsTab::Permissions,
                        _ => panic!(),
                    }
            },
        ),
    );

    let components = root.apply_template(&dui, "settings", props).unwrap();

    commands
        .entity(components.named("change-realm-button"))
        .insert(UpdateRealmText);
    commands
        .entity(components.named("settings-content"))
        .insert(tab);

    //start on the wearables tab
}

enum ProcessProfileState {
    Snapping(Entity, u32, RpcResultSender<Result<u32, String>>),
    Deploying(u32, RpcResultSender<Result<u32, String>>),
}

fn process_profile(
    mut commands: Commands,
    mut e: EventReader<SystemApi>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut processing: Local<Option<ProcessProfileState>>,
    mut photo_booth: PhotoBooth,
    booths: Query<&BoothInstance>,
    mut deployed: EventReader<ProfileDeployedEvent>,
) {
    if let Some(SystemApi::SetAvatar(set_avatar, sender)) = e
        .read()
        .filter(|ev| matches!(ev, SystemApi::SetAvatar(..)))
        .last()
    {
        let Some(profile) = current_profile.profile.as_mut() else {
            error!("can't amend missing profile");
            return;
        };

        if let Some(base) = &set_avatar.base {
            profile.content.avatar.body_shape = Some(base.body_shape_urn.clone());

            profile.content.avatar.hair = base.hair_color.map(AvatarColor::new);
            profile.content.avatar.eyes = base.eyes_color.map(AvatarColor::new);
            profile.content.avatar.skin = base.skin_color.map(AvatarColor::new);

            profile.content.name = base.name.clone();
            profile.content.avatar.name = Some(base.name.clone());
        }

        if let Some(has_claimed_name) = set_avatar.has_claimed_name {
            profile.content.has_claimed_name = has_claimed_name;
        }

        if let Some(equip) = &set_avatar.equip {
            profile.content.avatar.wearables = equip.wearable_urns.to_vec();
            profile.content.avatar.emotes = Some(
                equip
                    .emote_urns
                    .iter()
                    .enumerate()
                    .flat_map(|(ix, e)| {
                        (!e.is_empty()).then_some(AvatarEmote {
                            slot: ix as u32,
                            urn: if e.starts_with("urn:decentraland:off-chain:base-emotes") {
                                e.rsplit_once(':').unwrap().1.to_string()
                            } else {
                                e.clone()
                            },
                        })
                    })
                    .collect(),
            );
            profile.content.avatar.force_render = Some(equip.force_render.to_vec());
        }

        if let Some(extras) = &set_avatar.profile_extras {
            profile.content.extra_fields = extras.clone();
        }

        profile.version += 1;
        profile.content.version = profile.version as i64;

        debug!("{:#?}", profile.content);

        if let Some(existing) = processing.take() {
            match existing {
                ProcessProfileState::Snapping(entity, _, sender) => {
                    commands.entity(entity).despawn();
                    sender.send(Err("cancelled".to_owned()));
                }
                ProcessProfileState::Deploying(_, sender) => {
                    sender.send(Err("cancelled".to_owned()));
                }
            }
        }

        if profile.content.has_connected_web3.unwrap_or_default() {
            *processing = Some(ProcessProfileState::Snapping(
                commands
                    .spawn(photo_booth.spawn_booth(
                        PROFILE_UI_RENDERLAYER,
                        (&*profile).into(),
                        Default::default(),
                        true,
                    ))
                    .id(),
                profile.version,
                sender.clone(),
            ));
        } else {
            sender.send(Ok(u32::MAX))
        }

        return;
    }

    if let Some(ProcessProfileState::Snapping(booth_ent, version, sender)) = &*processing {
        let Ok(booth) = booths.get(*booth_ent) else {
            error!("no booth?");
            sender.send(Err("no snapshot booth?!".to_owned()));
            *processing = None;
            return;
        };

        debug!("processing ...");
        let (Some(face), Some(body)) = booth.snapshot_target.clone() else {
            return;
        };
        debug!("updating ...");

        commands.entity(*booth_ent).despawn();
        current_profile.snapshots = Some((face, body));
        current_profile.is_deployed = false;

        *processing = Some(ProcessProfileState::Deploying(*version, sender.clone()));
    }

    if let Some(ProcessProfileState::Deploying(version, sender)) = processing.take() {
        debug!("checking for response: {}", version);
        if let Some(ev) = deployed.read().next() {
            debug!("got response: {} vs {}", version, ev.version);
            let res = if version == ev.version {
                if ev.success {
                    Ok(version)
                } else {
                    Err("failed to deploy to server.".to_owned())
                }
            } else {
                Err("cancelled".to_owned())
            };
            sender.send(res);
            return;
        }
        *processing = Some(ProcessProfileState::Deploying(version, sender));
    } else {
        deployed.clear();
    }
}
