// The canvas detail pane. Selecting a node on the canvas shows its editable
// properties here — quick fields for the common kinds, plus a universal
// YAML editor of the node's own subtree so ANY node (known or future) can be
// edited. Every change flows back through the shared PlanDoc, so the YAML text,
// validation and the graph stay in lock-step.

import { useEffect, useMemo, useState } from 'react';
import yaml from 'js-yaml';

import { deleteIn, getIn, moveStepAt, setIn, type Path } from '../../shared/edit';
import type { Json, Scenario } from '../../shared/types';
import type { PlanDoc } from '../state/usePlanDoc';
import { Button, Field, IconButton, NumberInput, Select, TextInput } from '../ui/controls';
import { ArrowDown, ArrowUp, Copy, Trash, X } from '../ui/icons';
import { EXECUTORS } from '../forms/PlanForms';
import type { NodeData } from './graph';

export function Inspector({
  node,
  doc,
  onClose,
  onSelect,
}: {
  node: NodeData | null;
  doc: PlanDoc;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  if (!node) {
    return (
      <div className="grid h-full place-items-center p-6 text-center text-sm text-mist">
        Select a node on the canvas to edit it.
      </div>
    );
  }
  const value = getIn(doc.plan, node.path);
  const listPath = node.listPath;
  const index = node.index;

  const remove = () => {
    doc.apply((p) => deleteIn(p, node.path));
    onClose();
  };
  const duplicate = () => {
    if (!listPath || index == null) return;
    doc.apply((p) => {
      const arr = [...((getIn(p, listPath) as Json[]) ?? [])];
      arr.splice(index + 1, 0, structuredClone(arr[index]));
      return setIn(p, listPath, arr as unknown as Json);
    });
  };
  const move = (delta: number) => {
    if (!listPath || index == null) return;
    doc.apply((p) => moveStepAt(p, listPath, index, index + delta));
    onSelect([...listPath, index + delta].join('.'));
  };

  return (
    <div className="flex h-full flex-col bg-coal">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="min-w-0">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-mist">{node.kind}</div>
          <div className="truncate text-sm font-semibold text-white">{node.label}</div>
        </div>
        <IconButton icon={X} label="close inspector" onClick={onClose} />
      </div>

      {listPath && index != null && (
        <div className="flex items-center gap-1 border-b border-edge px-3 py-1.5">
          <IconButton icon={ArrowUp} label="move up" onClick={() => move(-1)} />
          <IconButton icon={ArrowDown} label="move down" onClick={() => move(1)} />
          <IconButton icon={Copy} label="duplicate" onClick={duplicate} />
          <span className="flex-1" />
          <IconButton icon={Trash} tone="danger" label="delete" onClick={remove} />
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-3">
        <QuickFields node={node} doc={doc} />
        <RawEditor path={node.path} value={value} doc={doc} />
      </div>
    </div>
  );
}

// Kind-specific shortcuts for the fields people touch most; the raw editor
// below covers everything else.
function QuickFields({ node, doc }: { node: NodeData; doc: PlanDoc }) {
  const p = node.path;
  if (node.kind === 'scenario') {
    const sc = getIn(doc.plan, p) as Scenario | undefined;
    return (
      <div className="mb-4 space-y-3">
        <Field label="Executor">
          <Select
            aria-label="Executor"
            value={sc?.executor ?? 'constant-vus'}
            onChange={(e) => doc.update([...p, 'executor'], e.target.value)}
          >
            {EXECUTORS.map((x) => <option key={x} value={x}>{x}</option>)}
          </Select>
        </Field>
        {sc && 'vus' in sc && (
          <Field label="VUs">
            <NumberInput value={sc.vus ?? 0} onChange={(e) => doc.update([...p, 'vus'], Number(e.target.value))} />
          </Field>
        )}
        {sc && 'duration' in sc && (
          <Field label="Duration">
            <TextInput value={sc.duration ?? ''} placeholder="30s" onChange={(e) => doc.update([...p, 'duration'], e.target.value)} />
          </Field>
        )}
      </div>
    );
  }
  if (node.kind === 'request') {
    const body = (getIn(doc.plan, [...p, 'request']) as Record<string, Json>) ?? {};
    const set = (k: string, v: Json) => doc.update([...p, 'request', k], v);
    return (
      <div className="mb-4 space-y-3">
        <div className="grid grid-cols-[110px_1fr] gap-2">
          <Field label="Method">
            <Select value={(body.method as string) ?? 'GET'} onChange={(e) => set('method', e.target.value)}>
              {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'].map((m) => <option key={m}>{m}</option>)}
            </Select>
          </Field>
          <Field label="URL">
            <TextInput aria-label="URL" value={(body.url as string) ?? ''} placeholder="/path" onChange={(e) => set('url', e.target.value)} />
          </Field>
        </div>
        <Field label="Name" hint="labels the request in metrics">
          <TextInput value={(body.name as string) ?? ''} onChange={(e) => set('name', (e.target.value || undefined) as Json)} />
        </Field>
      </div>
    );
  }
  return null;
}

// The universal escape hatch: edit the node's subtree as YAML. Applies on a
// successful parse; a bad edit shows the error and leaves the model untouched.
function RawEditor({ path, value, doc }: { path: Path; value: unknown; doc: PlanDoc }) {
  const initial = useMemo(() => (value === undefined ? '' : yaml.dump(value).trimEnd()), [value]);
  const [text, setText] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  // Re-sync from the model when the selection changes or an external edit lands
  // (but don't stomp the user mid-edit).
  useEffect(() => {
    if (!dirty) setText(initial);
  }, [initial, dirty]);

  const commit = () => {
    setDirty(false);
    try {
      const parsed = (text.trim() === '' ? undefined : yaml.load(text)) as Json;
      setError(null);
      doc.apply((p) => setIn(p, path, parsed));
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-mist">Raw (YAML)</span>
        {dirty && <Button onClick={commit}>Apply</Button>}
      </div>
      <textarea
        value={text}
        onChange={(e) => { setText(e.target.value); setDirty(true); }}
        onBlur={commit}
        spellCheck={false}
        rows={12}
        className="w-full resize-y rounded-lg border border-edge bg-ink px-2.5 py-2 font-mono text-xs text-ash outline-none focus:border-ember/50"
      />
      {error && <p className="mt-1 text-xs text-flare">{error}</p>}
    </div>
  );
}
