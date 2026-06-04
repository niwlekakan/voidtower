import { useEffect } from 'react'

type ModKey = 'ctrl' | 'meta' | 'shift' | 'alt'

interface Binding {
  key: string
  mods?: ModKey[]
  handler: (e: KeyboardEvent) => void
}

export function useKeyboard(bindings: Binding[]) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      for (const b of bindings) {
        const mods = b.mods ?? []
        const ctrlOk  = mods.includes('ctrl')  ? e.ctrlKey  : !e.ctrlKey
        const metaOk  = mods.includes('meta')  ? e.metaKey  : !e.metaKey
        const shiftOk = mods.includes('shift') ? e.shiftKey : !e.shiftKey
        const altOk   = mods.includes('alt')   ? e.altKey   : !e.altKey
        if (e.key.toLowerCase() === b.key.toLowerCase() && ctrlOk && metaOk && shiftOk && altOk) {
          b.handler(e)
        }
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [bindings])
}
