use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
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
use ui_core::dialog::{ButtonDisabledText, ButtonText, IntoDialogBody, SpawnButton, SpawnDialog};
use wallet::{browser_auth::try_create_remote_ephemeral, Wallet};

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_wallet, update_profile_for_realm));
    }
}

enum LoginType {
    ExistingRemote,
    NewRemote,
    Guest,
}

struct LoginDialog {
    sender: tokio::sync::mpsc::Sender<LoginType>,
    previous_login: Option<PreviousLogin>,
}

impl IntoDialogBody for LoginDialog {
    fn body(self, commands: &mut ChildBuilder) {
        let sender = self.sender.clone();
        if self.previous_login.is_some() {
            commands
                .spawn_empty()
                .spawn_button(ButtonText("Reuse Last Login"), move || {
                    let _ = sender.blocking_send(LoginType::ExistingRemote);
                });
        } else {
            commands
                .spawn_empty()
                .spawn_button(ButtonDisabledText("Reuse Last Login"), move || {});
        }
        let sender = self.sender.clone();
        commands
            .spawn_empty()
            .spawn_button(ButtonText("Connect External Wallet"), move || {
                let _ = sender.blocking_send(LoginType::NewRemote);
            });
        let sender = self.sender.clone();
        commands
            .spawn_empty()
            .spawn_button(ButtonText("Play as Guest"), move || {
                let _ = sender.blocking_send(LoginType::Guest);
            });
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn connect_wallet(
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
    mut receiver: Local<Option<tokio::sync::mpsc::Receiver<LoginType>>>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
) {
    // cleanup if we're done
    if wallet.address().is_some() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }
        *dialog = None;
        *receiver = None;
        *task = None;
        return;
    }

    // create dialog
    if dialog.is_none() && task.is_none() {
        let (sx, rx) = tokio::sync::mpsc::channel(1);
        *receiver = Some(rx);

        let previous_login = std::fs::read("config.json")
            .ok()
            .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
            .unwrap_or_default()
            .previous_login;

        *dialog = Some(commands.spawn_dialog(
            "Login".to_string(),
            LoginDialog {
                sender: sx,
                previous_login,
            },
            "Quit",
            || {
                std::process::exit(0);
            },
        ));
        return;
    }

    // handle task results
    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok((root_address, local_wallet, auth, profile))) => {
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
            }
            None => {
                *task = Some(t);
            }
        }
    }

    // handle click
    if let Ok(login) = receiver.as_mut().unwrap().try_recv() {
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
