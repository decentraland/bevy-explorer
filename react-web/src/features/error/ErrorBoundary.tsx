// Catches render crashes in the react-web HUD and shows the same error popup the engine uses, so a
// UI exception degrades to a recoverable "Reload" screen instead of a blank page.

import { Component, type ReactNode } from 'react'
import { EngineErrorModal } from './EngineErrorModal'

interface State {
  error: Error | null
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error): void {
    console.error('[react error boundary]', error)
  }

  render(): ReactNode {
    const { error } = this.state
    if (error) {
      return (
        <EngineErrorModal
          error={{ message: error.stack ?? error.message, source: 'react' }}
          onReload={() => location.reload()}
        />
      )
    }
    return this.props.children
  }
}
