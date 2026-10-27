import { calc } from "./calc.js";

// Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, stateless). Spec: modelcontextprotocol.io
export const PROTOCOL = "2025-06-18";
const NOTES = [
  "MCP servers expose tools, resources and prompts to any MCP client.",
  "Cloudflare Workers can host remote MCP servers on the free tier.",
  "JSON-RPC 2.0 messages carry a method, params and an id.",
];
export const TOOLS = [
  { name: "calculator", description: "Evaluate arithmetic like (2+3)*4", inputSchema: { type: "object", properties: { expression: { type: "string" } }, required: ["expression"] },
    run: ({ expression }) => String(calc(expression)) },
  { name: "word_stats", description: "Count words, characters and sentences in text", inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
    run: ({ text }) => JSON.stringify({ words: (text.match(/\S+/g) || []).length, chars: text.length, sentences: (text.match(/[.!?]+(\s|$)/g) || []).length }) },
  { name: "search_notes", description: "Keyword search over the server's notes", inputSchema: { type: "object", properties: { query: { type: "string" } }, required: ["query"] },
    run: ({ query }) => { const q = query.toLowerCase().split(/\W+/).filter(Boolean); const hits = NOTES.filter((n) => q.some((w) => n.toLowerCase().includes(w))); return hits.length ? hits.join("\n") : "No matching notes."; } },
  { name: "utc_time", description: "Current UTC time", inputSchema: { type: "object", properties: {} }, run: () => new Date().toISOString() },
];
const RESOURCES = [{ uri: "notes://all", name: "All notes", mimeType: "text/plain" }];

export function handle(msg) {
  const { id, method, params = {} } = msg || {};
  const ok = (result) => ({ jsonrpc: "2.0", id, result });
  const err = (code, message) => ({ jsonrpc: "2.0", id: id ?? null, error: { code, message } });
  if (!msg || msg.jsonrpc !== "2.0" || typeof method !== "string") return err(-32600, "Invalid Request");
  if (id === undefined) return null; // notification, e.g. notifications/initialized
  switch (method) {
    case "initialize": return ok({ protocolVersion: PROTOCOL, capabilities: { tools: {}, resources: {} }, serverInfo: { name: "agent-06-mcp-server", version: "1.0.0" } });
    case "ping": return ok({});
    case "tools/list": return ok({ tools: TOOLS.map(({ run, ...t }) => t) });
    case "tools/call": {
      const t = TOOLS.find((x) => x.name === params.name);
      if (!t) return err(-32602, `Unknown tool: ${params.name}`);
      try { return ok({ content: [{ type: "text", text: t.run(params.arguments || {}) }], isError: false }); }
      catch (e) { return ok({ content: [{ type: "text", text: `Error: ${e.message}` }], isError: true }); }
    }
    case "resources/list": return ok({ resources: RESOURCES });
    case "resources/read": return params.uri === "notes://all" ? ok({ contents: [{ uri: "notes://all", mimeType: "text/plain", text: NOTES.join("\n") }] }) : err(-32602, "Unknown resource");
    default: return err(-32601, `Method not found: ${method}`);
  }
}
