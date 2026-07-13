// The step palette. Drag a kind onto a scenario or a branch container on the
// canvas to compose a new step there. Uses native HTML drag-and-drop; the
// canvas reads the kind off the drop event.

import { STEP_KINDS, type StepKind } from '../../shared/types';
import { STEP_ICON } from '../ui/icons';

export const DND_MIME = 'application/loadr-step-kind';

export function Palette() {
  return (
    <div className="flex h-full flex-col bg-coal">
      <div className="px-3 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-mist">Steps</div>
      <div className="grid flex-1 auto-rows-min grid-cols-2 gap-1.5 overflow-y-auto px-2 pb-3">
        {STEP_KINDS.map((kind) => (
          <PaletteItem key={kind} kind={kind} />
        ))}
      </div>
      <p className="border-t border-edge px-3 py-2 text-[11px] leading-snug text-mist">
        Drag a step onto a scenario or branch to add it there.
      </p>
    </div>
  );
}

function PaletteItem({ kind }: { kind: StepKind }) {
  const IconC = STEP_ICON[kind];
  return (
    <div
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DND_MIME, kind);
        e.dataTransfer.effectAllowed = 'copy';
      }}
      className="flex cursor-grab items-center gap-1.5 rounded-lg border border-edge bg-panel px-2 py-1.5 text-xs text-ash transition-colors hover:border-ember/60 active:cursor-grabbing"
      title={`Add a ${kind} step`}
    >
      {IconC ? <span className="text-smoke"><IconC /></span> : <span className="h-1.5 w-1.5 rounded-full bg-edge-bright" />}
      <span className="truncate">{kind}</span>
    </div>
  );
}
