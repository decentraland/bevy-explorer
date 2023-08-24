use std::{cell::RefCell, rc::Rc};

mod byte_stream;
mod fetch_response_body_resource;

use bevy::prelude::debug;
use deno_core::{
    anyhow::anyhow,
    error::{type_error, AnyError},
    futures::TryStreamExt,
    op, AsyncRefCell, ByteString, CancelHandle, OpDecl, OpState, ResourceId, ZeroCopyBuf,
};
use deno_fetch::FetchPermissions;
use deno_web::TimersPermission;
use http::{
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, HOST, RANGE},
    HeaderName, HeaderValue, Method, Uri,
};
use isahc::{
    config::{CaCertificate, ClientCertificate, PrivateKey},
    prelude::Configurable,
    AsyncBody, AsyncReadResponseExt,
};
use serde::{Deserialize, Serialize};

use byte_stream::MpscByteStream;
use fetch_response_body_resource::{FetchRequestBodyResource, FetchResponseBodyResource};

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
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_fetch::decl(),
        op_fetch_send::decl(),
        op_fetch_custom_client::decl(),
    ]
}

struct IsahcFetchRequestResource {
    client: isahc::HttpClient,
    request: http::request::Builder,
    body: Option<MpscByteStream>,
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
    data: Option<ZeroCopyBuf>,
) -> Result<IsahcFetchReturn, AnyError> {
    let client = if let Some(rid) = client_rid {
        let r = state.resource_table.get::<IsahcClientResource>(rid)?;
        r.0.clone()
    } else if let Some(client) = state.try_borrow::<IsahcDefaultClientResource>() {
        client.0.clone()
    } else {
        state.put(IsahcDefaultClientResource(
            isahc::HttpClient::new().map_err(|e| anyhow!(e))?,
        ));
        state.borrow::<IsahcDefaultClientResource>().0.clone()
    };

    let mut request = isahc::Request::builder().uri(url.clone());
    let method = Method::from_bytes(&method)?;

    let (body, request_body_rid) = if has_body {
        let (stream, tx) = MpscByteStream::new();

        // If the size of the body is known, we include a content-length
        // header explicitly.
        if let Some(body_size) = body_length {
            request = request.header(CONTENT_LENGTH, HeaderValue::from(body_size))
        }

        // request = request.body(Body::from_reader(stream)).map_err(|e| anyhow!(e))?;

        match data {
            Some(data) => {
                tx.blocking_send(Some(data.into()))?;
                (Some(stream), None)
            }
            None => {
                let request_body_rid = state.resource_table.add(FetchRequestBodyResource {
                    body: AsyncRefCell::new(tx),
                    cancel: CancelHandle::default(),
                });
                (Some(stream), Some(request_body_rid))
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
        body,
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
        body,
    } = Rc::try_unwrap(request)
        .ok()
        .expect("multiple op_fetch_send ongoing");

    let fut = if let Some(body) = body {
        let body = AsyncBody::from_reader(body.into_async_read());
        let request = request.body(body)?;
        client.send_async(request)
    } else {
        let request = request.body(())?;
        client.send_async(request)
    };

    let mut res = match fut.await {
        Ok(res) => res,
        Err(err) => return Err(type_error(err.to_string())),
    };

    let status = res.status();
    let mut res_headers = Vec::new();
    for (key, val) in res.headers().iter() {
        res_headers.push((key.as_str().into(), val.as_bytes().into()));
    }

    let content_length = res.body().len();
    let chunk = bytes::Bytes::from(res.bytes().await?);

    let rid = state
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
        headers: res_headers,
        url: "why do you need that".into(),
        response_rid: rid,
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
    println!("custom client");
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
