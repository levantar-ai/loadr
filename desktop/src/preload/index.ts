// The only bridge between renderer and main. Exposes a small, typed, allow-
// listed API over contextBridge — no Node, no ipcRenderer, no fs in the renderer.

import { contextBridge, ipcRenderer } from 'electron';

export interface OpenedPlan {
  path: string;
  content: string;
}

export interface Diagnostic {
  severity: string;
  message: string;
  [k: string]: unknown;
}

export interface ValidateResult {
  ok: boolean;
  diagnostics: Diagnostic[];
  raw: string;
}

export interface LoadrApi {
  version(): Promise<string>;
  schema(): Promise<unknown>;
  validate(yamlText: string): Promise<ValidateResult>;
  openPlan(): Promise<OpenedPlan | null>;
  importPlan(): Promise<OpenedPlan | null>;
  readPlan(path: string): Promise<string>;
  savePlan(path: string | null, content: string): Promise<string | null>;
}

const api: LoadrApi = {
  version: () => ipcRenderer.invoke('loadr:version'),
  schema: () => ipcRenderer.invoke('loadr:schema'),
  validate: (yamlText) => ipcRenderer.invoke('loadr:validate', yamlText),
  openPlan: () => ipcRenderer.invoke('plan:open'),
  importPlan: () => ipcRenderer.invoke('plan:import'),
  readPlan: (path) => ipcRenderer.invoke('plan:read', path),
  savePlan: (path, content) => ipcRenderer.invoke('plan:save', path, content),
};

contextBridge.exposeInMainWorld('loadr', api);
