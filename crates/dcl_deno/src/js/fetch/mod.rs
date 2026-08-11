use std::{cell::RefCell, rc::Rc, time::Duration};

mod fetch_response_body_resource;

use bevy::{asset::AsyncReadExt, prelude::debug};
use common::rpc::{RpcCall, RpcResultSender};
use deno_core::{
    anyhow,
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
    HeaderName, HeaderValue, Method,
};
use serde::{Deserialize, Serialize};

use fetch_response_body_resource::FetchResponseBodyResource;

use dcl::{interface::crdt_context::CrdtContext, RpcCalls, SceneResourceCounters};

// We have to provide these perm structs for the deno extensions even though the ops we
// actually expose don't route through them. They DENY rather than panic: the ops that
// consult them (`deno_net`'s socket ops in particular) are registered on the runtime and
// callable from JS, and a `panic!()` there unwinds across the V8 boundary -- which is
// `panic_cannot_unwind`, i.e. an immediate process abort, not a catchable error. Scene
// input must never be able to reach that, so refusing is the only safe answer.
pub struct FP;
impl FetchPermissions for FP {
    fn check_net_url(&mut self, _: &deno_core::url::Url, _: &str) -> Result<(), AnyError> {
        anyhow::bail!("network access is not available to scenes through this API")
    }

    fn check_read(&mut self, _: &std::path::Path, _: &str) -> Result<(), AnyError> {
        anyhow::bail!("file access is not available to scenes")
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
        anyhow::bail!("raw socket access is not available to scenes")
    }

    fn check_read(&mut self, _p: &std::path::Path, _api_name: &str) -> Result<(), AnyError> {
        anyhow::bail!("file access is not available to scenes")
    }

    fn check_write(&mut self, _p: &std::path::Path, _api_name: &str) -> Result<(), AnyError> {
        anyhow::bail!("file access is not available to scenes")
    }
}

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![op_fetch::<FP>(), op_fetch_send(), op_fetch_custom_client()]
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_signed_fetch_headers()]
}

struct FetchRequestResource {
    client: reqwest::Client,
    request: reqwest::RequestBuilder,
    request_body_rid: Option<ResourceId>,
    body_bytes: Option<Vec<u8>>,
    url: String,
    /// URL host is the world-storage service (`storage.decentraland.*`).
    is_storage: bool,
}
impl deno_core::Resource for FetchRequestResource {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsahcFetchReturn {
    request_rid: ResourceId,
    cancel_handle_rid: Option<ResourceId>,
}

#[op2]
#[serde]
#[allow(clippy::too_many_arguments)]
pub fn op_fetch<FP>(
    state: &mut OpState,
    #[string] method: String,
    #[string] url: String,
    #[serde] headers: Vec<(String, String)>,
    #[smi] client_rid: Option<u32>,
    has_body: bool,
    #[buffer] data: Option<JsBuffer>,
    #[smi] resource: Option<ResourceId>,
) -> Result<IsahcFetchReturn, AnyError>
where
    FP: FetchPermissions + 'static,
{
    debug!("op_fetch");
    // TODO scene permissions

    let is_storage = deno_core::url::Url::parse(&url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| {
                h.strip_prefix("storage.decentraland.")
                    .is_some_and(|tld| !tld.contains('.'))
            })
        })
        .unwrap_or(false);

    {
        let counters = state.borrow_mut::<SceneResourceCounters>();
        counters.fetch_started += 1;
        if is_storage {
            counters.storage_requests += 1;
        }
    }

    // On the authoritative server redirects are never auto-followed, so a 3xx onto a private
    // host can't bypass the per-request SSRF check or leak signed headers. The desktop/web
    // client still follows them, but under the public-only redirect policy (see
    // `build_scene_client` / `public_only_redirect`).
    let (is_server, preview) = {
        let ctx = state.borrow::<CrdtContext>();
        (ctx.is_server, ctx.preview)
    };

    let client = if let Some(rid) = client_rid {
        let r = state.resource_table.get::<ClientResource>(rid)?;
        r.0.clone()
    } else {
        // One guarded default client for both modes: public-only DNS (unless preview), so a
        // scene can never reach loopback / private / metadata from a plain `fetch()`.
        match state.try_borrow::<SceneHttpClient>() {
            Some(client) => client.0.clone(),
            None => {
                let client = build_scene_client(preview, is_server);
                state.put(SceneHttpClient(client.clone()));
                client
            }
        }
    };

    if method.len() > 50 {
        debug!("bad method {}", method.len());
        anyhow::bail!("nope");
    }

    let method = Method::from_bytes(method.as_bytes())?;
    let mut request = client.request(method.clone(), &url);

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

    for (key, value) in headers {
        let name =
            HeaderName::from_bytes(key.as_bytes()).map_err(|err| type_error(err.to_string()))?;
        let v =
            HeaderValue::from_bytes(value.as_bytes()).map_err(|err| type_error(err.to_string()))?;

        if matches!(name, RANGE) {
            request = request.header(name, v);
            // https://fetch.spec.whatwg.org/#http-network-or-cache-fetch step 18
            // If httpRequest’s header list contains `Range`, then append (`Accept-Encoding`, `identity`)
            request = request.header(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        } else if !matches!(name, HOST | CONTENT_LENGTH) {
            request = request.header(name, v);
        }
    }

    request = request.header("User-Agent", "DCLExplorer/0.1");

    debug!("request {url}");
    let request_rid = state.resource_table.add(FetchRequestResource {
        body_bytes,
        client,
        request_body_rid,
        request,
        url,
        is_storage,
    });

    debug!("returning {:?}", request_rid);
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
    // copy the flag out before fetch_send_inner takes (and try_unwraps) the resource
    let is_storage = state
        .borrow()
        .resource_table
        .get::<FetchRequestResource>(rid)
        .map(|r| r.is_storage)
        .unwrap_or(false);
    let result = fetch_send_inner(state.clone(), rid).await;
    let mut op_state = state.borrow_mut();
    let counters = op_state.borrow_mut::<SceneResourceCounters>();
    match &result {
        Ok(response) => {
            counters.fetch_completed += 1;
            if is_storage {
                counters.storage_completed += 1;
                if matches!(response.status, 401 | 403) {
                    counters.storage_unauthorized += 1;
                }
            }
        }
        Err(_) => {
            counters.fetch_failed += 1;
            if is_storage {
                counters.storage_failed += 1;
            }
        }
    }
    result
}

async fn fetch_send_inner(
    state: Rc<RefCell<OpState>>,
    rid: ResourceId,
) -> Result<FetchResponse, AnyError> {
    debug!("op_fetch_send");
    let request = state
        .borrow_mut()
        .resource_table
        .take::<FetchRequestResource>(rid)?;

    let FetchRequestResource {
        client,
        request,
        body_bytes,
        request_body_rid,
        url,
        is_storage: _,
    } = Rc::try_unwrap(request)
        .ok()
        .expect("multiple op_fetch_send ongoing");

    let scene = state.borrow_mut().borrow::<CrdtContext>().scene_id.0;
    let (sx, rx) = RpcResultSender::channel();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::RequestGenericPermission {
            scene,
            ty: common::structs::PermissionType::Fetch,
            message: Some(url.clone()),
            response: sx,
        })?;
    let permit = rx.await?;
    if !permit {
        anyhow::bail!("User denied fetch request");
    }

    // SSRF guard — every scene, every mode. No deployed scene may reach cloud metadata /
    // loopback / private ranges, on the shared server OR the desktop client (a scene is
    // untrusted code with the user's network position — the web build is already held to
    // this by the browser's Private Network Access rules). `preview` widens the allowance
    // to the local network for local development, but never to link-local / metadata.
    let preview = {
        let op_state = state.borrow();
        op_state.borrow::<CrdtContext>().preview
    };
    common::util::assert_public_url(&url, preview).await?;

    let async_req = if let Some(body_id) = request_body_rid {
        let body = state.borrow_mut().resource_table.take_any(body_id)?;
        let mut buf = Vec::new();
        ResourceToBodyAdapter::new(body)
            .into_async_read()
            .read_to_end(&mut buf)
            .await?;
        let request = request.body(buf).build()?;
        client.execute(request).await
    } else if let Some(body) = body_bytes {
        let request = request.body(body).build()?;
        client.execute(request).await
    } else {
        let request = request.build()?;
        client.execute(request).await
    };

    let res = match async_req {
        Ok(res) => res,
        Err(err) => return Err(type_error(err.to_string())),
    };

    let status = res.status();
    let mut headers = Vec::new();
    for (key, val) in res.headers().iter() {
        headers.push((key.as_str().into(), val.as_bytes().into()));
    }

    let content_length = res.content_length();
    let chunk = res.bytes().await?;

    state
        .borrow_mut()
        .borrow_mut::<SceneResourceCounters>()
        .fetch_bytes_down += chunk.len() as u64;

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
        url,
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

pub struct ClientResource(reqwest::Client);
impl deno_core::Resource for ClientResource {}

/// Cached default client for a scene's own requests, in both server and client mode. Its
/// connections are public-only (see [`PublicOnlyResolver`]) unless `preview` widened them to
/// the local network; on the server, redirects are disabled too.
struct SceneHttpClient(reqwest::Client);

/// DNS resolver that only ever hands back public addresses.
///
/// `assert_public_url` runs before the request is sent, but the client resolves the host
/// AGAIN when it connects — nothing ties the two lookups together. A hostile authoritative
/// nameserver can therefore answer the pre-flight check with a public address and the
/// connect with 169.254.169.254 (DNS rebinding), and no scene-side trickery is needed to
/// reach it: plain `fetch()` from the SDK is enough. Enforcing inside the resolver makes
/// the checked answer and the dialled answer the same answer by construction.
struct PublicOnlyResolver {
    allow_private: bool,
}

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let allow_private = self.allow_private;
        let host = name.as_str().to_owned();
        Box::pin(async move {
            // port 0: reqwest fills in the real port after resolution
            let addrs = common::util::resolve_public_addrs(&host, 0, allow_private)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    e.to_string().into()
                })?;
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Shared base for every scene http client: connect timeout, native TLS, UA, and the
/// resolver that refuses any non-public address at connect time, so the address that was
/// checked is the address that is dialled. `allow_private` (preview) widens this to the
/// local network but never to link-local / metadata.
fn scene_client_builder(allow_private: bool) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .use_native_tls()
        .user_agent("DCLExplorer/0.1")
        .dns_resolver(std::sync::Arc::new(PublicOnlyResolver { allow_private }))
}

/// The default client for a scene's own requests. On the authoritative server redirects are
/// disabled entirely (a 3xx must not silently re-target or forward signed headers). The
/// desktop client keeps following them — matching the browser — but through
/// [`public_only_redirect`]: a hostname hop is re-resolved by [`PublicOnlyResolver`] at
/// connect, and an IP-literal hop (dialled with no DNS lookup, so the resolver never sees it)
/// is vetted by the policy.
fn build_scene_client(allow_private: bool, is_server: bool) -> reqwest::Client {
    scene_client_builder(allow_private)
        .redirect(if is_server {
            reqwest::redirect::Policy::none()
        } else {
            public_only_redirect(allow_private)
        })
        .build()
        .unwrap()
}

/// Parse a URL host component as an IP literal, tolerating the brackets the `url` crate keeps
/// around an IPv6 literal (`[::1]`). Returns `None` for a hostname.
fn host_ip_literal(host: &str) -> Option<std::net::IpAddr> {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
        .parse()
        .ok()
}

/// Redirect policy for the desktop client. Redirects are followed (as the browser would), but
/// a hop onto a non-public IP *literal* is refused: reqwest dials an IP literal with no DNS
/// lookup, so [`PublicOnlyResolver`] — which vets every hostname hop at connect — never sees
/// it. This is the redirect-time twin of [`reject_non_public_proxy`]. `allow_private`
/// (preview) permits the local network but never link-local / metadata.
fn public_only_redirect(allow_private: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        // Policy::custom replaces reqwest's built-in hop cap, so re-impose the default of 10.
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects".to_string());
        }
        if let Some(ip) = attempt.url().host_str().and_then(host_ip_literal) {
            let permitted = allow_private && common::util::is_private_lan(&ip);
            if !permitted && common::util::is_forbidden_ip(&ip) {
                return attempt.error(format!(
                    "redirect to non-public address {ip} is not allowed"
                ));
            }
        }
        attempt.follow()
    })
}

/// Vet a scene-supplied proxy endpoint. Only an IP *literal* needs checking here: reqwest's
/// proxy connector is an `HttpConnector<DynResolver>`, which dials an IP literal directly but
/// routes a *hostname* endpoint through [`PublicOnlyResolver`] at connect — so a hostname
/// proxy is already egress-checked there and a literal is the one case that skips it. (A
/// `socks*://` proxy needs no handling: the `socks` feature is off, so `reqwest::Proxy::http`
/// rejects it before we get here.) `allow_private` (preview) permits the local network but
/// never link-local / metadata.
fn reject_non_public_proxy(proxy_url: &str, allow_private: bool) -> Result<(), AnyError> {
    let url = deno_core::url::Url::parse(proxy_url)?;
    if let Some(ip) = url.host_str().and_then(host_ip_literal) {
        let permitted = allow_private && common::util::is_private_lan(&ip);
        if !permitted && common::util::is_forbidden_ip(&ip) {
            anyhow::bail!("custom fetch client proxy may not target a non-public address");
        }
    }
    Ok(())
}

#[op2]
#[serde]
pub fn op_fetch_custom_client(
    state: &mut OpState,
    #[serde] args: CreateHttpClientOptions,
) -> Result<ResourceId, AnyError> {
    debug!("op_fetch_custom_client");

    // A custom client is scene-supplied transport configuration, and on the shared
    // authoritative server none of it may be honoured:
    //
    // * `proxy` re-targets the connection at an address the SSRF guard never sees. The
    //   guard inspects the request URL; the proxy is what actually gets dialled. Pointing
    //   it at 169.254.169.254 reaches cloud metadata with a perfectly public-looking URL,
    //   and disabling redirects does nothing about it.
    // * `ca_certs` makes the scene a trust root for the server's outbound TLS.
    // * `cert_chain`/`private_key` let a scene present a client identity as the server.
    //
    // Refused rather than ignored so a scene that tries gets an error it can see.
    // Everything else (the plain `createHttpClient()` case) still works.
    let (is_server, preview) = {
        let ctx = state.borrow::<CrdtContext>();
        (ctx.is_server, ctx.preview)
    };
    if is_server {
        if args.proxy.is_some() {
            anyhow::bail!("custom fetch clients may not set a proxy on the authoritative server");
        }
        if !args.ca_certs.is_empty() {
            anyhow::bail!(
                "custom fetch clients may not add root certificates on the authoritative server"
            );
        }
        if args.cert_chain.is_some() || args.private_key.is_some() {
            anyhow::bail!(
                "custom fetch clients may not set a client identity on the authoritative server"
            );
        }
        // same transport rules as the default server client: no redirects, public-only DNS
        return Ok(state
            .resource_table
            .add(ClientResource(build_scene_client(preview, true))));
    }

    // Client mode: the scene may still tune TLS trust/identity for its own machine, but its
    // connections stay public-only via the resolver (unless preview), and redirects are held
    // to the same egress policy as the default client (see `public_only_redirect`). A proxy
    // endpoint given as an IP literal skips that resolver, so it is vetted synchronously by
    // `reject_non_public_proxy` (a hostname endpoint is resolved through the resolver).
    let mut builder = scene_client_builder(preview).redirect(public_only_redirect(preview));
    if let Some(proxy_def) = args.proxy {
        reject_non_public_proxy(&proxy_def.url, preview)?;
        let mut proxy = reqwest::Proxy::http(proxy_def.url)?;
        if let Some(creds) = proxy_def.basic_auth {
            proxy = proxy.basic_auth(&creds.username, &creds.password);
        }
        builder = builder.proxy(proxy);
    }
    if !args.ca_certs.is_empty() {
        for ca_cert in &args.ca_certs {
            builder =
                builder.add_root_certificate(reqwest::Certificate::from_pem(ca_cert.as_bytes())?);
        }
    }
    if let (Some(chain), Some(key)) = (args.cert_chain, args.private_key) {
        builder = builder.identity(reqwest::Identity::from_pkcs12_der(chain.as_bytes(), &key)?);
    }

    Ok(state.resource_table.add(ClientResource(builder.build()?)))
}

#[op2(async)]
#[serde]
pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<OpState>>,
    #[string] uri: String,
    #[string] method: Option<String>,
) -> Result<Vec<(String, String)>, AnyError> {
    dcl::js::fetch::op_signed_fetch_headers(state, uri, method).await
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
