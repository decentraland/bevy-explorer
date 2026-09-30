// Pre-boot GPU probe. The engine (bevy / wgpu) needs a usable WebGPU adapter. A Chromium browser can
// expose `navigator.gpu` yet still have NO usable adapter — Linux without GPU drivers, hardware
// acceleration disabled, a blocklisted GPU, a VM / remote desktop, or headless. Booting there
// downloads the ~105 MB WASM and then panics ("Unable to find a GPU!"), so we probe first and show a
// friendly gate (see MobileGate `reason="gpu"`) instead of a scary post-download crash.

const PROBE_TIMEOUT_MS = 5000

// Minimal shape — we don't depend on @webgpu/types just for this one call.
type GpuLike = { requestAdapter(): Promise<unknown> }

/**
 * True when the browser has a usable WebGPU adapter (safe to boot the engine). False when WebGPU is
 * absent or `requestAdapter()` resolves null (no usable GPU). Fails OPEN on a hung probe: a timeout
 * returns true so the check never traps the user — worst case we boot and hit today's panic path,
 * never a frozen "checking" screen.
 */
export async function hasUsableGpu(): Promise<boolean> {
  const gpu = (navigator as unknown as { gpu?: GpuLike }).gpu
  if (gpu == null) return false
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    const adapter = await Promise.race([
      gpu.requestAdapter(),
      new Promise<'timeout'>((resolve) => {
        timer = setTimeout(() => resolve('timeout'), PROBE_TIMEOUT_MS)
      })
    ])
    if (adapter === 'timeout') return true
    return adapter != null
  } catch {
    return false
  } finally {
    if (timer != null) clearTimeout(timer)
  }
}
