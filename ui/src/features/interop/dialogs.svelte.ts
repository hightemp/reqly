// Import, export and code generation dialogs are rendered once by the shell.

import type { ExportRequest } from './ExportDialog.svelte';

class InteropDialogs {
  import = $state<{
    collectionId: string | null;
    folderId?: string | null;
    source?: 'file' | 'text' | 'url';
  } | null>(null);
  export = $state<ExportRequest | null>(null);
  codegen = $state<string | null>(null);

  openImport(
    collectionId: string | null = null,
    source?: 'file' | 'text' | 'url',
    folderId: string | null = null,
  ) {
    this.import = { collectionId, source, folderId };
  }

  openExport(request: ExportRequest) {
    this.export = request;
  }

  openCodegen(requestId: string) {
    this.codegen = requestId;
  }
}

export const interopDialogs = new InteropDialogs();
