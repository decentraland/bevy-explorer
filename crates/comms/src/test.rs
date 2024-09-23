#![allow(unused_imports)]

use std::{collections::HashMap, sync::Arc};

use async_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::{HeaderValue, Uri},
};
#[cfg(feature = "livekit")]
use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioSourceOptions, RtcAudioSource},
    },
    RoomOptions,
};
use wallet::{signed_login::signed_login, SignedLoginMeta, Wallet};

#[test]
fn test_tls() {
    let _ = isahc::get("https://www.google.com/").unwrap();
}

#[cfg(feature = "livekit")]
#[test]
fn test_livekit() {
    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();

    let meta = SignedLoginMeta::new(
        true,
        Uri::try_from("https://worlds-content-server.decentraland.org/world/mannakia.dcl.eth")
            .unwrap(),
    );

    let rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );

    let task = rt.spawn(async move {
        let login = signed_login(Uri::try_from("https://worlds-content-server.decentraland.org/get-comms-adapter/world-prd-mannakia.dcl.eth").unwrap(), wallet, meta).await.unwrap();
        let adapter = login.fixed_adapter.unwrap();
        let (protocol, remote_address) = adapter.split_once(':').unwrap();
        assert_eq!(protocol, "livekit");

        let url = Uri::try_from(remote_address).unwrap();
        let address = format!(
            "{}://{}{}",
            url.scheme_str().unwrap_or_default(),
            url.host().unwrap_or_default(),
            url.path()
        );
        let params = HashMap::<String, String>::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
            par.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
        }));
        println!("{params:?}");
        let token = params.get("access_token").cloned().unwrap_or_default();

        let (room, _network_rx) = livekit::prelude::Room::connect(&address, &token, RoomOptions{ auto_subscribe: true, adaptive_stream: false, dynacast: false, ..Default::default() }).await.unwrap();
        let native_source = NativeAudioSource::new(AudioSourceOptions{
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
        }, 44_100, 1, None);
        let mic_track = LocalTrack::Audio(LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Native(native_source.clone())));
        room.local_participant().publish_track(mic_track, TrackPublishOptions{ source: TrackSource::Microphone, ..Default::default() }).await.unwrap();
        println!("ok");
    });

    rt.block_on(task).unwrap();
}

#[test]
fn test_async_tls() {
    futures_lite::future::block_on(async move {
        let remote_address = "wss://sdk-test-scenes.decentraland.zone/mini-comms/room-1";
        let mut request = remote_address.into_client_request()?;
        request
            .headers_mut()
            .append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));
        async_tungstenite::async_std::connect_async(request).await
    })
    .unwrap();
}
