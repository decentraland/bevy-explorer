// Decentraland logo mark (official sunset disc). useId keeps the gradient/clip IDs
// unique so multiple instances on a page don't cross-reference each other's defs.

import { useId } from 'react'

export function DclLogo({
  size = 24,
  className
}: {
  size?: number
  className?: string
}): React.JSX.Element {
  const uid = useId().replace(/:/g, '')
  const bg = `dcl-bg-${uid}`
  const p1 = `dcl-p1-${uid}`
  const p2 = `dcl-p2-${uid}`
  const clip = `dcl-clip-${uid}`
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <g clipPath={`url(#${clip})`}>
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M12 24C18.6274 24 24 18.6274 24 12C24 5.37258 18.6274 0 12 0C5.37258 0 0 5.37258 0 12C0 18.6274 5.37258 24 12 24Z"
          fill={`url(#${bg})`}
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M4.80002 21.5999C6.80402 23.1059 9.30002 23.9999 12 23.9999C14.7 23.9999 17.196 23.1059 19.2 21.5999H4.80002Z"
          fill="#FF2D55"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M2.40004 19.1999C3.08404 20.1059 3.89404 20.9159 4.80004 21.5999H19.2C20.106 20.9159 20.916 20.1059 21.6 19.1999H2.40004Z"
          fill="#FFA25A"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M16.098 16.7999H1.00201C1.37401 17.6579 1.84801 18.4619 2.40001 19.1999H16.104V16.7999H16.098Z"
          fill="#FFC95B"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M8.50203 7.79987V16.7999H16.002L8.50203 7.79987Z"
          fill={`url(#${p1})`}
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M1.00201 16.7999H8.50201V7.7999L1.00201 16.7999Z"
          fill="#FCFCFC"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M16.098 12.6V19.2H21.6L16.098 12.6Z"
          fill={`url(#${p2})`}
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M10.602 19.2H16.098V12.6L10.602 19.2Z"
          fill="#FCFCFC"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M16.098 10.7999C17.7549 10.7999 19.098 9.45675 19.098 7.79989C19.098 6.14304 17.7549 4.7999 16.098 4.7999C14.4412 4.7999 13.098 6.14304 13.098 7.79989C13.098 9.45675 14.4412 10.7999 16.098 10.7999Z"
          fill="#FFC95B"
        />
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M8.50198 5.99991C9.33041 5.99991 10.002 5.32833 10.002 4.49991C10.002 3.67148 9.33041 2.99991 8.50198 2.99991C7.67356 2.99991 7.00198 3.67148 7.00198 4.49991C7.00198 5.32833 7.67356 5.99991 8.50198 5.99991Z"
          fill="#FFC95B"
        />
      </g>
      <defs>
        <linearGradient id={bg} x1="12" y1="-4.97056" x2="-4.97055" y2="12" gradientUnits="userSpaceOnUse">
          <stop stopColor="#FF2D55" />
          <stop offset="1" stopColor="#FFBC5B" />
        </linearGradient>
        <linearGradient id={p1} x1="8.49951" y1="7.79987" x2="8.49951" y2="16.7999" gradientUnits="userSpaceOnUse">
          <stop stopColor="#A524B3" />
          <stop offset="1" stopColor="#FF2D55" />
        </linearGradient>
        <linearGradient id={p2} x1="16.0961" y1="12.6" x2="16.0961" y2="19.2" gradientUnits="userSpaceOnUse">
          <stop stopColor="#A524B3" />
          <stop offset="1" stopColor="#FF2D55" />
        </linearGradient>
        <clipPath id={clip}>
          <rect width="24" height="24" rx="12" fill="white" />
        </clipPath>
      </defs>
    </svg>
  )
}
