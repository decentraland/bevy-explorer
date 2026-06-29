import { describe, it, expect, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MobileGate } from '../features/gate/MobileGate'

const IPHONE_UA =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1'
const ANDROID_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36'

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
