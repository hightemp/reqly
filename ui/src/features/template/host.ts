// Template host backed by the environment store.

import type { EnvironmentStore } from '$lib/stores/environments.svelte';
import type { TemplateHostFactory } from '$lib/template/host';
import { tagEditor } from './tagEditorState.svelte';

export function createTemplateHostFactory(envs: EnvironmentStore): TemplateHostFactory {
  return (nodeId) => ({
    async load() {
      const [variables, catalog] = await Promise.all([
        envs.variables(nodeId),
        envs
          .loadCatalog()
          .catch(() => ({ functions: [], faker: [], fakerLocales: [], dynamicVariables: [] })),
      ]);
      return {
        variables,
        functions: catalog.functions,
        faker: catalog.faker,
        dynamicVariables: catalog.dynamicVariables,
      };
    },
    onChange: (listener) => envs.onChange(listener),
    preview: (text) => envs.preview(text, nodeId),
    editTag: (text, apply) => tagEditor.open({ text, nodeId, apply }),
  });
}
