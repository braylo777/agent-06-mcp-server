import { test } from "node:test";
import assert from "node:assert/strict";
import { handle } from "../src/mcp.js";
import { calc } from "../src/calc.js";
import { pickCall, llmTools, runAgent } from "../src/agent.js";
import worker from "../src/index.js";

const rpc = (method, params, id = 7) => handle({ jsonrpc: "2.0", id, method, params });
test("calc precedence, unary minus, modulo, decimals", () => {
  assert.equal(calc("2+3*4"), 14); assert.equal(calc("-(2+3)"), -5); assert.equal(calc("10%4"), 2); assert.equal(calc("1.5*2"), 3);
});
test("calc rejects junk and unbalanced parens", () => { for (const e of ["2+", "(1+2", "alert(1)", ""]) assert.throws(() => calc(e)); });
test("response echoes request id", () => assert.equal(rpc("ping", {}, "abc").id, "abc"));
test("invalid request -> -32600", () => assert.equal(handle({ id: 1, method: "ping" }).error.code, -32600));
test("unknown tool -> -32602", () => assert.equal(rpc("tools/call", { name: "nope" }).error.code, -32602));
test("word_stats counts", () => assert.deepEqual(JSON.parse(rpc("tools/call", { name: "word_stats", arguments: { text: "Hi there. Bye!" } }).result.content[0].text), { words: 3, chars: 14, sentences: 2 }));
test("search_notes hit and miss", () => {
  assert.match(rpc("tools/call", { name: "search_notes", arguments: { query: "json-rpc" } }).result.content[0].text, /JSON-RPC/);
  assert.equal(rpc("tools/call", { name: "search_notes", arguments: { query: "zebra" } }).result.content[0].text, "No matching notes.");
});
test("resources list/read and unknown resource", () => {
  assert.equal(rpc("resources/list").result.resources[0].uri, "notes://all");
  assert.match(rpc("resources/read", { uri: "notes://all" }).result.contents[0].text, /MCP/);
  assert.equal(rpc("resources/read", { uri: "x://y" }).error.code, -32602);
});
test("llmTools maps inputSchema to parameters", () => { const t = llmTools(); assert.equal(t.length, 4); assert.ok(t.every((x) => x.parameters?.type === "object")); });
test("pickCall reads native tool_calls, string args, and JSON text", () => {
  assert.deepEqual(pickCall({ tool_calls: [{ name: "calculator", arguments: { expression: "1+1" } }] }), { name: "calculator", arguments: { expression: "1+1" } });
  assert.deepEqual(pickCall({ tool_calls: [{ name: "calculator", arguments: '{"expression":"2"}' }] }).arguments, { expression: "2" });
  assert.equal(pickCall({ response: '{"name":"utc_time","parameters":{}}' }).name, "utc_time");
  assert.equal(pickCall({ response: "hello" }), null);
});
test("runAgent routes the model's call through MCP", async () => {
  let n = 0; const env = { AI: { run: async () => (++n === 1 ? { tool_calls: [{ name: "calculator", arguments: { expression: "17*23+4" } }] } : { response: "It is 395." }) } };
  const r = await runAgent(env, "17*23+4?"); assert.equal(r.tool, "calculator"); assert.equal(r.result, "395"); assert.equal(r.isError, false);
});
test("HTTP: batch, notification 202, GET 405, /api/agent validation", async () => {
  const post = (path, b) => worker.fetch(new Request("https://x" + path, { method: "POST", body: JSON.stringify(b) }), {});
  const batch = await (await post("/mcp", [{ jsonrpc: "2.0", id: 1, method: "ping" }, { jsonrpc: "2.0", method: "notifications/initialized" }])).json();
  assert.equal(batch.length, 1);
  assert.equal((await post("/mcp", { jsonrpc: "2.0", method: "notifications/initialized" })).status, 202);
  assert.equal((await worker.fetch(new Request("https://x/mcp"), {})).status, 405);
  assert.equal((await post("/api/agent", {})).status, 400);
});
