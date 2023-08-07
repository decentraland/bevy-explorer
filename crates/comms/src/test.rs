// use async_tungstenite::tungstenite::client::IntoClientRequest;
// use isahc::http::HeaderValue;

#[test]
fn test_tls() {
    let _ = futures_lite::future::block_on(surf::get("https://www.google.com/")).unwrap();
}

// #[test]
// fn test_async_tls() {
//     futures_lite::future::block_on(async move {
//         let remote_address = "wss://sdk-test-scenes.decentraland.zone/mini-comms/room-1";
//         let mut request = remote_address.into_client_request()?;
//         request
//             .headers_mut()
//             .append("Sec-WebSocket-Protocol", HeaderValue::from_static("rfc5"));
//         async_tungstenite::async_std::connect_async(request).await
//     })
//     .unwrap();
// }
