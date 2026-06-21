import { useEffect, useState } from 'react';

import { Button, Field, IconButton, Segmented, Select, Textarea, TextInput } from './ui/controls';
import { FolderOpen, Key, Sparkles, X } from './ui/icons';

const MODELS = [
  { value: 'claude-sonnet-4-6', label: 'Sonnet — balanced' },
  { value: 'claude-opus-4-8', label: 'Opus — most capable' },
  { value: 'claude-haiku-4-5-20251001', label: 'Haiku — fastest' },
];

type Mode = 'prompt' | 'repo';

// "Generate with AI": describe a test in natural language, or point loadr at a
// repository, and get a validated plan in a new tab. The Anthropic key lives in
// the OS keychain (main process); we only ask whether one is set.
export function AiPanel({ onClose, onGenerated }: { onClose: () => void; onGenerated: (yaml: string, title: string) => void }) {
  const [mode, setMode] = useState<Mode>('prompt');
  const [prompt, setPrompt] = useState('');
  const [source, setSource] = useState('');
  const [model, setModel] = useState(() => localStorage.getItem('loadr.ai.model') || MODELS[0].value);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [keyInput, setKeyInput] = useState('');
  const [editingKey, setEditingKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => { window.loadr.ai.hasKey().then(setHasKey).catch(() => setHasKey(false)); }, []);
  useEffect(() => { localStorage.setItem('loadr.ai.model', model); }, [model]);

  async function saveKey() {
    if (!keyInput.trim()) return;
    await window.loadr.ai.setKey(keyInput.trim());
    setKeyInput('');
    setEditingKey(false);
    setHasKey(true);
  }

  async function browse() {
    const dir = await window.loadr.ai.browseRepo();
    if (dir) setSource(dir);
  }

  const canGenerate = hasKey && !busy && (mode === 'prompt' ? prompt.trim().length > 0 : source.trim().length > 0);

  async function generate() {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const r = await window.loadr.ai.generate({ mode, prompt: prompt.trim(), source: source.trim(), model });
      const title = (r.yaml.match(/^name:\s*(.+)$/m)?.[1]?.trim() || 'ai plan').slice(0, 40);
      onGenerated(r.yaml, title);
      if (!r.valid) {
        const n = r.diagnostics.filter((d) => d.severity === 'error').length;
        setNote(`Opened in a new tab, but it still has ${n} validation error(s) after a repair pass — review the highlighted fields.`);
      }
      // Keep the panel open only if there's a note to show; otherwise close.
      if (r.valid) onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 p-6 backdrop-blur-sm" role="dialog" aria-label="Generate with AI">
      <div className="flex max-h-[85vh] w-[44rem] flex-col rounded-2xl border border-edge bg-panel shadow-2xl shadow-black/60">
        <div className="flex items-center justify-between border-b border-edge px-4 py-3">
          <h2 className="flex items-center gap-2 font-bold text-white"><span className="text-flare"><Sparkles /></span>Generate with AI</h2>
          <div className="flex items-center gap-1">
            {hasKey && !editingKey && (
              <button onClick={() => setEditingKey(true)} className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-mist hover:text-ash" title="Change API key">
                <Key /> key set
              </button>
            )}
            <IconButton icon={X} label="close" onClick={onClose} />
          </div>
        </div>

        <div className="flex-1 space-y-4 overflow-y-auto p-4">
          {hasKey === false || editingKey ? (
            <div className="rounded-xl border border-ember/40 bg-ember/5 p-3">
              <Field label="Anthropic API key" hint="stored encrypted in your OS keychain; used only to call the Claude API from this app">
                <div className="flex gap-2">
                  <TextInput type="password" value={keyInput} placeholder="sk-ant-…" onChange={(e) => setKeyInput(e.target.value)} />
                  <Button variant="primary" onClick={saveKey} disabled={!keyInput.trim()}>Save</Button>
                  {editingKey && <Button onClick={() => setEditingKey(false)}>Cancel</Button>}
                </div>
              </Field>
            </div>
          ) : null}

          <div className="flex items-center justify-between gap-3">
            <Segmented
              ariaLabel="generation source"
              value={mode}
              onChange={setMode}
              options={[{ value: 'prompt', label: 'Describe', icon: Sparkles }, { value: 'repo', label: 'From repository', icon: FolderOpen }]}
            />
            <Field label="Model" className="w-56">
              <Select value={model} onChange={(e) => setModel(e.target.value)}>
                {MODELS.map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}
              </Select>
            </Field>
          </div>

          {mode === 'prompt' ? (
            <Field label="Describe the load test" hint="e.g. “100 VUs for 2 minutes against POST /checkout with a JSON body, assert 200 and p95 < 400ms”">
              <Textarea rows={5} value={prompt} placeholder="Ramp to 200 users over 1m hitting GET /api/products and /api/products/${id}, then hold 3m…" onChange={(e) => setPrompt(e.target.value)} />
            </Field>
          ) : (
            <div className="space-y-3">
              <Field label="Repository" hint="a local folder or an https git URL — loadr reads its OpenAPI spec / routes to build the test">
                <div className="flex gap-2">
                  <TextInput value={source} placeholder="/path/to/repo  or  https://github.com/org/api.git" onChange={(e) => setSource(e.target.value)} />
                  <Button icon={FolderOpen} onClick={browse}>Browse…</Button>
                </div>
              </Field>
              <Field label="Focus (optional)" hint="anything to steer the test — endpoints to emphasise, load profile, auth">
                <TextInput value={prompt} placeholder="emphasise the checkout flow; ramp to 300 VUs" onChange={(e) => setPrompt(e.target.value)} />
              </Field>
            </div>
          )}

          {error && <p className="rounded-lg border border-blood/40 bg-blood/10 px-2.5 py-1.5 text-xs text-flare">✗ {error}</p>}
          {note && <p className="rounded-lg border border-warn/40 bg-warn/10 px-2.5 py-1.5 text-xs text-warn">{note}</p>}
        </div>

        <div className="flex items-center justify-between gap-2 border-t border-edge px-4 py-3">
          <span className="text-xs text-mist">{busy ? 'Generating & validating against the loadr CLI…' : 'Output is validated and opens in a new tab.'}</span>
          <Button variant="primary" icon={Sparkles} onClick={generate} disabled={!canGenerate}>
            {busy ? 'Generating…' : 'Generate plan'}
          </Button>
        </div>
      </div>
    </div>
  );
}
