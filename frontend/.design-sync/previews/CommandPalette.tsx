import { CommandPalette, useCmdPaletteStore, MemoryRouter } from 'voidtower-frontend'
import { trapFixedAt } from './_trapFixed'

// CommandPalette uses `fixed inset-0` internally — see _trapFixed.ts.
trapFixedAt(560, 440)

// Single export only: open/query live in global Zustand state.
useCmdPaletteStore.setState({ open: true, query: '' })

export function Open() {
  return (
    <MemoryRouter>
      <CommandPalette />
    </MemoryRouter>
  )
}
