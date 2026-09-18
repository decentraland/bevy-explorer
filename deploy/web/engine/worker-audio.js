export function createWorkerAudioHost() {
  const context = new AudioContext();
  const buffers = new Map();
  const instances = new Map();
  const resume = () => {
    if (context.state === 'suspended') context.resume().catch(console.error);
  };
  window.addEventListener('pointerdown', resume);
  window.addEventListener('keydown', resume);
  const stop = id => {
    const entry = instances.get(id);
    if (!entry) return;
    instances.delete(id);
    entry.gain.gain.linearRampToValueAtTime(0, context.currentTime + 0.01);
    entry.source.stop(context.currentTime + 0.01);
  };
  return {
    pcm(id, samples, sampleRate) {
      if (!samples.length) return;
      const buffer = context.createBuffer(1, samples.length, sampleRate);
      buffer.copyToChannel(samples, 0);
      buffers.set(id, buffer);
    },
    op(json) {
      const op = JSON.parse(json);
      switch (op.op) {
        case 'play': {
          const buffer = buffers.get(op.buf);
          if (!buffer) throw new Error(`Missing worker audio buffer: ${op.buf}`);
          const source = context.createBufferSource();
          source.buffer = buffer;
          const gain = context.createGain();
          gain.gain.value = 0;
          const panner = context.createStereoPanner();
          source.connect(gain).connect(panner).connect(context.destination);
          source.onended = () => {
            if (instances.get(op.inst)?.source === source) instances.delete(op.inst);
            source.disconnect(); gain.disconnect(); panner.disconnect();
          };
          source.start(0, Math.max(0, op.offset));
          instances.set(op.inst, { source, gain, panner });
          break;
        }
        case 'stop': stop(op.inst); break;
        case 'dropbuf': buffers.delete(op.buf); break;
        case 'cfg': {
          const entry = instances.get(op.inst);
          if (!entry) break;
          entry.source.playbackRate.value = op.rate;
          entry.source.loop = op.loop;
          break;
        }
        default: throw new Error(`Unknown worker audio operation: ${op.op}`);
      }
    },
    params(data) {
      for (let i = 0; i < data.length; i += 3) {
        const entry = instances.get(data[i]);
        if (!entry) continue;
        entry.gain.gain.value = data[i + 1];
        entry.panner.pan.value = Math.max(-1, Math.min(1, data[i + 2]));
      }
    },
    close() {
      window.removeEventListener('pointerdown', resume);
      window.removeEventListener('keydown', resume);
      for (const id of instances.keys()) stop(id);
      buffers.clear();
      return context.close();
    },
  };
}
