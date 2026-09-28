// Relays an engine stream for the life of the scene. One bad item is logged and skipped, and a
// stream that ends or fails is reopened after a backoff, so a domain never goes silent for good.
// No SDK imports: the delay is passed in (the scene counts frames), which keeps this testable.

const BACKOFF_MS = [1000, 2000, 5000, 10000, 30000]

export function relayStream<T>(
  name: string,
  open: () => Promise<AsyncIterable<T>>,
  onItem: (item: T) => void | Promise<void>,
  delay: (ms: number) => Promise<void>,
  shouldStop: () => boolean = () => false
): void {
  void (async () => {
    let attempt = 0
    while (!shouldStop()) {
      try {
        const stream = await open()
        for await (const item of stream) {
          attempt = 0
          try {
            await onItem(item)
          } catch (e) {
            console.error(`[${name}] item failed`, e)
          }
        }
        console.error(`[${name}] stream ended, reopening`)
      } catch (e) {
        console.error(`[${name}] stream failed, reopening`, e)
      }
      await delay(BACKOFF_MS[Math.min(attempt, BACKOFF_MS.length - 1)])
      attempt++
    }
  })()
}
