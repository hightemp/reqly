// What template-aware editors need from the app. Provided through Svelte
// context so low-level UI components stay independent of services.

import { getContext, setContext } from 'svelte';
import type { FakerInfo } from '$lib/api/generated/FakerInfo';
import type { FunctionInfo } from '$lib/api/generated/FunctionInfo';
import type { VariableInfo } from '$lib/api/generated/VariableInfo';

export interface TemplateData {
  variables: VariableInfo[];
  functions: FunctionInfo[];
  faker: FakerInfo[];
}

export interface TemplateHost {
  load(): Promise<TemplateData>;
  /** Called when variables may have changed; returns an unsubscribe function. */
  onChange(listener: () => void): () => void;
  preview(text: string): Promise<string>;
  /** Opens the tag editor for `text`; `apply` replaces it in the document. */
  editTag?(text: string, apply: (next: string) => void): void;
}

export type TemplateHostFactory = (nodeId: string | null) => TemplateHost;

const KEY = Symbol('template-host');

export function provideTemplateHost(factory: TemplateHostFactory) {
  setContext(KEY, factory);
}

export function templateHostFactory(): TemplateHostFactory | undefined {
  return getContext<TemplateHostFactory | undefined>(KEY);
}

/** Function names that need no catalog entry (Postman dynamic variables). */
export function isKnownFunction(name: string, functions: FunctionInfo[]): boolean {
  return (
    functions.some((f) => f.name === name) ||
    ['guid', 'timestamp', 'isoTimestamp'].includes(name) ||
    /^random[A-Z]/.test(name) ||
    name === 'randomUUID'
  );
}
