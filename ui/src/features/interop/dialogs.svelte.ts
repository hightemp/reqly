// Import, export and code generation dialogs are rendered once by the shell.

import type { ExportRequest } from './ExportDialog.svelte';

class InteropDialogs {
  import = $state<{ collectionId: string | null } | null>(null);
  export = $state<ExportRequest | null>(null);
  codegen = $state<string | null>(null);

  openImport(collectionId: string | null = null) {
    this.import = { collectionId };
  }

  openExport(request: ExportRequest) {
    this.export = request;
  }

  openCodegen(requestId: string) {
    this.codegen = requestId;
  }
}

export const interopDialogs = new InteropDialogs();
