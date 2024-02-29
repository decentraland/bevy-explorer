use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    window::PrimaryWindow,
};
use bevy_dui::{DuiCommandsExt, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::SerializedProfile,
    structs::{AppConfig, ChainLink, PreviousLogin},
    util::TaskExt,
};
use comms::profile::{get_remote_profile, CurrentUserProfile, UserProfile};
use ethers_core::types::Address;
use ethers_signers::LocalWallet;
use ipfs::{CurrentRealm, IpfsAssetServer};
use scene_runner::Toaster;
use ui_core::{
    button::DuiButton,
    dialog::{IntoDialogBody, SpawnButton},
    ui_actions::{Click, EventCloneExt, On},
    BODY_TEXT_STYLE,
};
use wallet::{browser_auth::try_create_remote_ephemeral, Wallet};

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoginType>().add_systems(
            Update,
            (login, update_profile_for_realm).run_if(in_state(ui_core::State::Ready)),
        );
    }
}

#[derive(Event, Clone)]
enum LoginType {
    ExistingRemote,
    NewRemote,
    Guest,
    Cancel,
}

struct CancelLoginDialog {
    sender: tokio::sync::mpsc::Sender<LoginType>,
}

impl IntoDialogBody for CancelLoginDialog {
    fn body(self, commands: &mut ChildBuilder) {
        let sender = self.sender.clone();

        commands.spawn(
            TextBundle::from_section(
                "Please follow the directions in your browser to connect your account",
                BODY_TEXT_STYLE.get().unwrap().clone(),
            )
            .with_text_alignment(TextAlignment::Center),
        );

        commands.spawn_empty().spawn_button("Cancel", move || {
            let _ = sender.blocking_send(LoginType::Cancel);
        });
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn login(
    mut commands: Commands,
    ipfas: IpfsAssetServer,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut task: Local<
        Option<
            Task<
                Result<(Address, LocalWallet, Vec<ChainLink>, Option<UserProfile>), anyhow::Error>,
            >,
        >,
    >,
    mut logins: EventReader<LoginType>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
    dui: Res<DuiRegistry>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    // cleanup if we're done
    if wallet.address().is_some() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }
        *dialog = None;
        *task = None;
        return;
    }

    // create dialog
    if dialog.is_none() && task.is_none() {
        let previous_login = std::fs::read("config.json")
            .ok()
            .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
            .unwrap_or_default()
            .previous_login;

        let mut dlg = commands.spawn_empty();
        *dialog = Some(dlg.id());
        dlg.apply_template(
            &dui,
            "login",
            DuiProps::new()
                .with_prop("allow-reuse", previous_login.is_some())
                .with_prop("reuse", LoginType::ExistingRemote.send_value_on::<Click>())
                .with_prop("connect", LoginType::NewRemote.send_value_on::<Click>())
                .with_prop("guest", LoginType::Guest.send_value_on::<Click>())
                .with_prop("quit", On::<Click>::new(move || std::process::exit(0))),
        )
        .unwrap();

        return;
    }

    // handle task results
    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok((root_address, local_wallet, auth, profile))) => {
                if let Ok(mut window) = window.get_single_mut() {
                    window.focused = true;
                }

                let ephemeral_key = local_wallet.signer().to_bytes().to_vec();

                // store to app config
                let mut config: AppConfig = std::fs::read("config.json")
                    .ok()
                    .and_then(|f| serde_json::from_slice(&f).ok())
                    .unwrap_or_default();
                config.previous_login = Some(PreviousLogin {
                    root_address,
                    ephemeral_key,
                    auth: auth.clone(),
                });
                if let Err(e) =
                    std::fs::write("config.json", serde_json::to_string(&config).unwrap())
                {
                    warn!("failed to write to config: {e}");
                }

                wallet.finalize(root_address, local_wallet, auth);
                if let Some(profile) = profile {
                    toaster.add_toast("login profile", "Profile loaded");
                    current_profile.profile = Some(profile);
                    current_profile.is_deployed = true;
                } else {
                    toaster.add_toast("login profile", "Failed to load profile, using default");
                    current_profile.profile = Some(UserProfile {
                        version: 0,
                        content: SerializedProfile {
                            has_connected_web3: Some(true),
                            eth_address: format!("{:#x}", wallet.address().unwrap()),
                            user_id: Some(format!("{:#x}", wallet.address().unwrap())),
                            ..Default::default()
                        },
                        base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                    });
                    current_profile.is_deployed = false;
                }
            }
            Some(Err(e)) => {
                error!("{e}");
                toaster.add_toast("login profile", format!("Login failed: {}", e));
                if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
                    commands.despawn_recursive();
                }
                *dialog = None;
            }
            None => {
                *task = Some(t);
            }
        }
    }

    // handle click
    if let Some(login) = logins.read().last() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }

        match login {
            LoginType::ExistingRemote => {
                info!("existing remote");
                let ipfs = ipfas.ipfs().clone();
                let previous_login = std::fs::read("config.json")
                    .ok()
                    .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
                    .unwrap()
                    .previous_login
                    .unwrap();

                *task = Some(IoTaskPool::get().spawn(async move {
                    let PreviousLogin {
                        root_address,
                        ephemeral_key,
                        auth,
                    } = previous_login;

                    let profile = get_remote_profile(root_address, ipfs).await.ok();

                    let local_wallet = LocalWallet::from_bytes(&ephemeral_key).unwrap();

                    Ok((previous_login.root_address, local_wallet, auth, profile))
                }));
            }
            LoginType::NewRemote => {
                info!("new remote");

                let ipfs = ipfas.ipfs().clone();
                *task = Some(IoTaskPool::get().spawn(async move {
                    let (root_address, local_wallet, auth, _) =
                        try_create_remote_ephemeral().await?;

                    let profile = get_remote_profile(root_address, ipfs).await.ok();

                    Ok((root_address, local_wallet, auth, profile))
                }));

                let components = commands
                    .spawn_template(
                        &dui,
                        "cancel-login",
                        DuiProps::new().with_prop(
                            "buttons",
                            vec![DuiButton::new_enabled(
                                "Cancel",
                                |mut e: EventWriter<LoginType>| e.send(LoginType::Cancel),
                            )],
                        ),
                    )
                    .unwrap();

                *dialog = Some(components.root);
            }
            LoginType::Guest => {
                info!("guest");
                toaster.add_toast(
                    "login profile",
                    "Warning: Guest profile will not persist beyond the current session",
                );
                wallet.finalize_as_guest();
                current_profile.profile = Some(UserProfile {
                    version: 0,
                    content: SerializedProfile {
                        eth_address: format!("{:#x}", wallet.address().unwrap()),
                        user_id: Some(format!("{:#x}", wallet.address().unwrap())),
                        ..Default::default()
                    },
                    base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                });
                current_profile.is_deployed = true;
            }
            LoginType::Cancel => {
                *task = None;
                *dialog = None;
            }
        }
    }
}

fn update_profile_for_realm(
    realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut task: Local<Option<Task<Result<UserProfile, anyhow::Error>>>>,
    ipfas: IpfsAssetServer,
) {
    if realm.is_changed() && !wallet.is_guest() {
        if let Some(address) = wallet.address() {
            *task =
                Some(IoTaskPool::get().spawn(get_remote_profile(address, ipfas.ipfs().clone())));
        }
    }

    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok(profile)) => {
                current_profile.profile = Some(profile);
                current_profile.is_deployed = true;
            }
            Some(Err(_)) => {
                current_profile.profile = Some(UserProfile {
                    version: 0,
                    content: SerializedProfile {
                        has_connected_web3: Some(true),
                        ..Default::default()
                    },
                    base_url: ipfas.ipfs().contents_endpoint().unwrap_or_default(),
                });
            }
            None => *task = Some(t),
        }
    }
}
