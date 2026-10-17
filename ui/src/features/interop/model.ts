// Pure helpers for the import wizard.

import type { ImportBundle } from '$lib/api/generated/ImportBundle';
import type { ImportedItem } from '$lib/api/generated/ImportedItem';
import type { ImportReport } from '$lib/api/generated/ImportReport';
import type { Protocol } from '$lib/api/generated/Protocol';
import { methodOf, protocolOf } from '$lib/ui/badges';

export interface PreviewRow {
  depth: number;
  kind: 'collection' | 'folder' | 'request';
  name: string;
  method?: string;
  protocol?: Protocol | null;
}

export interface BundleSummary {
  collections: number;
  folders: number;
  requests: number;
  environments: number;
  cookies: number;
}

function walk(
  items: ImportedItem[],
  visit: (item: ImportedItem, depth: number) => void,
  depth = 1,
) {
  for (const item of items) {
    visit(item, depth);
    if (item.kind === 'folder') walk(item.children, visit, depth + 1);
  }
}

export function summarize(bundle: ImportBundle): BundleSummary {
  const s: BundleSummary = {
    collections: bundle.collections.length,
    folders: 0,
    requests: 0,
    environments: bundle.environments.length,
    cookies: bundle.cookies.length,
  };
  for (const c of bundle.collections) {
    s.environments += c.environments.length;
    walk(c.items, (item) => (item.kind === 'folder' ? s.folders++ : s.requests++));
  }
  return s;
}

/** Flat rows for the preview tree; stops after `limit` rows. */
export function previewRows(
  bundle: ImportBundle,
  limit = 500,
): { rows: PreviewRow[]; truncated: boolean } {
  const rows: PreviewRow[] = [];
  let truncated = false;
  for (const c of bundle.collections) {
    rows.push({ depth: 0, kind: 'collection', name: c.name });
    walk(c.items, (item, depth) => {
      if (rows.length >= limit) {
        truncated = true;
        return;
      }
      rows.push(
        item.kind === 'folder'
          ? { depth, kind: 'folder', name: item.name }
          : {
              depth,
              kind: 'request',
              name: item.name,
              method: methodOf(item.data as unknown as { kind: string }),
              protocol: protocolOf({ ...item.data, kind: item.kind } as unknown as {
                kind: string;
              }),
            },
      );
    });
  }
  return { rows, truncated };
}

/** Plain text of a report, for copying and saving. */
export function reportText(report: ImportReport, title: string): string {
  const lines = [title, ''];
  for (const e of report.entries) {
    lines.push(`[${e.kind.toUpperCase()}] ${e.path || '—'}: ${e.message}`);
    if (e.raw !== null && e.raw !== undefined) lines.push(`    ${JSON.stringify(e.raw)}`);
  }
  return lines.join('\n') + '\n';
}
