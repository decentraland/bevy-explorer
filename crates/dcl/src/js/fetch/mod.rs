use std::{cell::RefCell, rc::Rc};

mod fetch_response_body_resource;

use bevy::prelude::debug;
use common::structs::SceneMeta;
use deno_core::{
    anyhow::{self, anyhow},
    error::{type_error, AnyError},
    futures::{FutureExt, TryStreamExt},
    op2, AsyncRefCell, BufView, ByteString, CancelHandle, JsBuffer, OpDecl, OpState, Resource,
    ResourceId,
};
use deno_fetch::FetchPermissions;
use deno_net::NetPermissions;
use deno_web::TimersPermission;
use http::{
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, HOST, RANGE},
    HeaderName, HeaderValue, Method, Uri,
};
use ipfs::IpfsResource;
use isahc::{
    config::{CaCertificate, ClientCertificate, PrivateKey},
    prelude::Configurable,
    AsyncBody, AsyncReadResponseExt,
};
use serde::{Deserialize, Serialize};

use fetch_response_body_resource::FetchResponseBodyResource;
use wallet::{sign_request, Wallet};

use crate::interface::crdt_context::CrdtContext;

use super::runtime::realm_information;

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
}

pub struct NP;
impl NetPermissions for NP {
    fn check_net<T: AsRef<str>>(
        &mut self,
        _host: &(T, Option<u16>),
        _api_name: &str,
      ) -> Result<(), AnyError> {
        Ok(())
    }

    fn check_read(&mut self, _p: &std::path::Path, _api_name: &str) -> Result<(), AnyError> {
        Ok(())
    }

    fn check_write(&mut self, _p: &std::path::Path, _api_name: &str)
        -> Result<(), AnyError> {
        Ok(())
    }
}

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![
        op_fetch(),
        op_fetch_send(),
        op_fetch_custom_client(),
    ]
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_signed_fetch_headers()]
}

struct IsahcFetchRequestResource {
    client: Option<isahc::HttpClient>,
    request: http::request::Builder,
    request_body_rid: Option<ResourceId>,
    body_bytes: Option<Vec<u8>>,
}
impl deno_core::Resource for IsahcFetchRequestResource {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsahcFetchReturn {
    request_rid: ResourceId,
    cancel_handle_rid: Option<ResourceId>,
}

#[op2]
#[serde]
#[allow(clippy::too_many_arguments)]
pub fn op_fetch(
    state: &mut OpState,
    #[serde] method: ByteString,
    #[string] url: String,
    #[serde] headers: Vec<(ByteString, ByteString)>,
    #[smi] client_rid: Option<u32>,
    has_body: bool,
    #[buffer] data: Option<JsBuffer>,
    #[smi] resource: Option<ResourceId>,
) -> Result<IsahcFetchReturn, AnyError> {
    // TODO scene permissions

    let client = if let Some(rid) = client_rid {
        let r = state.resource_table.get::<IsahcClientResource>(rid)?;
        Some(r.0.clone())
    } else {
        None
    };

    let mut request = isahc::Request::builder().uri(url.clone());
    let method = Method::from_bytes(&method)?;

    let (request_body_rid, body_bytes) = if has_body {
        match (data, resource) {
            (None, None) => unreachable!(),
            (Some(data), _) => (None, Some(data.to_vec())),
            (_, Some(resource_id)) => {
                let resource = state.resource_table.get_any(resource_id)?;
                match resource.size_hint() {
                    (body_size, Some(n)) if body_size == n && body_size > 0 => {
                        request = request.header(CONTENT_LENGTH, HeaderValue::from(body_size));
                    }
                    _ => {}
                }

                (Some(resource_id), None)
            }
        }
    } else {
        // POST and PUT requests should always have a 0 length content-length,
        // if there is no body. https://fetch.spec.whatwg.org/#http-network-or-cache-fetch
        if matches!(method, Method::POST | Method::PUT) {
            request = request.header(CONTENT_LENGTH, HeaderValue::from(0));
        }
        (None, None)
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
        body_bytes,
        client,
        request_body_rid,
        request,
    });

    debug!("request {url}, returning {:?}", request_rid);
    Ok(IsahcFetchReturn {
        request_rid,
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
    pub remote_addr_ip: Option<String>,
    pub remote_addr_port: Option<u16>,
    pub error: Option<String>,
}

#[op2(async)]
#[serde]
pub async fn op_fetch_send(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
) -> Result<FetchResponse, AnyError> {
    let request = state
        .borrow_mut()
        .resource_table
        .take::<IsahcFetchRequestResource>(rid)?;

    let IsahcFetchRequestResource {
        client,
        request,
        body_bytes,
        request_body_rid,
    } = Rc::try_unwrap(request)
        .ok()
        .expect("multiple op_fetch_send ongoing");

    let ipfs = state.borrow_mut().borrow_mut::<IpfsResource>().clone();

    let async_req = if let Some(body_id) = request_body_rid {
        let body = state.borrow_mut().resource_table.take_any(body_id)?;
        let request = request.body(AsyncBody::from_reader(
            ResourceToBodyAdapter::new(body).into_async_read(),
        ))?;
        ipfs.async_request(request, client).await
    } else if let Some(body) = body_bytes {
        let request = request.body(body)?;
        ipfs.async_request(request, client).await
    } else {
        let request = request.body(())?;
        ipfs.async_request(request, client).await
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

    debug!("request response [{:?} bytes]", content_length);
    Ok(FetchResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        url: "why do you need that".into(),
        response_rid,
        content_length,
        remote_addr_ip: None,
        remote_addr_port: None,
        error: None,
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

#[op2]
#[serde]
pub fn op_fetch_custom_client(
    state: &mut OpState,
    #[serde] args: CreateHttpClientOptions,
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

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMetaRealm {
    hostname: String,
    protocol: String,
    server_name: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMeta {
    origin: Option<String>,
    scene_id: Option<String>,
    parcel: Option<String>,
    tld: Option<String>,
    network: Option<String>,
    is_guest: Option<bool>,
    realm: SignedFetchMetaRealm,
    signer: String,
}

#[op2(async)]
#[serde]
pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<OpState>>,
    #[string] uri: String,
    #[string] method: Option<String>,
) -> Result<Vec<(String, String)>, AnyError> {
    if Uri::try_from(&uri)?.scheme_str() != Some("https") {
        anyhow::bail!("URL scheme must be `https`")
    }

    let realm_info = realm_information(state.clone()).await?;
    let wallet = state.borrow().borrow::<Wallet>().clone();
    let urn = state.borrow().borrow::<CrdtContext>().hash.clone();
    let ipfs = state.borrow().borrow::<IpfsResource>().clone();
    let scene_meta = ipfs
        .entity_definition(&urn)
        .await
        .and_then(|(entity, _)| {
            serde_json::from_str::<SceneMeta>(&entity.metadata.unwrap_or_default()).ok()
        })
        .ok_or(anyhow!("failed to parse scene metadata"))?;

    let meta = SignedFetchMeta {
        origin: Some(realm_info.base_url.clone()),
        scene_id: Some(urn),
        parcel: Some(scene_meta.scene.base.clone()),
        tld: Some("org".to_owned()),
        network: Some("mainnet".to_owned()),
        is_guest: Some(wallet.is_guest()),
        realm: SignedFetchMetaRealm {
            hostname: realm_info.base_url,
            protocol: "v3".to_owned(),
            server_name: realm_info.realm_name,
        },
        signer: "decentraland-kernel-scene".to_owned(),
    };

    debug!("signed fetch meta {:?}", meta);

    sign_request(
        method.as_deref().unwrap_or("get"),
        &Uri::try_from(uri)?,
        &wallet,
        meta,
    )
    .await
}

use core::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

#[allow(clippy::type_complexity)]
pub struct ResourceToBodyAdapter(
    Rc<dyn Resource>,
    Option<Pin<Box<dyn Future<Output = Result<BufView, anyhow::Error>>>>>,
);

impl ResourceToBodyAdapter {
    pub fn new(resource: Rc<dyn Resource>) -> Self {
        let future = resource.clone().read(64 * 1024);
        Self(resource, Some(future))
    }
}

// SAFETY: we only use this on a single-threaded executor
unsafe impl Send for ResourceToBodyAdapter {}
// SAFETY: we only use this on a single-threaded executor
unsafe impl Sync for ResourceToBodyAdapter {}

impl deno_core::futures::Stream for ResourceToBodyAdapter {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Some(mut fut) = this.1.take() {
            match fut.poll_unpin(cx) {
                Poll::Pending => {
                    this.1 = Some(fut);
                    Poll::Pending
                }
                Poll::Ready(res) => match res {
                    Ok(buf) if buf.is_empty() => Poll::Ready(None),
                    Ok(_) => {
                        this.1 = Some(this.0.clone().read(64 * 1024));
                        Poll::Ready(Some(
                            res.map(|b| b.to_vec().into())
                                .map_err(std::io::Error::other),
                        ))
                    }
                    _ => Poll::Ready(Some(
                        res.map(|b| b.to_vec().into())
                            .map_err(std::io::Error::other),
                    )),
                },
            }
        } else {
            Poll::Ready(None)
        }
    }
}
