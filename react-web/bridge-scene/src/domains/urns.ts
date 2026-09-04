// Item urn helpers shared by the wearables / emotes / marketplace domains.
//   urn:decentraland:off-chain:base-avatars:<name>                  base wearable
//   urn:decentraland:off-chain:base-emotes:<name>                   base emote
//   urn:decentraland:ethereum:collections-v1:<collection>:<slug>    legacy collectible (item form)
//   urn:decentraland:matic:collections-v2:<contract>:<itemId>       collectible (item form)
//   …:<tokenId>                                                     owned / deployed token form

// A deployed/owned urn may carry a tokenId (…:collections-v{1,2}:<contract>:<itemId>:<tokenId>);
// the item form drops it. Both v1 (ethereum) and v2 (matic) items are 6 segments, so a trailing
// token makes it >6 — strip it for either, else a by-urn resolve (which needs the bare item urn)
// misses the item and its category slot renders empty. Base urns pass through.
export function itemUrn(urn: string): string {
  const parts = urn.split(':')
  if ((parts[3] === 'collections-v2' || parts[3] === 'collections-v1') && parts.length > 6) {
    return parts.slice(0, 6).join(':')
  }
  return urn
}

// Collection items must be referenced in the DEPLOYED profile by their full token URN
// (…:{contract}:{itemId}:{tokenId}); the catalyst rejects the bare item URN with
// "should be an item, not an asset. The URN must include the tokenId.". The owned catalog's
// individualData carries the owned tokenId per item, so domains map item-urn → token-urn from it
// and send the token form on equip. Base (off-chain) items have no tokenId and pass through unchanged.
export function tokenUrnOf(el: { urn: string; individualData?: Array<{ id?: string; tokenId?: string }> }): string {
  const d = el.individualData?.[0]
  if (d?.id != null && d.id.startsWith(`${el.urn}:`)) return d.id
  if (d?.tokenId != null && d.tokenId !== '') return `${el.urn}:${d.tokenId}`
  return el.urn
}
