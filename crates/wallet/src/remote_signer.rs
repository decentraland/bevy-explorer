// A signer for an ephemeral key the engine never holds: on the web the shell page keeps the
// signed-in identity, and the engine asks it for each signature (src/web.rs installs the call).

use std::{future::Future, pin::Pin, str::FromStr, sync::OnceLock};

use alloy_core::primitives::Address;
use alloy_signer::{Error as WalletError, Signature};
use async_trait::async_trait;
use common::structs::ChainLink;

use crate::{ObjSafeWalletSigner, SimpleAuthChain};

#[cfg(target_arch = "wasm32")]
pub type RemoteSignFuture = Pin<Box<dyn Future<Output = Result<String, String>>>>;
#[cfg(not(target_arch = "wasm32"))]
pub type RemoteSignFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

/// Signs `message` (EIP-191) with the key for the ephemeral address given, resolving with the
/// 65-byte signature as hex.
pub type RemoteSign = fn(signer: String, message: String) -> RemoteSignFuture;

static REMOTE_SIGN: OnceLock<RemoteSign> = OnceLock::new();

pub fn set_remote_sign(sign: RemoteSign) {
    let _ = REMOTE_SIGN.set(sign);
}

pub fn remote_sign_available() -> bool {
    REMOTE_SIGN.get().is_some()
}

pub(crate) struct RemoteSigner {
    pub(crate) address: Address,
}

fn error(message: String) -> WalletError {
    WalletError::Other(message.into())
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ObjSafeWalletSigner for RemoteSigner {
    async fn sign_message(
        &self,
        message: String,
        root_address: Address,
        delegates: &[ChainLink],
    ) -> Result<SimpleAuthChain, WalletError> {
        let sign = REMOTE_SIGN
            .get()
            .ok_or_else(|| error("no remote signer".to_owned()))?;
        let hex = sign(format!("{:#x}", self.address), message.clone())
            .await
            .map_err(error)?;
        let signature =
            Signature::from_str(&hex).map_err(|e| error(format!("bad remote signature: {e}")))?;
        // the page signs with whatever key it holds for the address: make sure it was this one
        let recovered = signature
            .recover_address_from_msg(message.as_bytes())
            .map_err(|e| error(format!("bad remote signature: {e}")))?;
        if recovered != self.address {
            return Err(error(format!(
                "remote signature is from {recovered:#x}, not {:#x}",
                self.address
            )));
        }
        Ok(SimpleAuthChain::new(
            root_address,
            delegates,
            message,
            signature,
        ))
    }

    fn address(&self) -> Address {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use alloy_core::primitives::hex;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;

    use super::*;

    const KEY: &str = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
    // the vector react-web's shell signer is tested against too (src/test/shellSign.test.ts)
    const SOME_DATA_SIGNATURE: &str = "0xb91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c";

    fn local_sign(_signer: String, message: String) -> RemoteSignFuture {
        Box::pin(async move {
            let key = PrivateKeySigner::from_str(KEY).unwrap();
            let signature = key.sign_message_sync(message.as_bytes()).unwrap();
            Ok(hex::encode_prefixed(signature.as_bytes()))
        })
    }

    #[test]
    fn local_key_signs_like_the_shell() {
        async_std::task::block_on(async {
            assert_eq!(
                local_sign(String::new(), "Some data".to_owned())
                    .await
                    .unwrap(),
                SOME_DATA_SIGNATURE
            );
        });
    }

    #[test]
    fn remote_signer_checks_the_key() {
        set_remote_sign(local_sign);
        let key = PrivateKeySigner::from_str(KEY).unwrap();
        async_std::task::block_on(async {
            let signed = RemoteSigner {
                address: key.address(),
            }
            .sign_message("hello".to_owned(), Address::ZERO, &[])
            .await;
            assert!(signed.is_ok());

            let other = RemoteSigner {
                address: Address::repeat_byte(1),
            }
            .sign_message("hello".to_owned(), Address::ZERO, &[])
            .await;
            assert!(other.is_err());
        });
    }
}
