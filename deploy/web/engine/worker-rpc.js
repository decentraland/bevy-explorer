// Messages stay on the dedicated engine worker; scene workers never receive this endpoint.
export function createWorkerRpc(port, methods, timeoutMs = 60000) {
  let nextId = 0;
  let closed = false;
  const pending = new Map();
  const onMessage = async ({ data }) => {
    if (data?.type !== 'engine-rpc') return;
    const { id, method, args, error, value } = data;
    if (method === undefined) {
      const request = pending.get(id);
      if (!request) return;
      pending.delete(id);
      clearTimeout(request.timer);
      if (error !== undefined) request.reject(new Error(error));
      else request.resolve(value);
      return;
    }
    try {
      if (!Object.hasOwn(methods, method)) throw new Error(`Unknown engine method: ${method}`);
      const result = await methods[method](...args);
      if (!closed && id !== undefined) port.postMessage({ type: 'engine-rpc', id, value: result });
    } catch (error) {
      if (!closed && id !== undefined) port.postMessage({ type: 'engine-rpc', id, error: String(error) });
      else console.error(`Engine notification ${method} failed`, error);
    }
  };
  port.addEventListener('message', onMessage);
  return {
    call(method, args = [], transfer = []) {
      if (closed) return Promise.reject(new Error('Engine worker is closed'));
      const id = ++nextId;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`Engine worker timed out: ${method}`));
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer });
        try { port.postMessage({ type: 'engine-rpc', id, method, args }, transfer); }
        catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
      });
    },
    notify(method, ...args) {
      if (!closed) port.postMessage({ type: 'engine-rpc', method, args });
    },
    close(error = new Error('Engine worker is closed')) {
      closed = true;
      port.removeEventListener('message', onMessage);
      for (const request of pending.values()) {
        clearTimeout(request.timer);
        request.reject(error);
      }
      pending.clear();
    },
  };
}
