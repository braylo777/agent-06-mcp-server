// Live eval of the deployed MCP server.
//  protocol compliance: JSON-RPC 2.0 / MCP behaviour of /mcp (envelope, id echo, error codes, notifications, batches).
//  tool-call success: Llama 3.3 (Workers AI) acting as an MCP client picks a tool for a natural-language task;
//  pass = correct tool chosen AND the call ran without error AND the tool result contains the expected value.
import fs from "node:fs";
import { call, pct, ratio, has, sleep } from "./lib.mjs";
const BASE = process.env.BASE || "https://agent-06-mcp-server.braylo777.workers.dev";
const C = JSON.parse(fs.readFileSync(new URL("./cases.json", import.meta.url)));
const lat = { mcp: [], agent: [] }, P = [], A = [];
async function raw(req) { // /mcp may answer 202 with no body
  for (let a = 0; a < 6; a++) {
    await sleep(12500); const t = performance.now();
    const r = await fetch(BASE + "/mcp", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(req) });
    const ms = performance.now() - t, txt = await r.text();
    if (r.status === 429) { await sleep(30000); continue; }
    return { status: r.status, ms, body: txt ? JSON.parse(txt) : null };
  }
  throw new Error("rate limited repeatedly");
}
function check(c, r) {
  const b = r.body, env = (x) => x && x.jsonrpc === "2.0" && (("result" in x) !== ("error" in x));
  const idOk = !Array.isArray(c.req) && c.req.id !== undefined ? b?.id === c.req.id : true;
  const i = c.check.indexOf("="), k = i < 0 ? c.check : c.check.slice(0, i), v = i < 0 ? "" : c.check.slice(i + 1);
  const specific = {
    "result.protocolVersion": () => b?.result?.protocolVersion === v, result: () => !!b && "result" in b,
    tools4: () => b?.result?.tools?.length === 4 && b.result.tools.every((t) => t.name && t.inputSchema && !("run" in t)),
    text: () => b?.result?.content?.[0]?.text === v && b.result.isError === false, isError: () => b?.result?.isError === true,
    error: () => b?.error?.code === Number(v), status: () => r.status === Number(v) && b === null,
    batch2: () => Array.isArray(b) && b.length === 2 && b.every(env) && b.map((x) => x.id).join() === "9,10",
    resource: () => b?.result?.contents?.[0]?.uri === "notes://all",
  }[k]();
  return { specific, envelope: k === "status" || Array.isArray(b) || !!env(b), idOk };
}
for (const c of C.protocol) { const r = await raw(c.req); if (r.status < 300) lat.mcp.push(r.ms); const k = check(c, r);
  P.push({ name: c.name, status: r.status, ...k, pass: k.specific && k.envelope && k.idOk, response: r.body }); console.log("proto", c.name, P.at(-1).pass); }
for (const c of C.agent) { const r = await call(BASE + "/api/agent", { method: "POST", body: JSON.stringify({ task: c.task }) }); if (r.status === 200) lat.agent.push(r.ms);
  const b = r.body || {}; const toolOk = b.tool === c.tool, ran = toolOk && b.isError === false, resOk = ran && has(String(b.result).replace(/\s/g, ""), [c.expect.replace(/\s/g, "")]);
  A.push({ ...c, chosen: b.tool, arguments: b.arguments, result: b.result, answer: b.answer, error: b.error, tool_selected: toolOk, pass: resOk }); console.log("agent", c.tool, b.tool, resOk); }
const out = { project: "agent-06-mcp-server", base: BASE, model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast", protocol: "MCP 2025-06-18, JSON-RPC 2.0 over Streamable HTTP (stateless)", run_at: new Date().toISOString(),
  n_cases: P.length + A.length,
  metrics: { protocol_compliance: ratio(P.filter((x) => x.pass).length, P.length), tool_call_success: ratio(A.filter((x) => x.pass).length, A.length),
    tool_selection_accuracy: ratio(A.filter((x) => x.tool_selected).length, A.length),
    latency_ms: { mcp: { n: lat.mcp.length, p50: pct(lat.mcp, 50), p95: pct(lat.mcp, 95) }, agent: { n: lat.agent.length, p50: pct(lat.agent, 50), p95: pct(lat.agent, 95) }, note: "end-to-end from the eval client" } },
  cases: { protocol: P, agent: A } };
fs.writeFileSync(new URL("./results.json", import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(out.metrics, null, 2));
