// Smoke test for the design primitives ported from eordano/dcl-react-ui: each must
// mount and render without throwing. Behaviour lives with its consumers; this just
// guards the port (compiles AND renders).
import { render } from '@testing-library/react'
import { describe, it, expect } from 'vitest'
import {
  Modal,
  ModalTitle,
  ModalActions,
  ModalShell,
  Checkbox,
  Spinner,
  SearchField,
  FieldLabel,
  CharCounter,
  ContextMenu,
  Dropdown,
  EmptyState,
  PageHeader,
  Coin,
  ManaIcon,
  Search,
  Close,
  SocialIcon,
  StarWalletIcon,
  DclLogomark,
  ManaMark
} from '../design'

describe('ported design primitives', () => {
  it('render without throwing', () => {
    const { container } = render(
      <>
        <Modal>modal</Modal>
        <ModalShell onClose={() => {}} title="Title">shell</ModalShell>
        <ModalTitle title="t" subtitle="s" />
        <ModalActions>
          <button>ok</button>
        </ModalActions>
        <Checkbox checked onChange={() => {}}>label</Checkbox>
        <Spinner />
        <SearchField value="" onChange={() => {}} />
        <FieldLabel htmlFor="x">Field</FieldLabel>
        <CharCounter current={3} max={10} />
        <ContextMenu items={[{ label: 'Item', onClick: () => {} }, { kind: 'separator' }]} />
        <Dropdown options={['a', 'b']} value="a" onChange={() => {}} />
        <EmptyState title="Nothing here" />
        <PageHeader title="Places" />
        <Coin />
        <ManaIcon />
        <Search />
        <Close />
        <SocialIcon name="discord" />
        <StarWalletIcon />
        <DclLogomark />
        <ManaMark />
      </>
    )
    expect(container).toBeTruthy()
  })
})
