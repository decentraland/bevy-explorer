#![allow(unused_imports)]

use std::{collections::HashMap, sync::Arc};

use http::HeaderValue;
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
use platform::IntoClientRequest;
use wallet::{signed_login::signed_login, SignedLoginMeta, Wallet};

mod endpoint_gate {
    pub const OPT_OUT: &str = "ALLOW_SKIPPED_INTEGRATION";

    fn skips_allowed() -> bool {
        match std::env::var(OPT_OUT) {
            Ok(v) => !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"),
            Err(_) => false,
        }
    }

    pub fn require(var: &str, what: &str) -> Option<String> {
        match std::env::var(var) {
            Ok(v) if !v.is_empty() => Some(v),
            _ => {
                if skips_allowed() {
                    eprintln!("SKIPPED: {var} unavailable ({what}); {OPT_OUT} is set");
                    return None;
                }
                panic!(
                    "integration dependency unavailable: {var}\n  \
                     {what}\n  \
                     this test asserts nothing without it, so it fails instead of passing.\n  \
                     it will not fall back to a public Decentraland endpoint: point {var} at a \
                     stack you chose, or set {OPT_OUT}=1 to let it skip on a machine that \
                     cannot run it."
                );
            }
        }
    }
}

#[test]
fn test_tls() {
    let Some(url) = endpoint_gate::require(
        "BEVY_TEST_TLS_URL",
        "an https:// URL this machine may fetch, to prove reqwest's TLS stack handshakes",
    ) else {
        return;
    };
    reqwest::blocking::get(&url).unwrap_or_else(|e| panic!("BEVY_TEST_TLS_URL {url}: {e}"));
}

#[cfg(feature = "livekit")]
#[test]
fn test_livekit() {
    use http::Uri;

    let Some(base) = endpoint_gate::require(
        "BEVY_TEST_WORLDS_CONTENT_URL",
        "a worlds-content-server base URL (no trailing slash) that will mint a LiveKit adapter",
    ) else {
        return;
    };
    let Some(world) = endpoint_gate::require(
        "BEVY_TEST_WORLD",
        "the world name to join on BEVY_TEST_WORLDS_CONTENT_URL, e.g. foo.dcl.eth",
    ) else {
        return;
    };
    let base = base.trim_end_matches('/').to_owned();

    let mut wallet = Wallet::default();
    wallet.finalize_as_guest();

    let meta = SignedLoginMeta::new(
        true,
        Uri::try_from(format!("{base}/world/{world}")).unwrap(),
    );

    let rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );

    let task = rt.spawn(async move {
        let login = signed_login(
            Uri::try_from(format!("{base}/get-comms-adapter/world-prd-{world}")).unwrap(),
            wallet,
            meta,
        )
        .await
        .unwrap();
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
        let params = HashMap::<String, String>::from_iter(
            url.query().unwrap_or_default().split('&').flat_map(|par| {
                par.split_once('=')
                    .map(|(a, b)| (a.to_owned(), b.to_owned()))
            }),
        );
        println!("{params:?}");
        let token = params.get("access_token").cloned().unwrap_or_default();

        let (room, _network_rx) = livekit::prelude::Room::connect(
            &address,
            &token,
            RoomOptions {
                auto_subscribe: true,
                adaptive_stream: false,
                dynacast: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let native_source = NativeAudioSource::new(
            AudioSourceOptions {
                echo_cancellation: true,
                noise_suppression: true,
                auto_gain_control: true,
            },
            44_100,
            1,
            None,
        );
        let mic_track = LocalTrack::Audio(LocalAudioTrack::create_audio_track(
            "mic",
            RtcAudioSource::Native(native_source.clone()),
        ));
        room.local_participant()
            .publish_track(
                mic_track,
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        println!("ok");
    });

    rt.block_on(task).unwrap();
}

#[test]
fn test_async_tls() {
    let Some(remote_address) = endpoint_gate::require(
        "BEVY_TEST_WS_URL",
        "a wss:// room URL speaking the `rfc5` subprotocol, to prove the async websocket path",
    ) else {
        return;
    };
    futures_lite::future::block_on(async move {
        let mut request = remote_address.as_str().into_client_request()?;
        request
            .headers_mut()
            .append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));
        platform::websocket(request).await
    })
    .unwrap();
}
