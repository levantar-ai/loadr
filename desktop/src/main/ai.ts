// AI plan generation: orchestrate model call → extract YAML → `loadr validate`
// → one repair round, and the concrete Anthropic Messages API call. The
// orchestration takes its model and validator as injected functions (a seam),
// so it's unit-testable without network or electron. No electron imports here.

import { buildRepairMessage, buildUserMessage, extractYaml, SYSTEM_PROMPT, type GenerateInput } from '../shared/ai';

export interface ChatMessage { role: 'user' | 'assistant'; content: string }
export type ChatFn = (messages: ChatMessage[]) => Promise<string>;

export interface Diagnostic { severity: string; message: string }
export type ValidateFn = (yaml: string) => Promise<{ ok: boolean; diagnostics: Diagnostic[] }>;

export interface PlanResult {
  yaml: string;
  valid: boolean;
  repaired: boolean;
  diagnostics: Diagnostic[];
}

/**
 * Generate a plan: one model call, then if `loadr validate` rejects it, ONE
 * repair round feeding the errors back. Returns the best plan with its
 * validation state (the caller surfaces remaining diagnostics).
 */
export async function generatePlan(input: GenerateInput, chat: ChatFn, validate: ValidateFn): Promise<PlanResult> {
  const messages: ChatMessage[] = [{ role: 'user', content: buildUserMessage(input) }];
  const first = await chat(messages);
  let yaml = extractYaml(first);
  if (!yaml) throw new Error('The model did not return a YAML plan — try rephrasing the request.');

  let v = await validate(yaml);
  if (v.ok) return { yaml, valid: true, repaired: false, diagnostics: [] };

  const errors = v.diagnostics.filter((d) => d.severity === 'error');
  messages.push({ role: 'assistant', content: first });
  messages.push({ role: 'user', content: buildRepairMessage(yaml, errors) });
  const second = await chat(messages);
  const fixed = extractYaml(second);
  if (fixed) {
    yaml = fixed;
    v = await validate(yaml);
  }
  return { yaml, valid: v.ok, repaired: true, diagnostics: v.diagnostics };
}

const ANTHROPIC_URL = 'https://api.anthropic.com/v1/messages';

/** One Anthropic Messages API call; returns the concatenated text blocks. */
export async function anthropicChat(
  apiKey: string,
  model: string,
  messages: ChatMessage[],
): Promise<string> {
  const res = await fetch(ANTHROPIC_URL, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-api-key': apiKey,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({ model, max_tokens: 4096, system: SYSTEM_PROMPT, messages }),
  });
  if (!res.ok) {
    let detail = '';
    try {
      detail = (await res.json())?.error?.message ?? '';
    } catch {
      /* non-JSON error body */
    }
    throw new Error(`Anthropic API ${res.status}${detail ? `: ${detail}` : ''}`);
  }
  const data = (await res.json()) as { content?: { type: string; text?: string }[] };
  return (data.content ?? []).filter((b) => b.type === 'text').map((b) => b.text ?? '').join('');
}
