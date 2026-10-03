// The page's own "sign this deployment?" dialog: the editor asks, the player answers here, and
// only a yes reaches the signer (host/host.ts signDeployment).

import { showDialog } from '../../design'
import type { DeploymentRequest } from './host/host'
import styles from './DeployConfirm.module.css'

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

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
          <dd className={styles.wallet} title={wallet}>
            {wallet.slice(0, 6)}…{wallet.slice(-4)}
          </dd>
        </dl>
        <p className={styles.note}>Signing is free — it proves this publish comes from your account.</p>
      </>
    ),
    actions: [
      { id: 'cancel', label: 'Cancel', variant: 'secondary' },
      { id: 'sign', label: 'Sign and publish' }
    ]
  }).then((choice) => choice === 'sign')
}
