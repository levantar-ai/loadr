import { useEffect, useState } from 'react';

import { parsePlan, type PlanParseError } from '../shared/plan';
import { stepKind, type Plan, type Scenario, type Step } from '../shared/types';
import type { ValidateResult } from '../preload';

// Milestone 1: open an existing plan and render it (read-only). The form
// editor + drag-and-drop composition land in milestones 2–3; this proves the
// open→parse→render path and the CLI-backed validation surface.
export default function App() {
  const [path, setPath] = useState<string | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [validation, setValidation] = useState<ValidateResult | null>(null);
  const [version, setVersion] = useState<string>('');

  useEffect(() => {
    window.loadr.version().then(setVersion).catch(() => setVersion('loadr not found'));
  }, []);

  async function open() {
    const opened = await window.loadr.openPlan();
    if (!opened) return;
    setPath(opened.path);
    setError(null);
    setValidation(null);
    try {
      setPlan(parsePlan(opened.content));
    } catch (e) {
      setPlan(null);
      setError((e as PlanParseError).message);
      return;
    }
    setValidation(await window.loadr.validate(opened.content));
  }

  return (
    <div className="flex min-h-screen flex-col">
      <header className="flex items-center justify-between border-b border-[#232330] px-5 py-3">
        <div className="flex items-center gap-3">
          <span className="text-lg font-extrabold">
            loadr <span className="font-medium text-[#9ca3af]">Desktop</span>
          </span>
          <span className="rounded-full border border-[#ef4444]/40 bg-[#ef4444]/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-[#f87171]">
            Beta
          </span>
        </div>
        <button
          onClick={open}
          className="rounded-lg bg-[#dc2626] px-4 py-2 text-sm font-semibold text-white"
        >
          Open plan…
        </button>
      </header>

      <main className="flex-1 p-6">
        {!plan && !error && (
          <p className="text-[#9ca3af]">Open a loadr <code>.yaml</code> plan to render it.</p>
        )}
        {error && (
          <div className="rounded-lg border border-[#ef4444]/50 bg-[#ef4444]/10 p-4 text-[#fca5a5]">
            <strong>Could not parse:</strong> {error}
          </div>
        )}
        {plan && (
          <div className="space-y-6">
            <div>
              <h1 className="text-2xl font-extrabold text-white">{plan.name ?? '(unnamed plan)'}</h1>
              {path && <p className="text-xs text-[#6b7280]">{path}</p>}
              {plan.description && <p className="mt-1 text-sm text-[#9ca3af]">{plan.description}</p>}
            </div>

            {validation && <ValidationBadge result={validation} />}

            <PlanView plan={plan} />
          </div>
        )}
      </main>

      <footer className="border-t border-[#232330] px-5 py-2 text-xs text-[#6b7280]">{version}</footer>
    </div>
  );
}

function ValidationBadge({ result }: { result: ValidateResult }) {
  const errors = result.diagnostics.filter((d) => d.severity === 'error');
  const warnings = result.diagnostics.filter((d) => d.severity === 'warning');
  const cls = result.ok
    ? 'border-[#4ade80]/40 bg-[#4ade80]/10 text-[#86efac]'
    : 'border-[#ef4444]/50 bg-[#ef4444]/10 text-[#fca5a5]';
  return (
    <div className={`rounded-lg border p-3 text-sm ${cls}`}>
      {result.ok ? '✓ valid' : `✗ ${errors.length} error(s)`}
      {warnings.length > 0 && ` · ${warnings.length} warning(s)`}
      {result.diagnostics.slice(0, 6).map((d, i) => (
        <div key={i} className="mt-1 text-xs opacity-90">
          [{d.severity}] {d.message}
        </div>
      ))}
    </div>
  );
}

function PlanView({ plan }: { plan: Plan }) {
  const scenarios = Object.entries(plan.scenarios ?? {});
  if (scenarios.length === 0) return <p className="text-[#9ca3af]">No scenarios.</p>;
  return (
    <div className="space-y-4">
      {scenarios.map(([name, sc]) => (
        <ScenarioView key={name} name={name} scenario={sc} />
      ))}
    </div>
  );
}

function ScenarioView({ name, scenario }: { name: string; scenario: Scenario }) {
  const flow = scenario.flow ?? [];
  return (
    <section className="rounded-xl border border-[#232330] bg-[#141419] p-4">
      <div className="flex items-baseline gap-3">
        <h2 className="font-bold text-white">{name}</h2>
        <code className="rounded bg-[#0d0d12] px-2 py-0.5 text-xs text-[#f87171]">
          {scenario.executor}
        </code>
        <span className="text-xs text-[#6b7280]">
          {scenario.vus != null && `${scenario.vus} VUs`} {scenario.duration ?? ''}
        </span>
      </div>
      <ol className="mt-3 space-y-1">
        {flow.map((step, i) => (
          <li key={i} className="flex gap-2 text-sm">
            <span className="text-[#6b7280]">{i + 1}.</span>
            <StepView step={step} />
          </li>
        ))}
      </ol>
    </section>
  );
}

function StepView({ step }: { step: Step }) {
  const kind = stepKind(step);
  if (!kind) return <span className="text-[#fca5a5]">unknown step</span>;
  const body = step[kind] as Record<string, unknown> | string | undefined;
  let summary = '';
  if (kind === 'request' && body && typeof body === 'object') {
    summary = `${(body.method as string) ?? 'GET'} ${(body.url as string) ?? ''}`;
  } else if (typeof body === 'string') {
    summary = body;
  } else if (body && typeof body === 'object' && 'name' in body) {
    summary = String((body as { name: unknown }).name);
  }
  return (
    <span>
      <code className="text-[#fb923c]">{kind}</code>
      {summary && <span className="ml-2 text-[#9ca3af]">{summary}</span>}
    </span>
  );
}
