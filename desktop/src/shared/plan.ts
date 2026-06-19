// The round-trip core: YAML <-> plan model. This is the make-or-break of the
// GUI — every edit the UI makes round-trips through here, and the result must
// be YAML that `loadr validate` accepts. js-yaml preserves all keys, so parsing
// then serializing is lossless at the data level; the GUI owns formatting.

import yaml from 'js-yaml';

import type { Plan } from './types';

export class PlanParseError extends Error {}

/** Parse loadr YAML into a plan model. Throws PlanParseError on invalid YAML. */
export function parsePlan(source: string): Plan {
  let doc: unknown;
  try {
    doc = yaml.load(source);
  } catch (e) {
    throw new PlanParseError(e instanceof Error ? e.message : String(e));
  }
  if (doc === null || doc === undefined) return {};
  if (typeof doc !== 'object' || Array.isArray(doc)) {
    throw new PlanParseError('top-level of a loadr plan must be a mapping');
  }
  return doc as Plan;
}

/**
 * Serialize a plan model back to YAML. Block style, no anchors/refs, no line
 * wrapping (so long URLs/bodies stay on one line), insertion order preserved.
 * The output is what gets handed to `loadr validate` / written to disk.
 */
export function serializePlan(plan: Plan): string {
  return yaml.dump(plan, {
    indent: 2,
    lineWidth: -1,
    noRefs: true,
    sortKeys: false,
    quotingType: '"',
  });
}

/**
 * Round-trip a YAML document through the model and back. Used by tests and by
 * the "format/normalize" action in the editor.
 */
export function normalize(source: string): string {
  return serializePlan(parsePlan(source));
}

/**
 * Structural deep-equality ignoring object key order (YAML mappings are
 * unordered). Used to prove a round-trip preserved the plan's data.
 */
export function deepEqualUnordered(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => deepEqualUnordered(v, b[i]));
  }
  if (a && b && typeof a === 'object') {
    const ak = Object.keys(a as object);
    const bk = Object.keys(b as object);
    if (ak.length !== bk.length) return false;
    return ak.every(
      (k) =>
        Object.prototype.hasOwnProperty.call(b, k) &&
        deepEqualUnordered((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]),
    );
  }
  return false;
}
