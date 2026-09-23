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

    // deliver engine -> scene messages the way the scene host does
    fn route_to_scene(&mut self) {
        while let Ok((id, msg)) = self.router_rx.try_recv() {
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

    drop(receiver);
    bridge.settle();
    assert!(bridge.close_rx.try_recv().is_err());
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
}

#[test]
fn dropping_the_receiver_early_still_notifies_the_engine() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, receiver) = RpcStreamSender::<u32>::channel();
    let bytes = rmp_encode(&sender).unwrap();
    let _remote: RpcStreamSender<u32> = rmp_serde::from_slice(&bytes).unwrap();

    drop(receiver);
    bridge.settle();

    let id = bridge.close_rx.try_recv().unwrap();
    SCENE_IPC_CONTEXT.with_borrow(|ctx| assert!(ctx.as_ref().unwrap().registry.contains_key(&id)));
    assert_eq!(bridge.close_watchers(), 0);
}

#[test]
fn a_sender_serialized_twice_is_released_only_after_both_remotes_drop() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, mut receiver) = RpcStreamSender::<u32>::channel();
    let first_bytes = rmp_encode(&sender).unwrap();
    let second_bytes = rmp_encode(&sender).unwrap();
    assert_eq!(first_bytes, second_bytes);
    let first: RpcStreamSender<u32> = rmp_serde::from_slice(&first_bytes).unwrap();
    let second: RpcStreamSender<u32> = rmp_serde::from_slice(&second_bytes).unwrap();
    assert_eq!(bridge.scene_endpoints(), 1);
    assert_eq!(bridge.engine_tokens(), 1);
    assert_eq!(bridge.close_watchers(), 1);

    drop(first);
    bridge.settle();
    bridge.route_to_scene();
    bridge.settle();

    assert_eq!(bridge.scene_endpoints(), 1);
    assert_eq!(bridge.engine_tokens(), 1);
    assert_eq!(bridge.close_watchers(), 1);

    second.send(5).unwrap();
    bridge.route_to_scene();
    assert_eq!(receiver.try_recv().unwrap(), 5);

    drop(second);
    bridge.settle();
    bridge.route_to_scene();
    bridge.settle();

    assert_eq!(bridge.scene_endpoints(), 0);
    assert_eq!(bridge.engine_tokens(), 0);
    assert_eq!(bridge.close_watchers(), 0);
}

#[test]
fn dropping_the_receiver_early_closes_every_remote_of_a_sender_serialized_twice() {
    let mut bridge = Bridge::new();
    let _guard = bridge.rt.enter();

    let (sender, receiver) = RpcStreamSender::<u32>::channel();
    let first: RpcStreamSender<u32> = rmp_serde::from_slice(&rmp_encode(&sender).unwrap()).unwrap();
    let second: RpcStreamSender<u32> =
        rmp_serde::from_slice(&rmp_encode(&sender).unwrap()).unwrap();

    drop(receiver);
    bridge.settle();

    // deliver the close the way the scene host and the engine do
    let id = bridge.close_rx.try_recv().unwrap();
    SCENE_IPC_CONTEXT.with_borrow_mut(|ctx| ctx.as_mut().unwrap().registry.remove(&id));
    ENGINE_IPC_CONTEXT.with_borrow_mut(|ctx| {
        if let Some(lease) = ctx.as_mut().unwrap().ipc_channel_registry.remove(&id) {
            lease.token.cancel();
        }
    });

    assert!(first.is_closed());
    assert!(second.is_closed());
    assert!(second.send(1).is_err());

    drop(first);
    drop(second);
    bridge.settle();

    assert_eq!(bridge.engine_tokens(), 0);
    assert!(bridge.router_rx.try_recv().is_err());
}
