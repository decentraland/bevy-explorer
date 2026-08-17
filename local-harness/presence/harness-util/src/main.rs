//! presence-isolation test harness utilities. Three subcommands:
//! - `address --seed N`: print the guest address a `headless --wallet-seed N` will use
//! - `token --room R --identity I [--key K --secret S]`: mint a livekit dev-server JWT
//! - `bus --url U --seed N --room R --scene-hash H --message M`: join a room and publish
//!   an rfc4 scene bus packet declaring `scene-hash` (equal to the room's scene for a
//!   legitimate message, different for a forgery attempt)

use anyhow::{anyhow, Context};

fn seed_wallet(seed: u64) -> wallet::Wallet {
    // must match headless.rs finalize_guest_wallet: seed LE bytes into [0..8] of [u8; 32]
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    let mut wallet = wallet::Wallet::default();
    wallet.finalize_as_guest_with_seed(bytes);
    wallet
}

fn wallet_address(seed: u64) -> anyhow::Result<String> {
    let wallet = seed_wallet(seed);
    Ok(format!(
        "{:#x}",
        wallet.address().ok_or_else(|| anyhow!("no address"))?
    ))
}

fn mint_token(room: &str, identity: &str, key: &str, secret: &str) -> anyhow::Result<String> {
    Ok(
        livekit_api::access_token::AccessToken::with_api_key(key, secret)
            .with_identity(identity)
            .with_name(identity)
            .with_grants(livekit_api::access_token::VideoGrants {
                room_join: true,
                room: room.to_owned(),
                ..Default::default()
            })
            .to_jwt()?,
    )
}

async fn send_bus(url: &str, token: &str, scene_hash: &str, message: &str) -> anyhow::Result<()> {
    use dcl_component::proto_components::kernel::comms::rfc4;
    use prost::Message as _;

    let (room, _events) = livekit::Room::connect(
        url,
        token,
        livekit::RoomOptions {
            auto_subscribe: false,
            adaptive_stream: false,
            dynacast: false,
            ..Default::default()
        },
    )
    .await
    .context("room connect")?;

    // Wait for the server participant to appear before publishing: a reliable data packet
    // sent before the peer connection to it is established is silently dropped.
    for _ in 0..50 {
        if !room.remote_participants().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // give the data channel a moment to finish negotiating after the participant appears
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("bus peers: {}", room.remote_participants().len());

    // 1-byte CommsMessageType::String prefix, then utf8 payload
    let mut data = vec![1u8];
    data.extend_from_slice(message.as_bytes());
    let packet = rfc4::Packet {
        message: Some(rfc4::packet::Message::Scene(rfc4::Scene {
            scene_id: scene_hash.to_owned(),
            data,
        })),
        protocol_version: 100,
    };

    // publish a few times over a couple of seconds: reliable delivery still needs the
    // data channel up, and a synthetic one-shot sender has no retransmit loop of its own
    for _ in 0..5 {
        room.local_participant()
            .publish_data(livekit::DataPacket {
                payload: packet.encode_to_vec(),
                reliable: true,
                ..Default::default()
            })
            .await
            .map_err(|e| anyhow!("publish failed: {e}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    room.close().await.ok();
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let sub = args
        .subcommand()?
        .ok_or_else(|| anyhow!("usage: harness-util <address|token|bus> ..."))?;

    match sub.as_str() {
        "address" => {
            let seed: u64 = args.value_from_str("--seed")?;
            println!("{}", wallet_address(seed)?);
        }
        "token" => {
            let room: String = args.value_from_str("--room")?;
            let identity: String = match args.opt_value_from_str("--identity")? {
                Some(identity) => identity,
                None => wallet_address(args.value_from_str("--seed")?)?,
            };
            let key: String = args
                .opt_value_from_str("--key")?
                .unwrap_or_else(|| "devkey".to_owned());
            let secret: String = args
                .opt_value_from_str("--secret")?
                .unwrap_or_else(|| "secret".to_owned());
            println!("{}", mint_token(&room, &identity, &key, &secret)?);
        }
        "bus" => {
            let url: String = args.value_from_str("--url")?;
            let room: String = args.value_from_str("--room")?;
            let seed: u64 = args.value_from_str("--seed")?;
            let scene_hash: String = args.value_from_str("--scene-hash")?;
            let message: String = args.value_from_str("--message")?;
            let key: String = args
                .opt_value_from_str("--key")?
                .unwrap_or_else(|| "devkey".to_owned());
            let secret: String = args
                .opt_value_from_str("--secret")?
                .unwrap_or_else(|| "secret".to_owned());
            let identity = wallet_address(seed)?;
            let token = mint_token(&room, &identity, &key, &secret)?;
            println!("bus sender: {identity}");
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(send_bus(&url, &token, &scene_hash, &message))?;
        }
        other => return Err(anyhow!("unknown subcommand {other}")),
    }
    Ok(())
}
