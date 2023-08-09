use std::sync::Arc;

use async_trait::async_trait;
use bevy::prelude::*;
use ethers_core::types::{transaction::eip2718::TypedTransaction, Address, Signature};
use ethers_signers::{LocalWallet, Signer, WalletError};
use serde::{Deserialize, Serialize};

pub struct WalletPlugin;

impl Plugin for WalletPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Wallet {
            inner: Arc::new(Box::new(LocalWallet::new(&mut rand::thread_rng()))),
        });
    }
}

#[derive(Resource, Clone)]
pub struct Wallet {
    pub(crate) inner: Arc<Box<dyn ObjSafeWalletSigner + 'static + Send + Sync>>,
}

impl Wallet {
    pub async fn sign_message<S: Send + Sync + AsRef<[u8]>>(
        &self,
        message: S,
    ) -> Result<Signature, WalletError> {
        self.inner.sign_message(message.as_ref()).await
    }

    pub fn address(&self) -> Address {
        self.inner.address()
    }
}

#[async_trait]
pub(crate) trait ObjSafeWalletSigner {
    async fn sign_message(&self, message: &[u8]) -> Result<Signature, WalletError>;

    /// Signs the transaction
    async fn sign_transaction(&self, message: &TypedTransaction) -> Result<Signature, WalletError>;

    /// Returns the signer's Ethereum Address
    fn address(&self) -> Address;

    /// Returns the signer's chain id
    fn chain_id(&self) -> u64;
}

#[async_trait]
impl ObjSafeWalletSigner for LocalWallet {
    async fn sign_message(&self, message: &[u8]) -> Result<Signature, WalletError> {
        Signer::sign_message(self, message).await
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
    pub fn new(signer_address: Address, payload: String, signature: Signature) -> Self {
        Self(vec![
            ChainLink {
                ty: "SIGNER".to_owned(),
                payload: format!("{signer_address:#x}"),
                signature: String::default(),
            },
            ChainLink {
                ty: "ECDSA_SIGNED_ENTITY".to_owned(),
                payload,
                signature: format!("0x{signature}"),
            },
        ])
    }

    pub fn headers(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.0.iter().enumerate().map(|(ix, link)| {
            (
                format!("x-identity-auth-chain-{}", ix),
                serde_json::to_string(&link).unwrap(),
            )
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct ChainLink {
    #[serde(rename = "type")]
    ty: String,
    payload: String,
    signature: String,
}
