// <img> for a catalyst thumbnail. Resolves its src from a URN via the direct catalyst URL
// (loads under the engine's COEP `credentialless` context — no proxy/blob needed). An
// explicit `src` (e.g. a thumbnail the relay already provided) takes precedence.

import { catalystThumbUrl } from '../lib/identity'

export function CatalystImg({
  urn,
  src,
  alt = '',
  className
}: {
  urn?: string
  src?: string
  alt?: string
  className?: string
}): React.JSX.Element {
  return <img src={src ?? (urn ? catalystThumbUrl(urn) : undefined)} alt={alt} className={className} />
}
