use bevy::{
    ecs::event::ManualEventReader,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::util::TaskExt;
use ipfs::CurrentRealm;
use isahc::http::Uri;
use wallet::{
    signed_login::{signed_login, SignedLoginResponse},
    SignedLoginMeta, Wallet,
};

use crate::AdapterManager;

pub struct SignedLoginPlugin;

impl Plugin for SignedLoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, start_signed_login);
        app.add_event::<StartSignedLogin>();
    }
}

#[derive(Event)]
pub struct StartSignedLogin {
    pub address: String,
}

pub fn start_signed_login(
    mut signed_login_events: Local<ManualEventReader<StartSignedLogin>>,
    current_realm: Res<CurrentRealm>,
    wallet: Res<Wallet>,
    mut task: Local<Option<Task<Result<SignedLoginResponse, anyhow::Error>>>>,
    mut manager: AdapterManager,
) {
    if let Some(ev) = signed_login_events
        .read(&manager.signed_login_events)
        .last()
    {
        info!("starting signed login");
        let address = ev.address.clone();
        let Ok(uri) = Uri::try_from(&address) else {
            warn!("failed to parse signed login address as a uri: {address}");
            return;
        };
        let wallet = wallet.clone();
        let Ok(origin) = Uri::try_from(&current_realm.address) else {
            warn!("failed to parse signed login address as a uri: {address}");
            return;
        };

        let meta = SignedLoginMeta::new(wallet.is_guest(), origin);
        *task = Some(IoTaskPool::get().spawn(signed_login(uri, wallet, meta)));
    }

    if let Some(mut current_task) = task.take() {
        if let Some(result) = current_task.complete() {
            match result {
                Ok(SignedLoginResponse {
                    fixed_adapter: Some(adapter),
                    ..
                }) => {
                    info!("signed login ok, connecting to inner {adapter}");
                    manager.connect(adapter.as_str());
                }
                otherwise => warn!("signed login failed: {otherwise:?}"),
            }
        } else {
            *task = Some(current_task);
        }
    }
}
