// kuruk 0x481bed8645804714Efd1dE3f25467f78E7Ba07d6

use std::io::Write;

use avatar::{avatar_texture::BoothInstance, AvatarShape};
use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::{AvatarColor, SerializedProfile},
    rpc::RpcCall,
};
use comms::profile::CurrentUserProfile;
use ipfs::{ChangeRealmEvent, CurrentRealm};
use ui_core::{
    button::{DuiButton, TabSelection},
    ui_actions::{Click, DataChanged, EventCloneExt, EventDefaultExt, On, UiCaller},
};

use crate::{
    change_realm::{ChangeRealmDialog, UpdateRealmText},
    discover::DiscoverSettingsPlugin,
    emotes::EmotesSettingsPlugin,
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
            EmotesSettingsPlugin,
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
                top: Val::Px(10.0),
                right: Val::Px(10.0),
                ..Default::default()
            },
            focus_policy: bevy::ui::FocusPolicy::Block,
            ..Default::default()
        },
        Interaction::default(),
        ShowSettingsEvent(SettingsTab::Discover).send_value_on::<Click>(),
    ));
}

#[derive(Component)]
struct ProfileWindow;

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
    modified: Query<(Entity, Option<&AvatarShape>, Option<&BoothInstance>), With<SettingsDialog>>,
) {
    let Some(profile) = current_profile.profile.as_mut() else {
        error!("can't amend missing profile");
        return;
    };

    let Ok((dialog_ent, maybe_avatar, maybe_booth)) = modified.get_single() else {
        error!("no dialog");
        return;
    };

    if let Some(avatar) = maybe_avatar {
        profile.content.avatar.body_shape = avatar.0.body_shape.to_owned();
        profile.content.avatar.hair = avatar.0.hair_color.map(AvatarColor::new);
        profile.content.avatar.eyes = avatar.0.eye_color.map(AvatarColor::new);
        profile.content.avatar.skin = avatar.0.skin_color.map(AvatarColor::new);
        profile.content.avatar.wearables = avatar.0.wearables.to_vec();
    }

    profile.version += 1;
    profile.content.version = profile.version as i64;

    if let Some(booth) = maybe_booth {
        current_profile.snapshots = booth.snapshot_target.clone();
    }

    current_profile.is_deployed = false;

    commands.entity(dialog_ent).despawn_recursive();
}

fn really_close_settings(mut commands: Commands, modified: Query<Entity, With<SettingsDialog>>) {
    let Ok(dialog_ent) = modified.get_single() else {
        error!("no dialog");
        return;
    };

    commands.entity(dialog_ent).despawn_recursive();
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

pub fn show_settings(
    mut commands: Commands,
    dui: Res<DuiRegistry>,
    realm: Res<CurrentRealm>,
    current_profile: Res<CurrentUserProfile>,
    mut ev: EventReader<ShowSettingsEvent>,
    existing: Query<(), With<SettingsDialog>>,
) {
    let Some(ev) = ev.read().last() else {
        return;
    };

    if existing.iter().next().is_some() {
        return;
    }

    let title_initial = match ev.0 {
        SettingsTab::Discover => 0usize,
        SettingsTab::Wearables => 1,
        SettingsTab::Emotes => 2,
        SettingsTab::Map => 3,
        SettingsTab::Settings => 4,
    };

    let Some(profile) = &current_profile.profile.as_ref() else {
        error!("can't edit missing profile");
        return;
    };

    let mut root = commands.spawn(SettingsDialog {
        modified: false,
        profile: profile.content.clone(),
        on_close: None,
    });
    // let root_id = root.id();

    let mut props = DuiProps::new();

    for prop in [
        "discover",
        "emotes",
        "map",
        "settings",
        "connect-wallet",
        "profile-settings",
    ] {
        props.insert_prop(
            prop,
            InfoDialog::click("Not implemented".to_owned(), "Not implemented".to_owned()),
        );
    }

    let tabs = vec![
        DuiButton {
            label: Some("Discover".to_owned()),
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
            enabled: false,
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
                    1 => SettingsTab::Wearables,
                    2 => SettingsTab::Emotes,
                    3 => SettingsTab::Map,
                    4 => SettingsTab::Settings,
                    _ => panic!(),
                }
            },
        ),
    );

    // props.insert_prop("wearables", On::<Click>::new(|mut commands: Commands, q: Query<Entity, With<SettingsTab>>| {commands.entity(q.single()).insert(SettingsTab::Wearables);}));
    // props.insert_prop("emotes", On::<Click>::new(|mut commands: Commands, q: Query<Entity, With<SettingsTab>>| {commands.entity(q.single()).insert(SettingsTab::Emotes);}));
    // props.insert_prop("settings", SerializeUi::default_on::<Click>());

    let components = root.apply_template(&dui, "settings", props).unwrap();

    commands
        .entity(components.named("change-realm-button"))
        .insert(UpdateRealmText);
    commands
        .entity(components.named("settings-content"))
        .insert(ev.0);

    //start on the wearables tab
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Wearables,
    Emotes,
    Map,
    Discover,
    Settings,
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
