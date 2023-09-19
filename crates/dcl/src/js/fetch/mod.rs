use std::{cell::RefCell, rc::Rc};

mod byte_stream;
mod fetch_response_body_resource;

use bevy::prelude::{debug, AssetServer};
use deno_core::{
    error::{type_error, AnyError},
    futures::TryStreamExt,
    op, AsyncRefCell, ByteString, CancelHandle, JsBuffer, Op, OpDecl, OpState, ResourceId, anyhow,
};
use deno_fetch::FetchPermissions;
use deno_web::TimersPermission;
use http::{
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, HOST, RANGE},
    HeaderName, HeaderValue, Method, Uri,
};
use ipfs::IpfsLoaderExt;
use isahc::{
    config::{CaCertificate, ClientCertificate, PrivateKey},
    prelude::Configurable,
    AsyncBody, AsyncReadResponseExt,
};
use serde::{Deserialize, Serialize};

use byte_stream::MpscByteStream;
use fetch_response_body_resource::{FetchRequestBodyResource, FetchResponseBodyResource};
use wallet::{sign_request, Wallet};

// we have to provide fetch perm structs even though we don't use them
pub struct FP;
impl FetchPermissions for FP {
    fn check_net_url(&mut self, _: &deno_core::url::Url, _: &str) -> Result<(), AnyError> {
        panic!();
    }

    fn check_read(&mut self, _: &std::path::Path, _: &str) -> Result<(), AnyError> {
        panic!();
    }
}

pub struct TP;
impl TimersPermission for TP {
    fn allow_hrtime(&mut self) -> bool {
        false
    }

    fn check_unstable(&self, _: &OpState, _: &'static str) {
        panic!("i don't know what this is for")
    }
}

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![
        op_fetch::DECL,
        op_fetch_send::DECL,
        op_fetch_custom_client::DECL,
    ]
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_signed_fetch_headers::DECL]
}

struct IsahcFetchRequestResource {
    client: Option<isahc::HttpClient>,
    request: http::request::Builder,
    body_stream: Option<MpscByteStream>,
    body_bytes: Option<Vec<u8>>,
}
impl deno_core::Resource for IsahcFetchRequestResource {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsahcFetchReturn {
    request_rid: ResourceId,
    request_body_rid: Option<ResourceId>,
    cancel_handle_rid: Option<ResourceId>,
}

#[op]
pub fn op_fetch(
    state: &mut OpState,
    method: ByteString,
    url: String,
    headers: Vec<(ByteString, ByteString)>,
    client_rid: Option<u32>,
    has_body: bool,
    body_length: Option<u64>,
    data: Option<JsBuffer>,
) -> Result<IsahcFetchReturn, AnyError> {
    // TODO scene permissions

    let client = if let Some(rid) = client_rid {
        let r = state.resource_table.get::<IsahcClientResource>(rid)?;
        Some(r.0.clone())
    } else {
        None
    };

    if Uri::try_from(&url)?.scheme_str() != Some("https") {
        anyhow::bail!("URL scheme must be `https`")
    }

    let mut request = isahc::Request::builder().uri(url.clone());
    let method = Method::from_bytes(&method)?;

    let (body_stream, request_body_rid, body_bytes) = if has_body {
        let (stream, tx) = MpscByteStream::new();

        // If the size of the body is known, we include a content-length
        // header explicitly.
        if let Some(body_size) = body_length {
            request = request.header(CONTENT_LENGTH, HeaderValue::from(body_size))
        }

        match data {
            Some(data) => (None, None, Some(data.to_vec())),
            None => {
                let request_body_rid = state.resource_table.add(FetchRequestBodyResource {
                    body: AsyncRefCell::new(tx),
                    cancel: CancelHandle::default(),
                });
                (Some(stream), Some(request_body_rid), None)
            }
        }
    } else {
        // POST and PUT requests should always have a 0 length content-length,
        // if there is no body. https://fetch.spec.whatwg.org/#http-network-or-cache-fetch
        if matches!(method, Method::POST | Method::PUT) {
            request = request.header(CONTENT_LENGTH, HeaderValue::from(0));
        }
        (None, None, None)
    };

    request = request.method(method);

    for (key, value) in headers {
        let name = HeaderName::from_bytes(&key).map_err(|err| type_error(err.to_string()))?;
        let v = HeaderValue::from_bytes(&value).map_err(|err| type_error(err.to_string()))?;

        if matches!(name, RANGE) {
            request = request.header(name, v);
            // https://fetch.spec.whatwg.org/#http-network-or-cache-fetch step 18
            // If httpRequest’s header list contains `Range`, then append (`Accept-Encoding`, `identity`)
            request = request.header(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        } else if !matches!(name, HOST | CONTENT_LENGTH) {
            request = request.header(name, v);
        }
    }

    let request_rid = state.resource_table.add(IsahcFetchRequestResource {
        body_stream,
        body_bytes,
        client,
        request,
    });

    debug!(
        "request {url}, returning {:?}/{:?}",
        request_rid, request_body_rid
    );
    Ok(IsahcFetchReturn {
        request_rid,
        request_body_rid,
        cancel_handle_rid: None,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchResponse {
    status: u16,
    status_text: String,
    headers: Vec<(ByteString, ByteString)>,
    url: String,
    response_rid: ResourceId,
    content_length: Option<u64>,
}

#[op]
pub async fn op_fetch_send(
    state: Rc<RefCell<OpState>>,
    rid: ResourceId,
) -> Result<FetchResponse, AnyError> {
    let request = state
        .borrow_mut()
        .resource_table
        .take::<IsahcFetchRequestResource>(rid)?;

    let IsahcFetchRequestResource {
        client,
        request,
        body_stream,
        body_bytes,
    } = Rc::try_unwrap(request)
        .ok()
        .expect("multiple op_fetch_send ongoing");

    let asset_server = state.borrow_mut().borrow_mut::<AssetServer>().clone();

    let async_req = if let Some(body) = body_stream {
        let request = request.body(AsyncBody::from_reader(body.into_async_read()))?;
        asset_server.ipfs().async_request(request, client).await
    } else if let Some(body) = body_bytes {
        let request = request.body(body)?;
        asset_server.ipfs().async_request(request, client).await
    } else {
        let request = request.body(())?;
        asset_server.ipfs().async_request(request, client).await
    };

    let mut res = match async_req {
        Ok(res) => res,
        Err(err) => return Err(type_error(err.to_string())),
    };

    let status = res.status();
    let mut headers = Vec::new();
    for (key, val) in res.headers().iter() {
        headers.push((key.as_str().into(), val.as_bytes().into()));
    }

    let content_length = res.body().len();
    let chunk = bytes::Bytes::from(res.bytes().await?);

    let response_rid = state
        .borrow_mut()
        .resource_table
        .add(FetchResponseBodyResource {
            data: AsyncRefCell::new(chunk),
            cancel: CancelHandle::default(),
            size: content_length,
        });

    Ok(FetchResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        url: "why do you need that".into(),
        response_rid,
        content_length,
    })
}

// copy out the args struct so we can access the members...
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreateHttpClientOptions {
    ca_certs: Vec<String>,
    proxy: Option<Proxy>,
    cert_chain: Option<String>,
    private_key: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct Proxy {
    pub url: String,
    pub basic_auth: Option<BasicAuth>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

pub struct IsahcClientResource(isahc::HttpClient);
impl deno_core::Resource for IsahcClientResource {}

pub struct IsahcDefaultClientResource(isahc::HttpClient);
impl deno_core::Resource for IsahcDefaultClientResource {}

#[op]
pub fn op_fetch_custom_client(
    state: &mut OpState,
    args: CreateHttpClientOptions,
) -> Result<ResourceId, AnyError> {
    let mut builder = isahc::HttpClient::builder();
    if let Some(proxy) = args.proxy {
        builder = builder.proxy(Uri::try_from(proxy.url).ok());
        if let Some(creds) = proxy.basic_auth {
            builder = builder.proxy_credentials(isahc::auth::Credentials::new(
                creds.username,
                creds.password,
            ));
        }
    }
    if !args.ca_certs.is_empty() {
        let bytes = args.ca_certs.join("");
        builder = builder.ssl_ca_certificate(CaCertificate::pem(bytes));
    }
    if let (Some(chain), Some(key)) = (args.cert_chain, args.private_key) {
        builder = builder
            .ssl_client_certificate(ClientCertificate::pem(chain, PrivateKey::pem(key, None)));
    }

    Ok(state
        .resource_table
        .add(IsahcClientResource(builder.build()?)))
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMetaRealm {
    domain: Option<String>,
    catalyst_name: Option<String>,
    layer: Option<String>,
    lighthouse_version: Option<String>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMeta {
    origin: Option<String>,
    scene_id: Option<String>,
    parcel: Option<String>,
    tld: Option<String>,
    network: Option<String>,
    is_guest: Option<bool>,
    realm: SignedFetchMetaRealm,
}

#[op]
pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<OpState>>,
    uri: String,
    method: Option<String>,
) -> Result<Vec<(String, String)>, AnyError> {
    let wallet = state.borrow().borrow::<Wallet>().clone();

    let meta = SignedFetchMeta {
        origin: Some("localhost".to_owned()),
        is_guest: Some(true),
        ..Default::default()
    };

    let headers = sign_request(
        method.as_deref().unwrap_or("get"),
        &Uri::try_from(uri)?,
        &wallet,
        meta,
    )
    .await;

    Ok(headers)
}
