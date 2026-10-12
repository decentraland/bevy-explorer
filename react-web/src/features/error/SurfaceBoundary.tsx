import { Component, type ReactNode } from 'react'
import { showDialog } from '../../design/popups'

interface Props {
  /** Shown to the user, e.g. "Backpack". */
  name: string
  open: boolean
  onCrash: () => void
  children: ReactNode
}

interface State {
  failed: boolean
  wasOpen: boolean
}

// Keeps one crashing page or panel from taking down the whole HUD: it closes and retries on the next
// open. Resetting only on a closed→open edge stops a surface that crashes while closed from looping.
export class SurfaceBoundary extends Component<Props, State> {
  state: State = { failed: false, wasOpen: this.props.open }

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true }
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> {
    const reopened = props.open && !state.wasOpen
    return { wasOpen: props.open, failed: state.failed && !reopened }
  }

  componentDidCatch(error: Error): void {
    console.error(`[${this.props.name}] crashed`, error)
    // Closed, the user sees nothing; closing panels would only shut the one they have open.
    if (!this.props.open) return
    this.props.onCrash()
    void showDialog({
      title: `${this.props.name} ran into a problem`,
      body: 'It was closed so you can keep playing. Try opening it again.',
      actions: [{ id: 'ok', label: 'OK' }]
    })
  }

  render(): ReactNode {
    return this.state.failed ? null : this.props.children
  }
}
