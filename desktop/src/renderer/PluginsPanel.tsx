import { useEffect, useState } from 'react';

import type { InstalledPlugin } from '../shared/plugins';

// M5: manage protocol plugins via `loadr plugin` — list, install (by index name,
// directory or URL) and remove. Shown as a modal over the workspace.
export function PluginsPanel({ onClose }: { onClose: () => void }) {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [spec, setSpec] = useState('');
  const [allowUntrusted, setAllowUntrusted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const reload = () => window.loadr.pluginList().then(setPlugins).catch(() => {});
  useEffect(() => { reload(); }, []);

  async function install() {
    if (!spec.trim()) return;
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await window.loadr.pluginInstall(spec.trim(), allowUntrusted));
      setSpec('');
      await reload();
    } catch (e) {
      setMessage(`✗ ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  async function browse() {
    const dir = await window.loadr.pluginBrowseDir();
    if (dir) setSpec(dir);
  }

  async function remove(name: string) {
    setBusy(true);
    try {
      await window.loadr.pluginRemove(name);
      await reload();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60" role="dialog" aria-label="Plugins">
      <div className="flex max-h-[80vh] w-[40rem] flex-col rounded-xl border border-[#232330] bg-[#0d0d12] p-4">
        <div className="flex items-center justify-between">
          <h2 className="font-bold text-white">Plugins</h2>
          <button onClick={onClose} className="text-[#9ca3af] hover:text-white" aria-label="close plugins">✕</button>
        </div>

        <div className="mt-3 flex-1 overflow-y-auto">
          {plugins.length === 0 ? (
            <p className="text-sm text-[#6b7280]">No plugins installed.</p>
          ) : (
            <table className="w-full text-sm">
              <thead><tr className="text-left text-[#6b7280]"><th>name</th><th>kind</th><th>version</th><th>state</th><th></th></tr></thead>
              <tbody>
                {plugins.map((p) => (
                  <tr key={p.name} className="border-t border-[#232330]/60">
                    <td className="py-1 font-mono text-[#e5e7eb]">{p.name}</td>
                    <td className="text-[#9ca3af]">{p.kind}</td>
                    <td className="text-[#9ca3af]">{p.version}</td>
                    <td className={p.state === 'enabled' ? 'text-[#86efac]' : 'text-[#fca5a5]'}>{p.state}</td>
                    <td className="text-right"><button disabled={busy} onClick={() => remove(p.name)} className="text-xs text-[#fca5a5]">remove</button></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="mt-3 border-t border-[#232330] pt-3">
          <label className="text-xs font-semibold text-[#9ca3af]">Install (index name, directory or URL)</label>
          <div className="mt-1 flex gap-2">
            <input
              className="flex-1 rounded border border-[#232330] bg-[#141419] px-2 py-1 text-sm text-[#e5e7eb]"
              value={spec}
              placeholder="mongo  ·  /path/to/dist  ·  github:owner/repo"
              onChange={(e) => setSpec(e.target.value)}
              aria-label="plugin spec"
            />
            <button onClick={browse} className="rounded border border-[#232330] px-2 text-sm text-[#e5e7eb]">Browse…</button>
            <button onClick={install} disabled={busy || !spec.trim()} className="rounded bg-[#dc2626] px-3 text-sm font-semibold text-white disabled:opacity-50">Install</button>
          </div>
          <label className="mt-2 flex items-center gap-2 text-xs text-[#6b7280]">
            <input type="checkbox" checked={allowUntrusted} onChange={(e) => setAllowUntrusted(e.target.checked)} />
            allow untrusted (non-index sources)
          </label>
          {message && <pre className="mt-2 max-h-24 overflow-y-auto whitespace-pre-wrap text-xs text-[#9ca3af]">{message}</pre>}
        </div>
      </div>
    </div>
  );
}
