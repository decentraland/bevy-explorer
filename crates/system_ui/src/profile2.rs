use bevy::prelude::*;
use bevy_dui::{
    DuiEntityCommandsExt, DuiProps, DuiRegistry,
};
use comms::profile::CurrentUserProfile;
use ipfs::CurrentRealm;
use ui_core::{
    button::DuiButton, ui_actions::{Click, EntityActionExt, EventDefaultExt, On}
};

use crate::change_realm::{ChangeRealmDialog, UpdateRealmText};

pub struct ProfileEditPlugin;

impl Plugin for ProfileEditPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        // app.add_systems(Update, update_booth);
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
        On::<Click>::new(show_settings),
    ));
}

#[derive(Component)]
struct ProfileWindow;

pub struct InfoDialog;

impl InfoDialog {
    pub fn click(title: String, body: String) -> On<Click> {
        On::<Click>::new(move |mut commands: Commands, dui: Res<DuiRegistry>| {
            let mut commands = commands.spawn_empty();
            let id = commands.id();
            commands
                .apply_template(
                    &dui,
                    "text-dialog",
                    DuiProps::new()
                        .with_prop("title", title.clone())
                        .with_prop("body", body.clone())
                        .with_prop("buttons", vec![DuiButton::cancel("Ok", id)]),
                )
                .unwrap();
        })
    }
}

pub fn show_settings(mut commands: Commands, dui: Res<DuiRegistry>, realm: Res<CurrentRealm>, profile: Res<CurrentUserProfile>) {
    let mut root = commands.spawn_empty();
    let root_id = root.id();

    let mut props = DuiProps::new();

    for prop in ["discover", "wearables", "emotes", "map", "settings", "connect-wallet", "profile-settings"] {
        props.insert_prop(
            prop,
            InfoDialog::click("Not implemented".to_owned(), "Not implemented".to_owned()),
        );
    }

    props.insert_prop("realm", format!("Realm: {}", realm.config.realm_name.clone().unwrap_or_else(|| String::from("<none>"))));
    props.insert_prop("change-realm", ChangeRealmDialog::default_on::<Click>());
    props.insert_prop("profile-name", profile.profile.as_ref().unwrap().content.name.clone());
    props.insert_prop("close-settings", root_id.despawn_recursive_on::<Click>());

    let components = root.apply_template(&dui, "settings", props).unwrap();

    commands.entity(components.named("change-realm-button")).insert(UpdateRealmText);
    commands.entity(components.named("settings-content")).insert(SettingsContent(SettingsTab::Wearables));

    //start on the wearables tab
    
}

#[derive(Default)]
pub enum SettingsTab {
    #[default]
    Wearables,
    Emotes,
    Map,
    Discover,
    Settings,
}

#[derive(Component, Default)]
pub struct SettingsContent(SettingsTab);

fn update_settings_content(
    mut commands: Commands,
    q: Query<(Entity, &SettingsContent), Changed<SettingsContent>>,
    dui: Res<DuiRegistry>,
) {
    for (ent, settings) in q.iter() {
        commands.entity(ent).despawn_descendants();

        match settings.0 {
            SettingsTab::Wearables => {
                // commands.entity(ent).apply_template(&dui, "wearables", props)
            },
            SettingsTab::Emotes => todo!(),
            SettingsTab::Map => todo!(),
            SettingsTab::Discover => todo!(),
            SettingsTab::Settings => todo!(),
        };
    }
}
