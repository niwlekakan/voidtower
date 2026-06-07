// ── BSP Tiling Engine ──────────────────────────────────────────────────────
// Binary Space Partition tree for i3/hyprland-style tiling layout.
//
// - 'h' split = horizontal divider = top/bottom children
// - 'v' split = vertical divider = left/right children

export type SplitDir = 'h' | 'v'

export interface TileNode {
  id: string
  // Leaf node (panel):
  panelId?: string
  // Branch node (split):
  dir?: SplitDir
  ratio?: number       // 0..1, default 0.5
  childA?: TileNode    // left / top
  childB?: TileNode    // right / bottom
}

export interface TileLayout {
  root: TileNode | null
  x: number
  y: number
  w: number
  h: number
}

export interface TileRect {
  x: number
  y: number
  w: number
  h: number
}

// ── Helpers ────────────────────────────────────────────────────────────────

let _nodeIdCounter = 0
function nextNodeId(): string {
  return `tn-${++_nodeIdCounter}`
}

/** Deep-clone a node tree */
function cloneNode(node: TileNode): TileNode {
  const out: TileNode = { id: node.id }
  if (node.panelId !== undefined) out.panelId = node.panelId
  if (node.dir     !== undefined) out.dir     = node.dir
  if (node.ratio   !== undefined) out.ratio   = node.ratio
  if (node.childA  !== undefined) out.childA  = cloneNode(node.childA)
  if (node.childB  !== undefined) out.childB  = cloneNode(node.childB)
  return out
}

/** Find the leaf node for a panelId, returning the node and its parent. */
function findLeaf(
  node: TileNode,
  panelId: string,
  parent: TileNode | null = null,
  side: 'A' | 'B' | null = null,
): { node: TileNode; parent: TileNode | null; side: 'A' | 'B' | null } | null {
  if (node.panelId === panelId) return { node, parent, side }
  if (node.childA) {
    const r = findLeaf(node.childA, panelId, node, 'A')
    if (r) return r
  }
  if (node.childB) {
    const r = findLeaf(node.childB, panelId, node, 'B')
    if (r) return r
  }
  return null
}

/** Find a branch node by its id. */
function findBranch(node: TileNode, id: string): TileNode | null {
  if (node.id === id) return node
  if (node.childA) {
    const r = findBranch(node.childA, id)
    if (r) return r
  }
  if (node.childB) {
    const r = findBranch(node.childB, id)
    if (r) return r
  }
  return null
}

/** Collect all panelIds in the tree. */
export function collectPanelIds(node: TileNode | null): string[] {
  if (!node) return []
  if (node.panelId) return [node.panelId]
  return [
    ...collectPanelIds(node.childA ?? null),
    ...collectPanelIds(node.childB ?? null),
  ]
}

// ── Core mutations ─────────────────────────────────────────────────────────

/**
 * Insert a new panel into the tree by splitting `targetId`.
 * If tree is empty, creates a single leaf for newPanelId.
 * If targetId is not found, appends to the rightmost leaf.
 */
export function insertPanel(
  tree: TileLayout,
  newPanelId: string,
  targetId: string | null,
  dir: SplitDir,
): TileLayout {
  const newLeaf: TileNode = { id: nextNodeId(), panelId: newPanelId }

  // Empty tree: create root leaf
  if (!tree.root) {
    return { ...tree, root: newLeaf }
  }

  const root = cloneNode(tree.root)

  // Find the target leaf
  const effectiveTarget = targetId ?? collectPanelIds(root)[0] ?? null
  if (!effectiveTarget) return { ...tree, root }

  const found = findLeaf(root, effectiveTarget)
  if (!found) return { ...tree, root }

  const { node, parent, side } = found

  // Create a new branch splitting the found leaf
  const branch: TileNode = {
    id: nextNodeId(),
    dir,
    ratio: 0.5,
    childA: cloneNode(node),   // existing panel goes to A (left/top)
    childB: newLeaf,            // new panel goes to B (right/bottom)
  }

  if (!parent) {
    // We're replacing root
    return { ...tree, root: branch }
  }

  // Graft branch into parent
  if (side === 'A') parent.childA = branch
  else              parent.childB = branch

  return { ...tree, root }
}

/**
 * Remove a panel leaf from the tree.
 * The sibling of the removed leaf replaces the parent branch.
 */
export function removePanel(tree: TileLayout, panelId: string): TileLayout {
  if (!tree.root) return tree

  const root = cloneNode(tree.root)

  // Special case: root is the leaf we're removing
  if (root.panelId === panelId) return { ...tree, root: null }

  const found = findLeaf(root, panelId)
  if (!found || !found.parent || !found.side) return { ...tree, root }

  const { parent, side } = found
  const sibling = side === 'A' ? parent.childB : parent.childA

  // We need to find the grandparent to replace parent with sibling
  const gp = findGrandparent(root, parent.id)
  if (!gp) {
    // parent IS root — replace root with sibling
    return { ...tree, root: sibling ?? null }
  }

  if (gp.childA?.id === parent.id) gp.childA = sibling
  else                              gp.childB = sibling

  return { ...tree, root }
}

/** Find the parent of a branch node by its id. */
function findGrandparent(node: TileNode, childId: string): TileNode | null {
  if (node.childA?.id === childId || node.childB?.id === childId) return node
  if (node.childA) {
    const r = findGrandparent(node.childA, childId)
    if (r) return r
  }
  if (node.childB) {
    const r = findGrandparent(node.childB, childId)
    if (r) return r
  }
  return null
}

/**
 * Walk the BSP tree and compute a rect for every leaf panel.
 * Returns a Map<panelId, TileRect>.
 */
export function computeRects(
  node: TileNode | null,
  bounds: TileRect,
): Map<string, TileRect> {
  const out = new Map<string, TileRect>()
  if (!node) return out

  _computeRectsInto(node, bounds, out)
  return out
}

const TILE_GAP = 4 // px gap between tiles

function _computeRectsInto(
  node: TileNode,
  bounds: TileRect,
  out: Map<string, TileRect>,
): void {
  if (node.panelId !== undefined) {
    out.set(node.panelId, bounds)
    return
  }

  if (!node.childA || !node.childB) return

  const ratio = node.ratio ?? 0.5
  const half = TILE_GAP / 2

  if (node.dir === 'v') {
    // Vertical split — left | right
    const wA = Math.floor(bounds.w * ratio) - half
    const wB = bounds.w - wA - TILE_GAP
    _computeRectsInto(node.childA, { x: bounds.x,           y: bounds.y, w: wA, h: bounds.h }, out)
    _computeRectsInto(node.childB, { x: bounds.x + wA + TILE_GAP, y: bounds.y, w: wB, h: bounds.h }, out)
  } else {
    // Horizontal split — top / bottom
    const hA = Math.floor(bounds.h * ratio) - half
    const hB = bounds.h - hA - TILE_GAP
    _computeRectsInto(node.childA, { x: bounds.x, y: bounds.y,            w: bounds.w, h: hA }, out)
    _computeRectsInto(node.childB, { x: bounds.x, y: bounds.y + hA + TILE_GAP, w: bounds.w, h: hB }, out)
  }
}

/**
 * Update the ratio of a branch node (used for resize drag).
 */
export function resizeSplit(tree: TileLayout, nodeId: string, newRatio: number): TileLayout {
  if (!tree.root) return tree
  const root = cloneNode(tree.root)
  const node = findBranch(root, nodeId)
  if (!node) return tree
  node.ratio = Math.max(0.1, Math.min(0.9, newRatio))
  return { ...tree, root }
}
