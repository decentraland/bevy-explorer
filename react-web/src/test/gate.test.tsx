import { describe, it, expect, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MobileGate } from '../features/gate/MobileGate'
import { isChromiumBased } from '../lib/isMobile'

const IPHONE_UA =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1'
const ANDROID_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36'
const FIREFOX_UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0'
const SAFARI_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15'
const CHROME_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'

const origUA = Object.getOwnPropertyDescriptor(window.navigator, 'userAgent')
function setUA(ua: string): void {
  Object.defineProperty(window.navigator, 'userAgent', { value: ua, configurable: true })
}
afterEach(() => {
  if (origUA) Object.defineProperty(window.navigator, 'userAgent', origUA)
})

describe('mobile gate', () => {
  // jsdom's UA isn't iOS/Android → mobilePlatform() === 'other' → both store links render.
  it('shows both store download links with the right URLs', () => {
    render(<MobileGate />)
    const apple = screen.getByRole('link', { name: /App Store/i })
    const google = screen.getByRole('link', { name: /Google Play/i })
    expect(apple).toHaveAttribute('href', 'https://testflight.apple.com/join/KF4r3jlU')
    expect(google).toHaveAttribute(
      'href',
      'https://play.google.com/store/apps/details?id=org.decentraland.godotexplorer'
    )
  })

  it('on iPhone shows only the App Store link', () => {
    setUA(IPHONE_UA)
    render(<MobileGate />)
    expect(screen.getByRole('link', { name: /App Store/i })).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: /Google Play/i })).toBeNull()
  })

  it('on Android shows only the Google Play link', () => {
    setUA(ANDROID_UA)
    render(<MobileGate />)
    expect(screen.getByRole('link', { name: /Google Play/i })).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: /App Store/i })).toBeNull()
  })
})

describe('browser gate (non-Chromium desktop)', () => {
  it('isChromiumBased: true for Chrome, false for Firefox / Safari', () => {
    setUA(CHROME_UA)
    expect(isChromiumBased()).toBe(true)
    setUA(FIREFOX_UA)
    expect(isChromiumBased()).toBe(false)
    setUA(SAFARI_UA)
    expect(isChromiumBased()).toBe(false)
  })

  it('the browser variant shows a Download Chrome link + try-anyway, no store links', () => {
    render(<MobileGate reason="browser" />)
    expect(screen.getByRole('heading', { name: /Browser Not Supported/i })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Download Chrome/i })).toHaveAttribute('href', 'https://www.google.com/chrome/')
    expect(screen.getByRole('button', { name: /try anyway/i })).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: /App Store|Google Play/i })).toBeNull()
  })
})
