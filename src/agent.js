import { chat, text, parseJSON } from "./core.js";
import { handle, TOOLS } from "./mcp.js";

// LLM-as-MCP-client: Llama 3.3 sees the server's tools/list, picks one, and the call goes through the same JSON-RPC handler.
export const llmTools = () => handle({ jsonrpc: "2.0", id: 0, method: "tools/list" }).result.tools
  .map((t) => ({ name: t.name, description: t.description, parameters: t.inputSchema }));

export function pickCall(r) {
  const tc = r?.tool_calls?.[0];
  if (tc) return { name: tc.name ?? tc.function?.name, arguments: typeof (tc.arguments ?? tc.function?.arguments) === "string" ? parseJSON(tc.arguments ?? tc.function.arguments, {}) : (tc.arguments ?? {}) };
  const j = parseJSON(text(r), null); // some models answer tool calls as plain JSON text
  return j && typeof j.name === "string" && TOOLS.some((t) => t.name === j.name) ? { name: j.name, arguments: j.arguments || j.parameters || {} } : null;
}

export async function runAgent(env, task) {
  const sys = "You can call tools from an MCP server. If a tool helps, call exactly one tool with correct arguments.";
  const first = await chat(env, [{ role: "system", content: sys }, { role: "user", content: task }], { tools: llmTools(), max_tokens: 200 });
  const call = pickCall(first);
  if (!call) return { tool: null, arguments: null, result: null, answer: text(first) };
  const res = handle({ jsonrpc: "2.0", id: 1, method: "tools/call", params: call });
  const result = res.error ? `Error: ${res.error.message}` : res.result.content[0].text;
  const answer = text(await chat(env, [{ role: "system", content: "Answer the user in one short sentence using the tool result." }, { role: "user", content: `Task: ${task}\nTool ${call.name} returned: ${result}` }], { max_tokens: 120 }));
  return { tool: call.name, arguments: call.arguments, result, isError: !!(res.error || res.result.isError), answer };
}
