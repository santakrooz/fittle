/** Subsequence match with a score (higher is better) and matched indices, or null. */
export function fuzzy(query: string, text: string): { score: number; hits: number[] } | null {
  const q = query.trim().toLowerCase();
  if (!q) return { score: 0, hits: [] };
  const t = text.toLowerCase();
  const start = t.indexOf(q);
  if (start >= 0) {
    // Contiguous matches win; earlier and word-start matches win more.
    const wordStart = start === 0 || /[\s_\-.]/.test(t[start - 1]);
    return { score: 1000 - start + (wordStart ? 200 : 0), hits: [...q].map((_, i) => start + i) };
  }
  const hits: number[] = [];
  let i = 0;
  for (const ch of q) {
    i = t.indexOf(ch, i);
    if (i < 0) return null;
    hits.push(i++);
  }
  const spread = hits[hits.length - 1] - hits[0];
  return { score: 500 - spread - hits[0], hits };
}
