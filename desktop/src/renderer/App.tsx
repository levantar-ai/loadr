import { useCallback, useEffect, useRef, useState } from 'react';

import { AiPanel } from './AiPanel';
import { Editor, type EditorState } from './Editor';
import { PluginsPanel } from './PluginsPanel';
import {
  activeTab, closeTab, emptyWorkspace, newTabId, openTab, selectTab, updateTab, type Workspace,
} from './workspace/tabs';
import { Badge, Button } from './ui/controls';
import { Copy, FolderOpen, Import, Plus, Puzzle, Sparkles, X } from './ui/icons';

const STARTER = `name: new plan
scenarios:
  default:
    executor: constant-vus
    vus: 1
    duration: 30s
    flow:
      - request:
          method: GET
          url: https://example.com
`;

interface Seed {
  content: string;
  path: string | null;
}

const basename = (p: string) => p.split(/[\\/]/).pop() || p;

// M3: the tabbed multi-plan workspace. Each tab owns an Editor (kept mounted so
// switching preserves edits); the tab bar reflects dirty state and title. New /
// Open / Import (via `loadr convert`) / Duplicate create tabs.
export default function App() {
  const [ws, setWs] = useState<Workspace>(() => openTab(emptyWorkspace, { id: newTabId(), title: 'untitled' }));
  const [seeds, setSeeds] = useState<Record<string, Seed>>(() => {
    const first = ws.tabs[0].id;
    return { [first]: { content: STARTER, path: null } };
  });
  const yamlRef = useRef<Record<string, string>>({});
  const [version, setVersion] = useState('');
  const [engineProblem, setEngineProblem] = useState<string | null>(null);
  const [showPlugins, setShowPlugins] = useState(false);
  const [showAi, setShowAi] = useState(false);

  useEffect(() => {
    // One health check on startup: a broken engine (missing binary, wrong CPU
    // arch, no exec permission) is diagnosed once, up front, with a plain-English
    // fix — instead of surfacing a cryptic spawn errno on the first run.
    window.loadr
      ?.doctor()
      .then((h) => {
        setVersion(h.version ?? 'loadr unavailable');
        setEngineProblem(h.ok ? null : (h.problem ?? 'The loadr engine could not be started.'));
      })
      .catch((e: Error) => {
        setVersion('loadr unavailable');
        setEngineProblem(e.message);
      });
  }, []);

  const report = useCallback((id: string, s: EditorState) => {
    yamlRef.current[id] = s.yaml;
    setWs((prev) => {
      const t = prev.tabs.find((x) => x.id === id);
      if (!t) return prev;
      const title = s.path ? basename(s.path) : t.title;
      if (t.dirty === s.dirty && t.path === s.path && t.title === title) return prev;
      return updateTab(prev, id, { dirty: s.dirty, path: s.path, title });
    });
  }, []);

  function newTab(content = STARTER, path: string | null = null, title = 'untitled') {
    const id = newTabId();
    setSeeds((s) => ({ ...s, [id]: { content, path } }));
    setWs((prev) => openTab(prev, { id, path, title }));
  }

  async function openFile() {
    const o = await window.loadr.openPlan();
    if (o) newTab(o.content, o.path, basename(o.path));
  }

  async function importFile() {
    const o = await window.loadr.importPlan();
    if (o) newTab(o.content, null, `${basename(o.path)} (imported)`);
  }

  function duplicate() {
    const at = activeTab(ws);
    if (at) newTab(yamlRef.current[at.id] ?? seeds[at.id]?.content ?? STARTER, null, `${at.title} copy`);
  }

  return (
    <div className="flex h-screen flex-col bg-ink text-ash">
      <header className="flex items-center justify-between border-b border-edge bg-coal px-4 py-2.5">
        <div className="flex items-center gap-2.5">
          <span className="grid h-7 w-7 place-items-center rounded-lg bg-ink">
            <svg viewBox="0 0 51 74" className="h-4 w-auto" aria-hidden="true">
              <path d="M17 0 L38 0 L20 36 L50 40 L11 73 L20 48 L1 44 Z" fill="#FD1E2E" />
            </svg>
          </span>
          <span className="text-[15px] font-extrabold tracking-tight text-white">
            loadr <span className="font-medium text-smoke">Desktop</span>
          </span>
          <Badge tone="ember">Beta</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="primary" icon={Sparkles} onClick={() => setShowAi(true)}>AI</Button>
          <div className="mx-1 h-5 w-px bg-edge" />
          <Button icon={Plus} onClick={() => newTab()}>New</Button>
          <Button icon={FolderOpen} onClick={openFile}>Open…</Button>
          <Button icon={Import} onClick={importFile}>Import…</Button>
          <Button icon={Copy} onClick={duplicate}>Duplicate</Button>
          <div className="mx-1 h-5 w-px bg-edge" />
          <Button icon={Puzzle} onClick={() => setShowPlugins(true)}>Plugins</Button>
        </div>
      </header>

      {engineProblem && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-blood/40 bg-blood/10 px-4 py-2 text-sm text-flare"
        >
          <span aria-hidden className="mt-0.5 font-bold">⚠</span>
          <div>
            <span className="font-semibold">loadr engine problem.</span>{' '}
            <span className="text-ash">{engineProblem}</span>
          </div>
        </div>
      )}

      {showPlugins && <PluginsPanel onClose={() => setShowPlugins(false)} />}
      {showAi && (
        <AiPanel
          onClose={() => setShowAi(false)}
          onGenerated={(yaml, title) => newTab(yaml, null, title)}
        />
      )}

      <div role="tablist" className="flex items-center gap-1 overflow-x-auto border-b border-edge bg-coal px-2 pt-1">
        {ws.tabs.map((t) => {
          const active = t.id === ws.activeId;
          return (
            <div
              key={t.id}
              role="tab"
              aria-selected={active}
              className={`group flex items-center gap-1.5 rounded-t-lg border-b-2 px-3 py-2 text-sm transition-colors ${
                active ? 'border-ember bg-ink text-white' : 'border-transparent text-smoke hover:bg-panel/60 hover:text-ash'
              }`}
            >
              <button className="flex items-center gap-1.5" onClick={() => setWs(selectTab(ws, t.id))}>
                {t.dirty && <span className="h-1.5 w-1.5 rounded-full bg-warn" aria-label="unsaved changes" />}
                {t.title}
              </button>
              <button
                aria-label={`close ${t.title}`}
                className="grid h-4 w-4 place-items-center rounded text-mist opacity-0 transition group-hover:opacity-100 hover:bg-edge hover:text-flare"
                onClick={() => setWs(closeTab(ws, t.id))}
              >
                <X />
              </button>
            </div>
          );
        })}
      </div>

      <div className="relative flex-1 overflow-hidden">
        {ws.tabs.map((t) => (
          <div key={t.id} className="absolute inset-0" hidden={t.id !== ws.activeId}>
            <Editor
              seedContent={seeds[t.id]?.content ?? STARTER}
              seedPath={seeds[t.id]?.path ?? null}
              onState={(s) => report(t.id, s)}
            />
          </div>
        ))}
        {ws.tabs.length === 0 && (
          <div className="grid h-full place-items-center text-smoke">
            <p>No open plans. Click <strong className="text-ash">New</strong> to start.</p>
          </div>
        )}
      </div>

      <footer className="flex items-center gap-2 border-t border-edge bg-coal px-4 py-1 text-xs text-mist">
        <span className={`h-1.5 w-1.5 rounded-full ${engineProblem ? 'bg-blood' : 'bg-ok'}`} />
        <span className="font-mono">{version || 'loadr —'}</span>
      </footer>
    </div>
  );
}
