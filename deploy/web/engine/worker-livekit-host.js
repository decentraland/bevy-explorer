const METHODS = /* @__PURE__ */ new Set([
  "room_connect",
  "room_close",
  "local_participant_publish_data",
  "local_participant_publish_track",
  "local_participant_unpublish_track",
  "remote_track_publication_set_subscribed",
  "local_audio_track_new",
  "remote_track_pan_and_volume",
  "remote_audio_track_set_volume",
  "promptMicrophonePermission"
]);
const FIELDS = ["name", "sid", "identity", "metadata", "isLocal", "trackSid", "kind", "source", "track", "localParticipant"];
function installLivekitHost(worker, api) {
  const handles = /* @__PURE__ */ new Map();
  const ids = /* @__PURE__ */ new WeakMap();
  const ownership = /* @__PURE__ */ new Map();
  let nextId = 0;
  let closed = false;
  const release = (owner) => {
    const released = [...ownership.get(owner) ?? []];
    for (const id of released) handles.delete(id);
    ownership.delete(owner);
    if (!closed) worker.postMessage({ type: "LIVEKIT_RELEASE", ids: released });
  };
  const encode = (value, owner) => {
    if (!value || typeof value !== "object" || ArrayBuffer.isView(value)) return value;
    if (Array.isArray(value)) return value.map((item) => encode(item, owner));
    const object = value;
    if (Object.getPrototypeOf(value) === Object.prototype)
      return Object.fromEntries(Object.entries(object).map(([key, item]) => [key, encode(item, owner)]));
    let id = ids.get(value);
    if (id === void 0) {
      id = ++nextId;
      ids.set(value, id);
    }
    handles.set(id, object);
    if (owner !== void 0) {
      const owned = ownership.get(owner) ?? /* @__PURE__ */ new Set();
      owned.add(id);
      ownership.set(owner, owned);
    }
    const snapshot = { __livekitHandle: id };
    for (const key of FIELDS) if (object[key] !== void 0) snapshot[key] = encode(object[key], owner);
    return snapshot;
  };
  const decode = (value) => {
    if (!value || typeof value !== "object") return value;
    const ref = value;
    if (typeof ref.__livekitHandle === "number") {
      const object = handles.get(ref.__livekitHandle);
      if (!object) throw new Error("Voice object was already released");
      return object;
    }
    if (typeof ref.__livekitCallback === "number") {
      const owner = ref.__livekitCallback;
      return (event) => {
        if (closed) return;
        worker.postMessage({ type: "LIVEKIT_EVENT", id: owner, value: encode(event, owner) });
        if (event?.type === "disconnected") release(owner);
      };
    }
    return value;
  };
  const onMessage = ({ data }) => {
    if (data?.type !== "LIVEKIT_CALL" || closed) return;
    void (async () => {
      try {
        if (!METHODS.has(data.method) || !Array.isArray(data.args)) throw new Error("Unknown voice operation");
        const fn = api[data.method];
        if (!fn) throw new Error("Voice operation unavailable");
        const args = data.args.map(decode);
        const result = await fn(...args);
        if (!closed) worker.postMessage({ type: "LIVEKIT_RESULT", id: data.id, ok: true, value: encode(result, data.method === "room_connect" ? data.id : void 0) });
        else if (result && typeof result === "object") {
          const object = result;
          await object.disconnect?.();
          object.stop?.();
        }
        if (data.method === "local_participant_unpublish_track") {
          const id = data.args[1]?.__livekitHandle;
          if (typeof id === "number") {
            handles.delete(id);
            if (!closed) worker.postMessage({ type: "LIVEKIT_RELEASE", ids: [id] });
          }
        }
      } catch (error) {
        if (data.method === "room_connect") release(data.id);
        if (!closed) worker.postMessage({ type: "LIVEKIT_RESULT", id: data.id, ok: false, error: String(error) });
      }
    })();
  };
  worker.addEventListener("message", onMessage);
  api.setupMicrophonePermission?.();
  let previous = "";
  const publishMic = () => {
    const available = api.is_microphone_available?.();
    const permission = api.microphonePermissionState?.();
    const key = `${available}:${permission}`;
    if (key === previous) return;
    previous = key;
    worker.postMessage({ type: "LIVEKIT_MIC", available, permission });
  };
  publishMic();
  const timer = setInterval(publishMic, 250);
  return () => {
    closed = true;
    clearInterval(timer);
    worker.removeEventListener("message", onMessage);
    for (const object of handles.values()) {
      if (typeof object.disconnect === "function") void Promise.resolve(object.disconnect()).catch(console.warn);
      if (typeof object.stop === "function") object.stop();
    }
    handles.clear();
    ownership.clear();
  };
}
export {
  installLivekitHost
};
