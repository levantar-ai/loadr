/// <reference types="vite/client" />

import type { LoadrApi } from '../preload';

declare global {
  interface Window {
    loadr: LoadrApi;
  }
}

export {};
