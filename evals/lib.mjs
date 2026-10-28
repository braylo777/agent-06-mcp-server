// Shared eval helpers: throttled live calls (demo is rate limited to 5 req/min/IP) and nearest-rank percentiles.
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export const GAP = Number(process.env.EVAL_GAP_MS || 12500);
export async function call(url, init, gap = GAP) {
  for (let attempt = 0; attempt < 6; attempt++) {
    await sleep(gap);
    const t = performance.now();
    const r = await fetch(url, { ...init, headers: { "content-type": "application/json" } });
    const ms = performance.now() - t;
    if (r.status === 429) { await sleep(30000); continue; }
    return { status: r.status, ms, body: await r.json().catch(() => null) };
  }
  throw new Error("rate limited repeatedly");
}
export function pct(xs, p) { const s = [...xs].sort((a, b) => a - b); return s.length ? +s[Math.max(0, Math.ceil((p / 100) * s.length) - 1)].toFixed(0) : null; }
export const ratio = (k, n) => ({ correct: k, n, rate: n ? +(k / n).toFixed(4) : null });
export const has = (s, keys) => keys.some((k) => String(s ?? "").toLowerCase().includes(k.toLowerCase()));
