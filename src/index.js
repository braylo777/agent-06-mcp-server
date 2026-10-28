import { json, limited, body, cap } from "./core.js";
import { handle } from "./mcp.js";
import { runAgent } from "./agent.js";
import page from "./page.html";

const CORS = { "access-control-allow-origin": "*", "access-control-allow-headers": "content-type, mcp-session-id, mcp-protocol-version", "access-control-allow-methods": "POST, GET, OPTIONS" };

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (url.pathname === "/mcp") {
      if (req.method === "OPTIONS") return new Response(null, { headers: CORS });
      if (req.method !== "POST") return new Response("Use POST (Streamable HTTP, stateless)", { status: 405, headers: CORS });
      if (await limited(req, env)) return json({ jsonrpc: "2.0", id: null, error: { code: -32000, message: "Rate limited" } }, 429);
      const msg = await body(req);
      const out = Array.isArray(msg) ? msg.map(handle).filter(Boolean) : handle(msg);
      if (out === null || (Array.isArray(out) && !out.length)) return new Response(null, { status: 202, headers: CORS });
      return Response.json(out, { headers: CORS });
    }
    if (req.method === "POST" && url.pathname === "/api/agent") {
      if (await limited(req, env)) return json({ error: "Rate limit: try again in a minute." }, 429);
      const { task } = await body(req);
      if (!cap(task)) return json({ error: "task required (<=500 chars)" }, 400);
      try { return json(await runAgent(env, task)); } catch (e) { return json({ error: "Agent error: " + e.message }, 502); }
    }
    return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
  },
};
