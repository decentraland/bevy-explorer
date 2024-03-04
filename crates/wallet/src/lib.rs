use std::sync::Arc;

use async_trait::async_trait;
use bevy::prelude::*;
use common::structs::ChainLink;
use ethers_core::types::{transaction::eip2718::TypedTransaction, Address, Signature};
use ethers_signers::{LocalWallet, Signer, WalletError};
use isahc::http::Uri;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub mod browser_auth;
pub mod signed_login;

pub struct WalletPlugin;

impl Plugin for WalletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wallet>();
    }
}

#[derive(Resource, Clone, Default)]
pub struct Wallet(Arc<RwLock<WalletInner>>);

#[derive(Default)]
struct WalletInner {
    pub(crate) inner: Option<Box<dyn ObjSafeWalletSigner + 'static + Send + Sync>>,
    pub(crate) root_address: Option<Address>,
    pub(crate) delegates: Vec<ChainLink>,
}

impl Wallet {
    pub fn disconnect(&mut self) {
        let mut write = self.0.try_write().unwrap();
        write.inner = None;
        write.root_address = None;
        write.delegates.clear();
    }

    pub fn finalize_as_guest(&mut self) {
        let inner: Box<dyn ObjSafeWalletSigner + Send + Sync> =
            Box::new(LocalWallet::new(&mut rand::thread_rng()));
        let mut write = self.0.try_write().unwrap();
        write.root_address = Some(inner.address());
        write.delegates.clear();
        write.inner = Some(inner);
    }

    pub fn finalize(
        &mut self,
        root_address: Address,
        local_wallet: LocalWallet,
        auth: Vec<ChainLink>,
    ) {
        let mut write = self.0.try_write().unwrap();
        write.root_address = Some(root_address);
        write.delegates = auth;
        write.inner = Some(Box::new(local_wallet));
    }

    pub async fn sign_message(&self, message: String) -> Result<SimpleAuthChain, WalletError> {
        let read = self.0.read().await;
        read.inner
            .as_ref()
            .ok_or_else(|| {
                WalletError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "wallet not connected",
                ))
            })?
            .sign_message(message, read.root_address.unwrap(), &read.delegates)
            .await
    }

    pub fn address(&self) -> Option<Address> {
        self.0.try_read().unwrap().root_address
    }

    pub fn is_guest(&self) -> bool {
        self.0.try_read().unwrap().delegates.is_empty()
    }
}

#[async_trait]
pub(crate) trait ObjSafeWalletSigner {
    async fn sign_message(
        &self,
        message: String,
        root_address: Address,
        delegates: &[ChainLink],
    ) -> Result<SimpleAuthChain, WalletError>;

    /// Signs the transaction
    async fn sign_transaction(&self, message: &TypedTransaction) -> Result<Signature, WalletError>;

    /// Returns the signer's Ethereum Address
    fn address(&self) -> Address;

    /// Returns the signer's chain id
    fn chain_id(&self) -> u64;
}

#[async_trait]
impl ObjSafeWalletSigner for LocalWallet {
    async fn sign_message(
        &self,
        message: String,
        root_address: Address,
        delegates: &[ChainLink],
    ) -> Result<SimpleAuthChain, WalletError> {
        let signature = Signer::sign_message(self, &message).await?;
        Ok(SimpleAuthChain::new(
            root_address,
            delegates,
            message,
            signature,
        ))
    }

    async fn sign_transaction(&self, message: &TypedTransaction) -> Result<Signature, WalletError> {
        Signer::sign_transaction(self, message).await
    }

    fn address(&self) -> Address {
        Signer::address(self)
    }

    fn chain_id(&self) -> u64 {
        Signer::chain_id(self)
    }
}

#[derive(Serialize, Deserialize)]
pub struct SimpleAuthChain(Vec<ChainLink>);

impl SimpleAuthChain {
    pub fn new(
        signer_address: Address,
        delegates: &[ChainLink],
        payload: String,
        signature: Signature,
    ) -> Self {
        let mut links = Vec::with_capacity(delegates.len() + 2);
        links.push(ChainLink {
            ty: "SIGNER".to_owned(),
            payload: format!("{signer_address:#x}"),
            signature: String::default(),
        });
        links.extend(delegates.iter().cloned());
        links.push(ChainLink {
            ty: "ECDSA_SIGNED_ENTITY".to_owned(),
            payload,
            signature: format!("0x{signature}"),
        });
        Self(links)
    }

    pub fn headers(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.0.iter().enumerate().map(|(ix, link)| {
            (
                format!("x-identity-auth-chain-{}", ix),
                serde_json::to_string(&link).unwrap(),
            )
        })
    }

    pub fn formdata(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.0.iter().enumerate().flat_map(|(ix, link)| {
            [
                (format!("authChain[{ix}][type]"), link.ty.clone()),
                (format!("authChain[{ix}][payload]"), link.payload.clone()),
                (
                    format!("authChain[{ix}][signature]"),
                    link.signature.clone(),
                ),
            ]
        })
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedLoginMeta {
    pub intent: String,
    pub signer: String,
    is_guest: bool,
    origin: String,
}

impl SignedLoginMeta {
    pub fn new(is_guest: bool, origin: Uri) -> Self {
        let origin = origin.into_parts();

        Self {
            intent: "dcl:explorer:comms-handshake".to_owned(),
            signer: "dcl:explorer".to_owned(),
            is_guest,
            origin: format!("{}://{}", origin.scheme.unwrap(), origin.authority.unwrap()),
        }
    }
}

pub async fn sign_request<META: Serialize>(
    method: &str,
    uri: &Uri,
    wallet: &Wallet,
    meta: META,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    let unix_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let meta = serde_json::to_string(&meta).unwrap();
    let payload = format!("{}:{}:{}:{}", method, uri.path(), unix_time, meta).to_lowercase();
    let auth_chain = wallet.sign_message(payload).await?;

    let mut headers: Vec<_> = auth_chain.headers().collect();
    headers.push(("x-identity-timestamp".to_owned(), format!("{}", unix_time)));
    headers.push(("x-identity-metadata".to_owned(), meta));
    Ok(headers)
}
