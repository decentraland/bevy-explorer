// Generic server-side-paginated owned-items catalog (backpack grid). One `catalogQuery` handler
// dispatches by `catalog` kind to a per-type page fetcher and emits a uniform `catalogPage`.
//   wearables → wearables.ts fetchWearablesPage (catalyst /explorer/:address/wearables, paged)
//   emotes    → TODO: reuse the same pattern (currently served by the getEmotes domain, uncapped).
import { getPlayer } from '@dcl/sdk/players'
import { fetchWearablesPage } from './wearables'
import type { Ctx } from '../bridge'
import type { Wearable } from '../../../src/engine/protocol'

export function registerCatalog(ctx: Ctx): void {
  ctx.on('catalogQuery', async (msg) => {
    const me = getPlayer()
    const empty: { items: Wearable[]; total: number } = { items: [], total: 0 }
    let page = empty
    if (me != null) {
      if (msg.catalog === 'wearables') {
        page = await fetchWearablesPage(me.userId, {
          page: msg.page,
          pageSize: msg.pageSize,
          category: msg.category,
          search: msg.search,
          orderBy: msg.orderBy,
          direction: msg.direction,
          collectiblesOnly: msg.collectiblesOnly
        }).catch(() => empty)
      } else {
        // TODO(emotes): route emotes through the same paged fetcher.
        console.log('[catalog] emotes pagination not implemented yet')
      }
    }
    ctx.send({ kind: 'catalogPage', catalog: msg.catalog, items: page.items, total: page.total, requestId: msg.requestId })
  })
}
