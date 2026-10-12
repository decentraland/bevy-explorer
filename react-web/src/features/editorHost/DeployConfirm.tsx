// The page's own "sign this deployment?" and "sign this undeploy?" dialogs: the editor asks, the
// player answers here, and only a yes reaches the signer (host/host.ts).

import { showDialog } from '../../design'
import type { DeploymentRequest, UndeployRequest } from './host/host'
import styles from './DeployConfirm.module.css'

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

const short = (wallet: string): React.JSX.Element => (
  <dd className={styles.wallet} title={wallet}>
    {wallet.slice(0, 6)}…{wallet.slice(-4)}
  </dd>
)

/** Resolves true only when the player chose to sign. `server`: the host it publishes to. */
export function confirmDeployment(request: DeploymentRequest, wallet: string, server: string): Promise<boolean> {
  return showDialog({
    title: 'Publish this scene?',
    // the editor's UI is fixed device px, and a stray click must not cancel the publish
    fixed: true,
    dismissible: false,
    body: (
      <>
        <dl className={styles.facts}>
          <dt>World</dt>
          <dd>{request.world}</dd>
          <dt>Scene</dt>
          <dd>{request.title}</dd>
          <dt>Files</dt>
          <dd>
            {request.fileCount} ({size(request.bytes)})
          </dd>
          <dt>Server</dt>
          <dd>{server}</dd>
          <dt>Signed by</dt>
          {short(wallet)}
        </dl>
        <p className={styles.note}>Signing is free — it proves this publish comes from your account. You sign the scene as the editor built it; the server is where the editor sends it.</p>
      </>
    ),
    actions: [
      { id: 'cancel', label: 'Cancel', variant: 'secondary' },
      { id: 'sign', label: 'Sign and publish' }
    ]
  }).then((choice) => choice === 'sign')
}

/** Resolves true only when the player chose to sign removing the scene at `request.coordinate`,
 *  or every scene in the world when it is null. Closes, declined, when `signal` aborts. */
export function confirmUndeploy(request: UndeployRequest, wallet: string, server: string, signal?: AbortSignal): Promise<boolean> {
  const all = request.coordinate == null
  return showDialog({
    title: all ? 'Unpublish this world?' : 'Unpublish this scene?',
    fixed: true,
    dismissible: false,
    signal,
    body: (
      <>
        <dl className={styles.facts}>
          <dt>World</dt>
          <dd>{request.world}</dd>
          <dt>Parcel</dt>
          <dd>{request.coordinate ?? 'All scenes'}</dd>
          <dt>Server</dt>
          <dd>{server}</dd>
          <dt>Signed by</dt>
          {short(wallet)}
        </dl>
        <p className={styles.note}>
          {all ? 'Every scene in this world is removed for everyone.' : 'The scene on this parcel is removed from the world for everyone.'} Signing is free — it
          proves the request comes from your account.
        </p>
      </>
    ),
    actions: [
      { id: 'cancel', label: 'Cancel', variant: 'secondary' },
      { id: 'sign', label: all ? 'Sign and unpublish all' : 'Sign and unpublish' }
    ]
  }).then((choice) => choice === 'sign')
}
