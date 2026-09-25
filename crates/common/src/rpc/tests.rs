use super::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

// both ends of the ipc bridge on one thread: the scene host serializes senders into
// SCENE_IPC_CONTEXT, the engine deserializes them against ENGINE_IPC_CONTEXT
struct Bridge {
    rt: tokio::runtime::Runtime,
    close_rx: UnboundedReceiver<u64>,
    router_rx: UnboundedReceiver<(u64, IpcMessage)>,
}

impl Bridge {
    fn new() -> Self {
        let (close_sender, close_rx) = unbounded_channel();
        SCENE_IPC_CONTEXT.set(Some(RequestContext {
            registry: Default::default(),
            close_sender,
            next_id: 1,
        }));
        let (ipc_router, router_rx) = unbounded_channel();
        ENGINE_IPC_CONTEXT.set(Some(ResponseContext {
            ipc_channel_registry: Default::default(),
            ipc_router,
        }));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        Self {
            rt,
            close_rx,
            router_rx,
        }
    }

    // let spawned watcher tasks run
    fn settle(&self) {
        self.rt.block_on(async {
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
        });
    }

    // deliver engine -> scene messages the way renderer_ipc_out and the scene host do
    fn route_to_scene(&mut self) {
        while let Ok((id, msg)) = self.router_rx.try_recv() {
            if matches!(msg, IpcMessage::Closed) {
                ENGINE_IPC_CONTEXT
                    .with_borrow_mut(|ctx| ctx.as_mut().unwrap().ipc_channel_registry.remove(&id));
            }
            SCENE_IPC_CONTEXT.with_borrow_mut(|ctx| {
                let ctx = ctx.as_mut().unwrap();
                match msg {
                    IpcMessage::Data(data) => {
                        if let Some(endpoint) = ctx.registry.get_mut(&id) {
                            endpoint.send(data);
                        }
                    }
                    IpcMessage::Closed => {
                        ctx.registry.remove(&id);
                    }
                }
            });
        }
    }

    // deliver scene -> engine closes the way scene_ipc_out and renderer_ipc_in do,
    // returning how many actually crossed the pipe
    fn route_to_engine(&mut self) -> usize {
        let mut sent = 0;
        while let Ok(id) = self.close_rx.try_recv() {
            let was_open = SCENE_IPC_CONTEXT
                .with_borrow_mut(|ctx| ctx.as_mut().unwrap().registry.remove(&id).is_some());
            if was_open {
                sent += 1;
                ENGINE_IPC_CONTEXT.with_borrow_mut(|ctx| {
                    if let Some(token) = ctx.as_mut().unwrap().ipc_channel_registry.remove(&id) {
                        token.cancel();
                    }
                });
            }
        }
        sent
    }

    fn scene_endpoints(&self) -> usize {
        SCENE_IPC_CONTEXT.with_borrow(|ctx| ctx.as_ref().unwrap().registry.len())
    }

    fn engine_tokens(&self) -> usize {
        ENGINE_IPC_CONTEXT.with_borrow(|ctx| ctx.as_ref().unwrap().ipc_channel_registry.len())
    }

    // each close watcher task holds a clone of the context's close sender
    fn close_watchers(&self) -> usize {
        SCENE_IPC_CONTEXT.with_borrow(|ctx| ctx.as_ref().unwrap().close_sender.strong_count() - 1)
    }
}

#[test]
fn result_channel_is_released_on_both_sides_after_a_reply() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, mut receiver) = RpcResultSender::<u32>::channel();
    let bytes = rmp_encode(&sender).unwrap();
    let remote: RpcResultSender<u32> = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(bridge.scene_endpoints(), 1);
    assert_eq!(bridge.engine_tokens(), 1);
    assert_eq!(bridge.close_watchers(), 1);

    remote.send(7);
    drop(remote);
    bridge.settle();
    bridge.route_to_scene();
    bridge.settle();

    assert_eq!(receiver.try_recv().unwrap(), 7);
    assert_eq!(bridge.scene_endpoints(), 0);
    assert_eq!(bridge.engine_tokens(), 0);
    assert_eq!(bridge.close_watchers(), 0);
    // the watcher's close stays local: the endpoint is already gone
    assert_eq!(bridge.route_to_engine(), 0);

    drop(receiver);
    bridge.settle();
    assert_eq!(bridge.route_to_engine(), 0);
}

#[test]
fn stream_channel_is_released_on_both_sides_when_the_engine_closes_it() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, mut receiver) = RpcStreamSender::<u32>::channel();
    let bytes = rmp_encode(&sender).unwrap();
    let remote: RpcStreamSender<u32> = rmp_serde::from_slice(&bytes).unwrap();

    remote.send(1).unwrap();
    remote.send(2).unwrap();
    drop(remote);
    bridge.settle();
    bridge.route_to_scene();
    bridge.settle();

    assert_eq!(receiver.try_recv().unwrap(), 1);
    assert_eq!(receiver.try_recv().unwrap(), 2);
    assert_eq!(bridge.scene_endpoints(), 0);
    assert_eq!(bridge.engine_tokens(), 0);
    assert_eq!(bridge.close_watchers(), 0);
    assert_eq!(bridge.route_to_engine(), 0);
}

#[test]
fn dropping_the_receiver_early_still_notifies_the_engine() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, receiver) = RpcStreamSender::<u32>::channel();
    let bytes = rmp_encode(&sender).unwrap();
    let remote: RpcStreamSender<u32> = rmp_serde::from_slice(&bytes).unwrap();

    drop(receiver);
    bridge.settle();

    assert_eq!(bridge.close_watchers(), 0);
    assert_eq!(bridge.route_to_engine(), 1);
    assert!(remote.is_closed());
    assert_eq!(bridge.scene_endpoints(), 0);
    assert_eq!(bridge.engine_tokens(), 0);

    // the engine's own close afterwards finds nothing left on either side
    drop(remote);
    bridge.settle();
    bridge.route_to_scene();
    bridge.settle();
    assert_eq!(bridge.route_to_engine(), 0);
}
