import { useCallback, useEffect, useRef, useState } from 'react';

import { Editor, type EditorState } from './Editor';
import { PluginsPanel } from './PluginsPanel';
import {
  activeTab, closeTab, emptyWorkspace, newTabId, openTab, selectTab, updateTab, type Workspace,
} from './workspace/tabs';

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
  const [showPlugins, setShowPlugins] = useState(false);

  useEffect(() => {
    window.loadr?.version().then(setVersion).catch(() => setVersion('loadr not found'));
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
    <div className="flex h-screen flex-col">
      <header className="flex items-center justify-between border-b border-[#232330] px-4 py-2">
        <div className="flex items-center gap-3">
          <span className="font-extrabold">loadr <span className="font-medium text-[#9ca3af]">Desktop</span></span>
          <span className="rounded-full border border-[#ef4444]/40 bg-[#ef4444]/10 px-2 py-0.5 text-[10px] font-bold uppercase text-[#f87171]">Beta</span>
        </div>
        <div className="flex gap-2 text-sm">
          <button onClick={() => newTab()} className="rounded border border-[#232330] px-3 py-1 text-[#e5e7eb]">New</button>
          <button onClick={openFile} className="rounded border border-[#232330] px-3 py-1 text-[#e5e7eb]">Open…</button>
          <button onClick={importFile} className="rounded border border-[#232330] px-3 py-1 text-[#e5e7eb]">Import…</button>
          <button onClick={duplicate} className="rounded border border-[#232330] px-3 py-1 text-[#e5e7eb]">Duplicate</button>
          <button onClick={() => setShowPlugins(true)} className="rounded border border-[#232330] px-3 py-1 text-[#e5e7eb]">Plugins</button>
        </div>
      </header>

      {showPlugins && <PluginsPanel onClose={() => setShowPlugins(false)} />}

      <div role="tablist" className="flex items-center gap-1 overflow-x-auto border-b border-[#232330] bg-[#0d0d12] px-2">
        {ws.tabs.map((t) => (
          <div
            key={t.id}
            role="tab"
            aria-selected={t.id === ws.activeId}
            className={`flex items-center gap-2 border-b-2 px-3 py-1.5 text-sm ${
              t.id === ws.activeId ? 'border-[#ef4444] text-white' : 'border-transparent text-[#9ca3af]'
            }`}
          >
            <button onClick={() => setWs(selectTab(ws, t.id))}>
              {t.dirty && <span className="mr-1 text-[#fbbf24]">●</span>}
              {t.title}
            </button>
            <button aria-label={`close ${t.title}`} className="text-[#6b7280] hover:text-[#fca5a5]" onClick={() => setWs(closeTab(ws, t.id))}>
              ✕
            </button>
          </div>
        ))}
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
        {ws.tabs.length === 0 && <p className="p-6 text-[#9ca3af]">No open plans. Click <strong>New</strong>.</p>}
      </div>

      <footer className="border-t border-[#232330] px-4 py-1 text-xs text-[#6b7280]">{version}</footer>
    </div>
  );
}
