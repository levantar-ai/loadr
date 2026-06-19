// Pure reducer for the tabbed multi-plan workspace. Tab *metadata* lives here
// (id, title, file path, dirty); each tab's editing document (usePlanDoc) is
// held by the component keyed on the tab id. Keeping this pure makes the tab
// behaviour — open, close (incl. picking the next active tab), select, rename,
// dirty, reorder — headless-testable.

export interface Tab {
  id: string;
  title: string;
  path: string | null;
  dirty: boolean;
}

export interface Workspace {
  tabs: Tab[];
  activeId: string | null;
}

export const emptyWorkspace: Workspace = { tabs: [], activeId: null };

let seq = 0;
/** Monotonic id; deterministic so tests don't depend on time/random. */
export function newTabId(): string {
  seq += 1;
  return `tab-${seq}`;
}

function titleFor(path: string | null, fallback: string): string {
  if (!path) return fallback;
  return path.split(/[\\/]/).pop() || fallback;
}

export function openTab(ws: Workspace, opts: { id?: string; path?: string | null; title?: string }): Workspace {
  const path = opts.path ?? null;
  // Re-focus an already-open file rather than opening a duplicate.
  if (path) {
    const existing = ws.tabs.find((t) => t.path === path);
    if (existing) return { ...ws, activeId: existing.id };
  }
  const id = opts.id ?? newTabId();
  const tab: Tab = { id, path, title: opts.title ?? titleFor(path, 'untitled'), dirty: false };
  return { tabs: [...ws.tabs, tab], activeId: id };
}

export function closeTab(ws: Workspace, id: string): Workspace {
  const idx = ws.tabs.findIndex((t) => t.id === id);
  if (idx === -1) return ws;
  const tabs = ws.tabs.filter((t) => t.id !== id);
  let activeId = ws.activeId;
  if (activeId === id) {
    // Activate the neighbour to the right, else the left, else none.
    activeId = tabs[idx]?.id ?? tabs[idx - 1]?.id ?? null;
  }
  return { tabs, activeId };
}

export function selectTab(ws: Workspace, id: string): Workspace {
  return ws.tabs.some((t) => t.id === id) ? { ...ws, activeId: id } : ws;
}

export function updateTab(ws: Workspace, id: string, patch: Partial<Omit<Tab, 'id'>>): Workspace {
  return { ...ws, tabs: ws.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)) };
}

export function setDirty(ws: Workspace, id: string, dirty: boolean): Workspace {
  return updateTab(ws, id, { dirty });
}

export function setSaved(ws: Workspace, id: string, path: string): Workspace {
  return updateTab(ws, id, { path, title: titleFor(path, 'untitled'), dirty: false });
}

export function reorderTabs(ws: Workspace, from: number, to: number): Workspace {
  if (from < 0 || to < 0 || from >= ws.tabs.length || to >= ws.tabs.length) return ws;
  const tabs = [...ws.tabs];
  const [moved] = tabs.splice(from, 1);
  tabs.splice(to, 0, moved);
  return { ...ws, tabs };
}

export function activeTab(ws: Workspace): Tab | null {
  return ws.tabs.find((t) => t.id === ws.activeId) ?? null;
}
