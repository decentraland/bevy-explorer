use std::{io::ErrorKind, str::FromStr, sync::Arc, time::Duration};

use async_std::io::{Cursor, ReadExt};
use bevy::{
    asset::{
        io::{AssetReader, AssetReaderError, AssetSourceBuilder, Reader},
        AssetApp, AssetLoader,
    },
    prelude::*,
    utils::ConditionalSendFuture,
};
use common::util::reqwest_client;
use num::{BigInt, ToPrimitive};
use reqwest::StatusCode;
use serde::Deserialize;

pub struct NftReaderPlugin;

impl Plugin for NftReaderPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_source(
            "nft",
            AssetSourceBuilder::default().with_reader(|| Box::<NftReader>::default()),
        );
    }
}

#[derive(Default)]
pub struct NftReader;

impl AssetReader for NftReader {
    fn read<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> impl ConditionalSendFuture<
        Output = Result<Box<bevy::asset::io::Reader<'a>>, bevy::asset::io::AssetReaderError>,
    > {
        let path = path.to_owned();
        Box::pin(async_compat::Compat::new(async move {
            debug!("getting nft raw data");

            let path = path.to_string_lossy();
            let Some(encoded_urn) = path.split('.').next() else {
                return Err(AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    path,
                ))));
            };
            let urn = urlencoding::decode(encoded_urn).map_err(|e| {
                AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    e,
                )))
            })?;
            let urn = urn::Urn::from_str(&urn).map_err(|e| {
                AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    e,
                )))
            })?;

            if urn.nid() != "decentraland" {
                return Err(AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "nid must be `decentraland`",
                ))));
            }

            let mut parts = urn.nss().split(':');
            let (Some(chain), Some(_standard), Some(address), Some(token)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "nss must be `chain:standard:contract_address:token`",
                ))));
            };

            if !["ethereum", "matic"].contains(&chain) {
                return Err(AssetReaderError::Io(Arc::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unsupported chain `{chain}`"),
                ))));
            }

            let remote = format!("https://opensea.decentraland.org/api/v2/chain/{chain}/contract/{address}/nfts/{token}");

            let token = path;

            let mut attempt = 0;
            let data = loop {
                attempt += 1;

                let response = reqwest_client()
                    .get(&remote)
                    .timeout(Duration::from_secs(30 * attempt))
                    .send()
                    .await;

                debug!("[{token:?}]: attempt {attempt}: request: {remote}, response: {response:?}");

                let response = match response {
                    Err(e) if e.is_timeout() && attempt <= 3 => continue,
                    Err(e) => {
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!("[{token:?}]: {e}"),
                        ))))
                    }
                    Ok(response) if !matches!(response.status(), StatusCode::OK) => {
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!(
                                "[{token:?}]: server responded with status {} requesting `{}`",
                                response.status(),
                                remote,
                            ),
                        ))))
                    }
                    Ok(response) => response,
                };

                let data = response.bytes().await;

                match data {
                    Ok(data) => break data,
                    Err(e) => {
                        if e.is_timeout() && attempt <= 3 {
                            continue;
                        }
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!("[{token:?}] {e}"),
                        ))));
                    }
                }
            };

            debug!("got nft raw data");

            let reader: Box<Reader> = Box::new(Cursor::new(data));
            Ok(reader)
        }))
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> impl ConditionalSendFuture<
        Output = Result<Box<bevy::asset::io::Reader<'a>>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async { Err(AssetReaderError::NotFound(path.to_owned())) })
    }

    fn read_directory<'a>(
        &'a self,
        _: &'a std::path::Path,
    ) -> impl ConditionalSendFuture<
        Output = Result<Box<bevy::asset::io::PathStream>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async {
            panic!();
        })
    }

    fn is_directory<'a>(
        &'a self,
        _: &'a std::path::Path,
    ) -> impl ConditionalSendFuture<Output = Result<bool, bevy::asset::io::AssetReaderError>> {
        Box::pin(async { Ok(false) })
    }
}

#[derive(Deserialize)]
pub struct NftUser {
    pub username: Option<String>,
}

#[derive(Deserialize)]
pub struct NftIdent {
    pub user: Option<NftUser>,
    pub address: String,
}

impl NftIdent {
    pub fn get_string(&self) -> String {
        if let Some(user) = self.user.as_ref() {
            if let Some(name) = user.username.as_ref() {
                return format!("{} ({})", name, self.address);
            }
        }

        self.address.clone()
    }
}

#[derive(Deserialize)]
pub struct NftPaymentToken {
    pub symbol: String,
    pub eth_price: String,
    pub usd_price: String,
}

#[derive(Deserialize)]
pub struct NftLastSale {
    pub total_price: String,
    pub payment_token: Option<NftPaymentToken>,
}

impl NftLastSale {
    pub fn get_string(&self) -> Option<String> {
        let token = self.payment_token.as_ref()?;
        let big_price = BigInt::parse_bytes(self.total_price.as_bytes(), 10)?
            / BigInt::parse_bytes("10000000000000000".as_bytes(), 10)?; // 1e16
        let price = big_price.to_f32()? / 100.0; // ... 1e18 total, 2dp
        let usd_price = token.usd_price.parse::<f32>().ok()?;
        let eth_price = token.eth_price.parse::<f32>().ok()?;

        Some(format!(
            "ETH {:.2} (USD {:.2})",
            eth_price * price,
            usd_price * price
        ))
    }
}

#[derive(Deserialize)]
pub struct NftOwner {
    pub owner: NftIdent,
}

#[derive(Asset, TypePath, Deserialize)]
pub struct Nft {
    pub image_url: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub permalink: Option<String>,
    pub creator: Option<String>,
    // pub last_sale: Option<NftLastSale>,
    // pub top_ownerships: Option<Vec<NftOwner>>,
}

#[derive(Deserialize)]
pub struct NftWrapper {
    nft: Nft,
}

pub struct NftLoader;

impl AssetLoader for NftLoader {
    type Asset = Nft;
    type Settings = ();
    type Error = std::io::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _: &'a Self::Settings,
        _: &'a mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            debug!("loading nft");
            let mut bytes = Vec::default();
            reader
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| std::io::Error::new(e.kind(), e))?;

            let res = serde_json::from_reader::<_, NftWrapper>(bytes.as_slice())
                .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e));
            if res.is_err() {
                debug!("errored nft bytes: {}", String::from_utf8(bytes).unwrap());
            }
            res.map(|wrapper| wrapper.nft)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["nft"]
    }
}
