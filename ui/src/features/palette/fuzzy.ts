// Small fuzzy matcher for the command palette: subsequence match scored by
// consecutive runs, word starts and match position.

export interface FuzzyMatch {
  score: number;
  positions: number[];
}

const isWordStart = (text: string, i: number) =>
  i === 0 ||
  /[\s/_.:-]/.test(text[i - 1]!) ||
  (/[a-z]/.test(text[i - 1]!) && /[A-Z]/.test(text[i]!));

export function fuzzyScore(query: string, text: string): FuzzyMatch | null {
  const q = query.trim().toLowerCase();
  if (!q) return { score: 0, positions: [] };
  const lower = text.toLowerCase();
  const positions: number[] = [];
  let score = 0;
  let from = 0;
  let run = 0;
  for (const ch of q) {
    if (ch === ' ') continue;
    // Prefer the next occurrence at a word start, otherwise the nearest one.
    let at = -1;
    for (let i = lower.indexOf(ch, from); i >= 0; i = lower.indexOf(ch, i + 1)) {
      if (at < 0) at = i;
      if (isWordStart(text, i) && (positions.length === 0 || i !== positions.at(-1)! + 1)) {
        if (positions.length && at === positions.at(-1)! + 1) break;
        at = i;
        break;
      }
      if (positions.length && i === positions.at(-1)! + 1) {
        at = i;
        break;
      }
    }
    if (at < 0) return null;
    const consecutive = positions.length > 0 && at === positions.at(-1)! + 1;
    run = consecutive ? run + 1 : 0;
    score += 10 + run * 15 + (isWordStart(text, at) ? 20 : 0) - Math.min(at, 20);
    positions.push(at);
    from = at + 1;
  }
  // Shorter texts with the same matches rank higher; exact prefix wins.
  score -= text.length / 4;
  if (lower.startsWith(q)) score += 50;
  return { score, positions };
}

export function rank<T>(
  query: string,
  items: T[],
  text: (item: T) => string,
  limit = 50,
): { item: T; match: FuzzyMatch }[] {
  const out: { item: T; match: FuzzyMatch; index: number }[] = [];
  items.forEach((item, index) => {
    const match = fuzzyScore(query, text(item));
    if (match) out.push({ item, match, index });
  });
  if (query.trim()) out.sort((a, b) => b.match.score - a.match.score || a.index - b.index);
  return out.slice(0, limit).map(({ item, match }) => ({ item, match }));
}
