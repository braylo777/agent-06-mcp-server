import { test } from "node:test";
import assert from "node:assert/strict";
import { handle, PROTOCOL } from "../src/mcp.js";

const rpc = (method, params, id = 1) => handle({ jsonrpc: "2.0", id, method, params });
test("initialize", () => assert.equal(rpc("initialize", {}).result.protocolVersion, PROTOCOL));
test("tools/list hides implementations", () => { const t = rpc("tools/list").result.tools; assert.equal(t.length, 4); assert.ok(!("run" in t[0])); });
test("tools/call calculator", () => assert.equal(rpc("tools/call", { name: "calculator", arguments: { expression: "(2+3)*4" } }).result.content[0].text, "20"));
test("tool errors are isError, not crashes", () => assert.equal(rpc("tools/call", { name: "calculator", arguments: { expression: "x" } }).result.isError, true));
test("notifications get no reply; unknown method errors", () => {
  assert.equal(handle({ jsonrpc: "2.0", method: "notifications/initialized" }), null);
  assert.equal(rpc("nope").error.code, -32601);
});
