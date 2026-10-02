// The page's own "sign this deployment?" dialog: the editor asks, the player answers here, and
// only a yes reaches the signer (host/host.ts signDeployment).

import { showDialog } from '../../design'
import type { DeploymentRequest } from './host/host'
import styles from './DeployConfirm.module.css'

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

/** Resolves true only when the player chose to sign. */
export function confirmDeployment(request: DeploymentRequest, wallet: string): Promise<boolean> {
  return showDialog({
    title: 'Publish this scene?',
    body: (
      <dl className={styles.facts}>
        <dt>World</dt>
        <dd>{request.world}</dd>
        <dt>Scene</dt>
        <dd>{request.title}</dd>
        <dt>Files</dt>
        <dd>
          {request.fileCount} ({size(request.bytes)})
        </dd>
        <dt>Signed by</dt>
        <dd className={styles.wallet}>{wallet}</dd>
      </dl>
    ),
    actions: [
      { id: 'cancel', label: 'Cancel', variant: 'secondary' },
      { id: 'sign', label: 'Sign and publish' }
    ]
  }).then((choice) => choice === 'sign')
}
