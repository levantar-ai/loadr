import { useEffect, useState } from 'react';

import { compareResults, runsForPlan, type RunRecord } from '../shared/history';
import { deriveResults, parseProgressLine, type LiveMetrics, type Results } from '../shared/results';
import { Button } from './ui/controls';
import { Play } from './ui/icons';

// M4: run the current plan via the CLI, show live metrics while it runs, then
// the results; persist to history and compare against a previous run.
export function RunPanel({ yaml, planName }: { yaml: string; planName: string }) {
  const [running, setRunning] = useState(false);
  const [live, setLive] = useState<LiveMetrics | null>(null);
  const [results, setResults] = useState<Results | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<RunRecord[]>([]);
  const [compareId, setCompareId] = useState<string | null>(null);

  useEffect(() => {
    window.loadr?.historyList().then(setHistory).catch(() => {});
  }, []);

  async function run() {
    setRunning(true);
    setLive(null);
    setResults(null);
    setError(null);
    try {
      const summary = await window.loadr.run(yaml, (line) => {
        const m = parseProgressLine(line);
        if (m) setLive(m);
      });
      const r = deriveResults(summary);
      setResults(r);
      const rec: RunRecord = { id: String(Date.now()), planName, at: Date.now(), passed: r.passed, results: r };
      setHistory(await window.loadr.historyAppend(rec));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRunning(false);
    }
  }

  const planRuns = runsForPlan(history, planName);
  const baseline = planRuns.find((r) => r.id === compareId)?.results;

  return (
    <div className="flex max-h-[40vh] flex-col overflow-y-auto border-t border-edge bg-coal p-3 text-sm">
      <div className="flex items-center gap-3">
        <Button variant="primary" icon={Play} onClick={run} disabled={running}>
          {running ? 'Running…' : 'Run'}
        </Button>
        {live && (
          <span className="font-mono text-xs text-smoke">
            {live.elapsedSecs}s · vus {live.vus} · rps {live.rps.toFixed(0)} ·
            p95 {live.p95Ms == null ? '—' : `${live.p95Ms}ms`} · failed {live.failed}
          </span>
        )}
      </div>

      {error && <p className="mt-2 rounded-lg border border-blood/40 bg-blood/10 px-2.5 py-1.5 text-xs text-flare">✗ {error}</p>}

      {results && (
        <div className="mt-3 rounded-xl border border-edge bg-panel p-3">
          <div className="flex items-center gap-3">
            <span className={`inline-flex items-center gap-1.5 text-sm font-semibold ${results.passed ? 'text-ok' : 'text-flare'}`}>
              {results.passed ? '✓ passed' : '✗ failed'}
            </span>
            <span className="text-xs text-mist">{results.durationSecs.toFixed(1)}s</span>
          </div>
          <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-2 text-xs sm:grid-cols-4">
            <Stat label="requests" value={results.totalRequests.toLocaleString()} />
            <Stat label="error rate" value={`${(results.errorRate * 100).toFixed(2)}%`} />
            <Stat label="p95" value={results.latency.p95 == null ? '—' : `${results.latency.p95.toFixed(1)}ms`} />
            <Stat label="p99" value={results.latency.p99 == null ? '—' : `${results.latency.p99.toFixed(1)}ms`} />
            <Stat label="iterations" value={results.iterations.toLocaleString()} />
            <Stat label="checks" value={`${results.checks.passed}✓ ${results.checks.failed}✗`} />
            <Stat label="thresholds" value={results.thresholdsPassed ? 'pass' : 'fail'} />
          </div>
        </div>
      )}

      {planRuns.length > 0 && (
        <div className="mt-3">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-mist">History · compare</p>
          <ul className="mt-1.5 space-y-1">
            {planRuns.slice(0, 8).map((r) => (
              <li key={r.id} className="flex items-center gap-2 text-xs">
                <input type="radio" name="cmp" checked={compareId === r.id} onChange={() => setCompareId(r.id)} aria-label={`compare run ${r.id}`} className="accent-ember" />
                <span className={r.passed ? 'text-ok' : 'text-flare'}>{r.passed ? '✓' : '✗'}</span>
                <span className="text-smoke">{new Date(r.at).toLocaleString()}</span>
                <span className="text-mist">p95 {r.results.latency.p95?.toFixed(0) ?? '—'}ms · err {(r.results.errorRate * 100).toFixed(1)}%</span>
              </li>
            ))}
          </ul>
          {results && baseline && (
            <table className="mt-2 w-full text-xs">
              <thead>
                <tr className="text-left text-mist"><th className="font-medium">metric</th><th className="font-medium">baseline</th><th className="font-medium">current</th><th className="font-medium">Δ</th></tr>
              </thead>
              <tbody>
                {compareResults(baseline, results).map((d) => (
                  <tr key={d.label} className="border-t border-edge/50">
                    <td className="py-0.5 text-smoke">{d.label}</td>
                    <td className="font-mono">{d.a ?? '—'}</td>
                    <td className="font-mono">{d.b ?? '—'}</td>
                    <td className={`font-mono ${deltaClass(d.deltaPct, d.lowerIsBetter)}`}>
                      {d.deltaPct == null ? '—' : `${d.deltaPct > 0 ? '+' : ''}${d.deltaPct.toFixed(1)}%`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-mist">{label}</div>
      <div className="font-mono text-ash">{value}</div>
    </div>
  );
}

function deltaClass(deltaPct: number | null, lowerIsBetter: boolean): string {
  if (deltaPct == null || deltaPct === 0) return 'text-mist';
  const better = lowerIsBetter ? deltaPct < 0 : deltaPct > 0;
  return better ? 'text-ok' : 'text-flare';
}
