use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex, MutexGuard, PoisonError,
};
use std::time::Duration;

use crate::IpfsIo;

use super::{test::build_pack, PackData, PackState, ScenePack};

static IDLE_ENV: Mutex<()> = Mutex::new(());

pub(crate) struct IdleEnv(#[allow(dead_code)] MutexGuard<'static, ()>);

pub(crate) fn idle_env(value: &str) -> IdleEnv {
    let guard = IDLE_ENV.lock().unwrap_or_else(PoisonError::into_inner);
    std::env::set_var("SCENE_PACK_IDLE_ABORT_SECS", value);
    IdleEnv(guard)
}

impl Drop for IdleEnv {
    fn drop(&mut self) {
        std::env::remove_var("SCENE_PACK_IDLE_ABORT_SECS");
    }
}

pub(crate) struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(false),
            cv: Condvar::new(),
        })
    }

    pub(crate) fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.cv.notify_all();
    }

    pub(crate) fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.cv.wait(open).unwrap();
        }
    }
}

pub(crate) struct Script {
    pub(crate) chunk: usize,
    pub(crate) hold_at: Option<(usize, Arc<Gate>)>,
    pub(crate) truncate_at: Option<usize>,
    pub(crate) body: Option<Vec<u8>>,
}

pub(crate) fn full(chunk: usize) -> Script {
    Script {
        chunk,
        hold_at: None,
        truncate_at: None,
        body: None,
    }
}

pub(crate) fn serve_scripted(
    path: String,
    body: Vec<u8>,
    scripts: Vec<Script>,
) -> (String, Arc<AtomicBool>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let broken = Arc::new(AtomicBool::new(false));
    let broken_flag = broken.clone();
    std::thread::spawn(move || {
        let mut scripts = scripts.into_iter();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let Some(script) = scripts.next() else { break };
            let body = script.body.as_ref().unwrap_or(&body);
            let _ = stream.set_nodelay(true);
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            if request.split_whitespace().nth(1) != Some(path.as_str()) {
                continue;
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(header.as_bytes()).is_err() {
                broken_flag.store(true, Ordering::SeqCst);
                continue;
            }
            let end = script.truncate_at.unwrap_or(body.len()).min(body.len());
            let mut sent = 0usize;
            let mut held = false;
            while sent < end {
                let mut next = (sent + script.chunk).min(end);
                if let Some((hold, gate)) = &script.hold_at {
                    if !held && sent < *hold {
                        next = next.min(*hold);
                    }
                    if !held && sent == *hold {
                        gate.wait();
                        held = true;
                    }
                }
                if stream.write_all(&body[sent..next]).is_err() {
                    broken_flag.store(true, Ordering::SeqCst);
                    break;
                }
                sent = next;
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    });
    (base, broken)
}

pub(crate) fn stream_fixture() -> (String, Vec<u8>) {
    let glb: Vec<u8> = (0..100u8).collect();
    let mp3: Vec<u8> = (0..4096usize).map(|i| (i % 251) as u8).collect();
    let files: &[(&str, &str, &[u8])] = &[
        ("scene.json", "bafyjson", b"{}"),
        ("models/big.glb", "bafyglb", &glb),
        ("sounds/tail.mp3", "bafymp3", &mp3),
    ];
    (
        "stream-entity".to_owned(),
        build_pack("stream-entity", files),
    )
}

pub(crate) fn pack_route(entity: &str) -> String {
    format!("/bvwebgpu/{}/{entity}.pack", super::PACK_PROFILE)
}

pub(crate) fn wire_start(bytes: &[u8], entity: &str, cid: &str) -> usize {
    let pack = ScenePack::parse("u".into(), entity, bytes).unwrap();
    pack.entry_range(pack.entry_by_cid(cid).unwrap()).0
}

pub(crate) async fn spawn_fetch(
    io: &Arc<IpfsIo>,
    base: &str,
    entity: &str,
) -> async_std::task::JoinHandle<()> {
    let (sender, receiver) = tokio::sync::watch::channel(());
    io.scene_packs
        .packs
        .write()
        .await
        .insert(entity.to_owned(), PackState::Fetching(receiver));
    let io = io.clone();
    let base = base.to_owned();
    let entity = entity.to_owned();
    async_std::task::spawn(platform::compat(async move {
        io.run_pack_stream(&base, &entity, sender).await;
    }))
}

pub(crate) async fn ready_pack(io: &IpfsIo, entity: &str) -> Arc<ScenePack> {
    for _ in 0..500 {
        if let Some(PackState::Ready(pack)) = io.scene_packs.packs.read().await.get(entity) {
            return pack.clone();
        }
        async_std::task::sleep(Duration::from_millis(10)).await;
    }
    panic!("pack never became ready");
}

pub(crate) async fn is_streaming(pack: &ScenePack) -> bool {
    matches!(&*pack.data.read().await, PackData::Streaming(_))
}
