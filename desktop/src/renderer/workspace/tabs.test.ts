import { describe, expect, it } from 'vitest';

import {
  activeTab, closeTab, emptyWorkspace, openTab, reorderTabs, selectTab, setDirty, setSaved,
} from './tabs';

describe('tab workspace reducer', () => {
  it('opens tabs and focuses the new one', () => {
    let ws = openTab(emptyWorkspace, { id: 'a', title: 'A' });
    ws = openTab(ws, { id: 'b', title: 'B' });
    expect(ws.tabs.map((t) => t.id)).toEqual(['a', 'b']);
    expect(ws.activeId).toBe('b');
  });

  it('re-focuses an already-open file instead of duplicating', () => {
    let ws = openTab(emptyWorkspace, { id: 'a', path: '/plans/x.yaml' });
    ws = openTab(ws, { id: 'b', title: 'other' });
    ws = openTab(ws, { id: 'c', path: '/plans/x.yaml' });
    expect(ws.tabs).toHaveLength(2);
    expect(ws.activeId).toBe('a');
  });

  it('closing the active tab activates a neighbour', () => {
    let ws = openTab(openTab(openTab(emptyWorkspace, { id: 'a' }), { id: 'b' }), { id: 'c' });
    ws = selectTab(ws, 'b');
    ws = closeTab(ws, 'b');
    expect(ws.tabs.map((t) => t.id)).toEqual(['a', 'c']);
    expect(ws.activeId).toBe('c'); // neighbour to the right
  });

  it('closing the last tab clears active', () => {
    let ws = openTab(emptyWorkspace, { id: 'a' });
    ws = closeTab(ws, 'a');
    expect(ws.tabs).toHaveLength(0);
    expect(ws.activeId).toBeNull();
  });

  it('tracks dirty and clears it on save with a new path/title', () => {
    let ws = openTab(emptyWorkspace, { id: 'a', title: 'untitled' });
    ws = setDirty(ws, 'a', true);
    expect(activeTab(ws)!.dirty).toBe(true);
    ws = setSaved(ws, 'a', '/plans/saved.yaml');
    expect(activeTab(ws)!.dirty).toBe(false);
    expect(activeTab(ws)!.title).toBe('saved.yaml');
  });

  it('reorders tabs (drag)', () => {
    let ws = openTab(openTab(openTab(emptyWorkspace, { id: 'a' }), { id: 'b' }), { id: 'c' });
    ws = reorderTabs(ws, 2, 0);
    expect(ws.tabs.map((t) => t.id)).toEqual(['c', 'a', 'b']);
  });
});
