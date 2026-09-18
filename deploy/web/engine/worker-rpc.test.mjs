import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MessageChannel } from 'node:worker_threads';
import { createWorkerRpc } from './worker-rpc.js';

function pair(t, methods, timeout = 1000) {
  const { port1, port2 } = new MessageChannel();
  const client = createWorkerRpc(port1, {}, timeout);
  const server = createWorkerRpc(port2, methods, timeout);
  t.after(() => { client.close(); server.close(); port1.close(); port2.close(); });
  return client;
}

test('round-trips asynchronous results and transferred buffers', async t => {
  const rpc = pair(t, { length: async bytes => bytes.byteLength });
  const bytes = new Uint8Array(12);
  const result = rpc.call('length', [bytes], [bytes.buffer]);
  assert.equal(bytes.byteLength, 0);
  assert.equal(await result, 12);
});

test('rejects unavailable methods and remote errors', async t => {
  const rpc = pair(t, { fail() { throw new Error('startup failed'); } });
  await assert.rejects(rpc.call('constructor'), /Unknown engine method/);
  await assert.rejects(rpc.call('fail'), /startup failed/);
});

test('rejects requests when the worker does not answer', async t => {
  const rpc = pair(t, { stall: () => new Promise(() => {}) }, 20);
  await assert.rejects(rpc.call('stall'), /timed out: stall/);
});

test('crash cleanup rejects outstanding and later calls', async t => {
  const rpc = pair(t, { stall: () => new Promise(() => {}) });
  const pending = rpc.call('stall');
  rpc.close(new Error('worker crashed'));
  await assert.rejects(pending, /worker crashed/);
  await assert.rejects(rpc.call('stall'), /closed/);
});

test('an uncloneable argument does not poison later calls', async t => {
  const rpc = pair(t, { echo: value => value });
  await assert.rejects(rpc.call('echo', [() => {}]), /clone/);
  assert.equal(await rpc.call('echo', ['working']), 'working');
});
