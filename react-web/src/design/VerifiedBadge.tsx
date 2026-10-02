// The verified check shown next to a claimed name.

import { useId } from 'react'

export function VerifiedBadge({ className }: { className?: string }): React.JSX.Element {
  // one gradient per badge: a shared id would point every badge at the first one in the page
  const gradient = useId()
  return (
    <svg className={className} viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--brand)" />
          <stop offset="1" stopColor="var(--verified-end)" />
        </linearGradient>
      </defs>
      <path
        d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z"
        fill={`url(#${gradient})`}
      />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="var(--white)" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}
