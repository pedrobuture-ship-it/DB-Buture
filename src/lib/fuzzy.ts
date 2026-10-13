/**
 * Subsequence fuzzy matcher. Returns null when `query` does not match, otherwise a score where
 * higher is better (prefix and word-boundary hits and consecutive runs score more).
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().trim();
  if (q === "") return 0;
  const t = text.toLowerCase();
  let score = 0;
  let ti = 0;
  let run = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found === -1) return null;
    const boundary = found === 0 || /[^a-z0-9]/.test(t[found - 1] ?? "");
    run = found === ti ? run + 1 : 1;
    score += 1 + run * 2 + (boundary ? 4 : 0) - Math.min(found - ti, 6) * 0.3;
    ti = found + 1;
  }
  if (t.startsWith(q)) score += 12;
  if (t === q) score += 20;
  // shorter haystacks are more specific
  return score - t.length * 0.05;
}

export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  if (query.trim() === "") return items;
  return items
    .map((item) => ({ item, score: fuzzyScore(query, text(item)) }))
    .filter((x): x is { item: T; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item);
}
