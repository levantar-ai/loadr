import type { DragEndEvent } from '@dnd-kit/core';

// Sortable items in the flow use their stringified index as id. A drop maps to
// a (from, to) reorder. Pure so it's headless-testable; the moveStep it feeds is
// already covered in edit.test.ts. The dnd-kit wiring itself is e2e-verified.
export function dragEndIndices(event: DragEndEvent): { from: number; to: number } | null {
  const from = Number(event.active.id);
  const to = event.over ? Number(event.over.id) : NaN;
  if (Number.isNaN(from) || Number.isNaN(to) || from === to) return null;
  return { from, to };
}
