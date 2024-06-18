// kuruk 0x481bed8645804714Efd1dE3f25467f78E7Ba07d6

use std::io::Write;

use avatar::{avatar_texture::BoothInstance, AvatarShape};
use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::{AvatarColor, AvatarEmote, SerializedProfile},
    rpc::RpcCall,
    structs::{ActiveDialog, AppConfig},
};
use comms::profile::CurrentUserProfile;
use ipfs::{ChangeRealmEvent, CurrentRealm};
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
};

pub struct ProfileEditPlugin;

impl Plugin for ProfileEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SerializeUi>()
            .add_event::<ShowSettingsEvent>();
        app.add_systems(Startup, setup);
        app.add_systems(Update, (show_settings, dump));
        app.add_plugins((
            DiscoverSettingsPlugin,
            WearableSettingsPlugin,
            EmoteSettingsPlugin,
            AppSettingsPlugin,
            PermissionSettingsPlugin,
        ));
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // profile button
    commands.spawn((
        ImageBundle {
            image: asset_server.load("images/profile_button.png").into(),
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::VMin(BUTTON_SCALE * 0.5),
                right: Val::VMin(BUTTON_SCALE * 0.5),
                width: Val::VMin(BUTTON_SCALE),
                height: Val::VMin(BUTTON_SCALE),
                ..Default::default()
            },
            focus_policy: bevy::ui::FocusPolicy::Block,
            ..Default::default()
        },
        Interaction::default(),
        ShowSettingsEvent(SettingsTab::Discover).send_value_on::<Click>(),
    ));
}

pub struct InfoDialog;

impl InfoDialog {
    pub fn click(title: String, body: String) -> On<Click> {
        On::<Click>::new(move |mut commands: Commands, dui: Res<DuiRegistry>| {
            commands
                .spawn_template(
                    &dui,
                    "text-dialog",
                    DuiProps::new()
                        .with_prop("title", title.clone())
                        .with_prop("body", body.clone())
                        .with_prop("buttons", vec![DuiButton::close("Ok")]),
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
        modified.get_single()
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
                .clone_from(&avatar.shape.body_shape);
            profile.content.avatar.hair = avatar.shape.hair_color.map(AvatarColor::new);
            profile.content.avatar.eyes = avatar.shape.eye_color.map(AvatarColor::new);
            profile.content.avatar.skin = avatar.shape.skin_color.map(AvatarColor::new);
            profile.content.avatar.wearables = avatar.shape.wearables.to_vec();
            profile.content.avatar.emotes = Some(
                avatar
                    .shape
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
            current_profile.snapshots.clone_from(&booth.snapshot_target);
        }

        current_profile.is_deployed = false;
    }

    commands.entity(dialog_ent).despawn_recursive();
}

fn really_close_settings(
    mut commands: Commands,
    modified: Query<Entity, With<SettingsDialog>>,
    mut config: ResMut<AppConfig>,
) {
    let Ok(dialog_ent) = modified.get_single() else {
        error!("no dialog");
        return;
    };

    commands.entity(dialog_ent).despawn_recursive();

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
    let Ok((settings_ent, mut settings)) = q.get_single_mut() else {
        warn!("no settings dialog");
        return;
    };

    let ev = settings.on_close.take();
    if settings.modified {
        let send_onclose =
            move |mut cr: EventWriter<ChangeRealmEvent>, mut rpc: EventWriter<RpcCall>| match &ev {
                Some(OnCloseEvent::ChangeRealm(cr_ev, rpc_ev)) => {
                    cr.send(cr_ev.clone());
                    rpc.send(rpc_ev.clone());
                }
                Some(OnCloseEvent::SomethingElse) => (),
                _ => (),
            };

        commands
            .spawn_template(
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
                            DuiButton::new_enabled_and_close(
                                "Save Changes",
                                save_settings.pipe(send_onclose.clone()),
                            ),
                            DuiButton::new_enabled_and_close(
                                "Discard",
                                really_close_settings.pipe(send_onclose),
                            ),
                            DuiButton::new_enabled_and_close(
                                "Cancel",
                                |mut q: Query<&mut SettingsDialog>| {
                                    if let Ok(mut settings) = q.get_single_mut() {
                                        settings.on_close = None;
                                    }
                                },
                            ),
                        ],
                    ),
            )
            .unwrap();
    } else {
        commands.entity(settings_ent).despawn_recursive();
        match &ev {
            Some(OnCloseEvent::ChangeRealm(cr_ev, rpc_ev)) => {
                cr.send(cr_ev.clone());
                rpc.send(rpc_ev.clone());
            }
            Some(OnCloseEvent::SomethingElse) => (),
            _ => (),
        }
    }
}

#[derive(Event, Clone)]
pub struct ShowSettingsEvent(pub SettingsTab);

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
                *content.single_mut() = match selected.get(caller.0).unwrap().selected.unwrap() {
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

    commands.entity(components.root).insert(ZIndex::Global(2));
    commands
        .entity(components.named("change-realm-button"))
        .insert(UpdateRealmText);
    commands
        .entity(components.named("settings-content"))
        .insert(tab);

    //start on the wearables tab
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    ProfileDetail,
    #[default]
    Wearables,
    Emotes,
    Map,
    Discover,
    Settings,
    Permissions,
}

#[derive(Event, Default)]
pub struct SerializeUi;

fn dump(world: &World, mut ev: EventReader<SerializeUi>) {
    if ev.read().last().is_none() {
        return;
    }

    let scene = DynamicScene::from_world(world);
    let mut file = std::fs::File::create("dump.ron").unwrap();
    let type_registry = world.resource::<AppTypeRegistry>();

    file.write_all(scene.serialize_ron(&type_registry.0).unwrap().as_bytes())
        .unwrap();
}
