import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MessageChannel } from 'node:worker_threads';
import { installLivekitHost } from './worker-livekit-host.js';
import { installLivekitWorker } from './worker-livekit.js';
import { local_audio_track_new } from '../../../crates/comms/livekit_web_bindings.js';

// Class instances cannot cross the worker boundary with their methods intact.
class Participant { identity = 'guest'; sid = 'p1'; isLocal = true; }
class Room { name = 'test-room'; localParticipant = new Participant(); }

test('voice objects and callbacks round-trip; disconnect releases the handles', async t => {
  const { port1, port2 } = new MessageChannel();
  const globals = {};
  let callback;
  let sent;
  const room = new Room();
  const close = installLivekitHost(port1, {
    room_connect: async (_url, _token, _options, _connectOptions, handler) => {
      callback = handler;
      return room;
    },
    room_close: async value => { assert.equal(value, room); callback({type:'disconnected'}); },
    local_participant_publish_data: async (participant, bytes) => {
      assert.equal(participant, room.localParticipant);
      sent = [...bytes];
    },
  });
  installLivekitWorker(port2, globals);
  t.after(() => { close(); port1.close(); port2.close(); });
  const events = [];
  const proxy = await globals.__dclLivekitRpc('room_connect', ['url','token',{}, {}, e => events.push(e)]);
  assert.equal(proxy.name, 'test-room');
  assert.equal(proxy.localParticipant.identity, 'guest');
  await globals.__dclLivekitRpc('local_participant_publish_data', [proxy.localParticipant, new Uint8Array([1,2,3])]);
  assert.deepEqual(sent, [1,2,3]);
  await globals.__dclLivekitRpc('room_close', [proxy]);
  assert.equal(events[0].type, 'disconnected');
  await assert.rejects(globals.__dclLivekitRpc('room_close', [proxy]), /already released/);
});

test('microphone options are copied from wasm-bindgen getters, not its pointer', async t => {
  const previous = globalThis.__dclLivekitRpc;
  t.after(() => { globalThis.__dclLivekitRpc = previous; });
  globalThis.__dclLivekitRpc = async (method, [options]) => {
    assert.equal(method, 'local_audio_track_new');
    assert.equal(options.channelCount, 2);
    assert.equal(options.echoCancellation, true);
    assert.equal('__wbg_ptr' in options, false);
    return 'track';
  };
  const options = { __wbg_ptr: 123 };
  Object.defineProperties(options, {
    channelCount: {get: () => 2n}, echoCancellation: {get: () => true},
  });
  assert.equal(await local_audio_track_new(options), 'track');
});
