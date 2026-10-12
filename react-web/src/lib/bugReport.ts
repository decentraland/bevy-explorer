/** A prefilled GitHub issue for a bug report. */
export function bugReportUrl(): string {
  const body = `**What happened**\n\n**Steps to reproduce**\n\n**Environment**\n- Browser: ${navigator.userAgent}\n`
  return `https://github.com/decentraland/bevy-explorer/issues/new?body=${encodeURIComponent(body)}`
}
