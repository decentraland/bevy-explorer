function installLivekitWorker(scope, globals) {
  let nextId = 0;
  const pending = /* @__PURE__ */ new Map();
  const handlers = /* @__PURE__ */ new Map();
  const objects = /* @__PURE__ */ new Map();
  const decode = (value) => {
    if (Array.isArray(value)) return value.map(decode);
    if (!value || typeof value !== "object" || ArrayBuffer.isView(value)) return value;
    const record = value;
    const id = record.__livekitHandle;
    const out = typeof id === "number" ? objects.get(id) ?? {} : {};
    if (typeof id === "number") objects.set(id, out);
    for (const [key, item] of Object.entries(record)) out[key] = decode(item);
    return out;
  };
  globals.__dclLivekitRpc = (method, args) => {
    const id = ++nextId;
    const encoded = args.map((arg) => {
      if (typeof arg === "function") {
        handlers.set(id, arg);
        return { __livekitCallback: id };
      }
      if (arg && typeof arg === "object" && "__livekitHandle" in arg)
        return { __livekitHandle: arg.__livekitHandle };
      return arg;
    });
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        handlers.delete(id);
        reject(new Error(`Voice request timed out: ${method}`));
      }, 3e4);
      pending.set(id, { resolve, reject, timer });
      scope.postMessage({ type: "LIVEKIT_CALL", id, method, args: encoded });
    });
  };
  scope.addEventListener("message", ({ data }) => {
    if (data?.type === "LIVEKIT_RELEASE") {
      for (const id of data.ids) objects.delete(id);
    } else if (data?.type === "LIVEKIT_MIC") {
      globals.__dclMicrophoneAvailable = data.available;
      globals.__dclMicrophonePermission = data.permission;
    } else if (data?.type === "LIVEKIT_EVENT") {
      handlers.get(data.id)?.(decode(data.value));
      if (data.value?.type === "disconnected") handlers.delete(data.id);
    } else if (data?.type === "LIVEKIT_RESULT") {
      const request = pending.get(data.id);
      if (!request) return;
      pending.delete(data.id);
      clearTimeout(request.timer);
      if (data.ok) request.resolve(decode(data.value));
      else {
        handlers.delete(data.id);
        request.reject(new Error(data.error));
      }
    }
  });
}
export {
  installLivekitWorker
};
