// Imports of the engine half of the scene relay (src/inner/relay.rs). Only the relay instance in
// a sandbox worker gets real implementations (sandbox_worker.js relayStart swaps them in by
// name); reaching one of these means a relay export ran somewhere else.
const unbound = () => {
  throw new Error("relay import called outside a relay worker");
};
export const relay_copy_to_b = unbound;
export const relay_copy_from_b = unbound;
export const relay_b_alloc = unbound;
export const relay_b_deliver = unbound;
export const relay_b_pump = unbound;
export const relay_b_next_len = unbound;
export const relay_b_next_ptr = unbound;
export const relay_b_pop = unbound;
