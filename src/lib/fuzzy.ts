/** JetBrains-style matching: substring > word start (_, ., uppercase) > scattered letters in order. Higher score comes first */
export function fuzzyScore(query: string, text: string): number | null {
  if (!query) return 0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t === q) return 1000;
  if (t.startsWith(q)) return 800 - t.length;
  const at = t.indexOf(q);
  if (at >= 0) {
    const wordStart = at === 0 || /[_.\s-]/.test(t[at - 1]) || text[at] !== t[at];
    return (wordStart ? 600 : 400) - at - t.length * 0.1;
  }
  // Scattered letters (e.g. "uph" → user_payment_history)
  let score = 200;
  let ti = 0;
  let prev = -2;
  for (const ch of q) {
    if (ch === " ") continue;
    let found = -1;
    // Find word start positions first
    for (let k = ti; k < t.length; k++) {
      if (t[k] === ch && (k === 0 || /[_.\s-]/.test(t[k - 1]) || text[k] !== t[k])) {
        found = k;
        break;
      }
    }
    if (found < 0) found = t.indexOf(ch, ti);
    if (found < 0) return null;
    score += found === prev + 1 ? 5 : -Math.min(found - ti, 10);
    prev = found;
    ti = found + 1;
  }
  return score - t.length * 0.1;
}
