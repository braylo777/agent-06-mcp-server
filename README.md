# agent-06-mcp-server

A remote **Model Context Protocol (MCP) server** that any MCP client (Claude Desktop, Cursor, an agent framework) can connect to and call tools on. It runs serverless on Cloudflare Workers. Project 6 of a 10-part agentic AI portfolio.

**Live demo:** https://agent-06-mcp-server.braylo777.workers.dev

## What it is
An MCP server exposing 4 tools (`calculator`, `word_stats`, `search_notes`, `utc_time`) and one resource (`notes://all`) at `/mcp` over Streamable HTTP. The demo page doubles as a small inspector: pick a JSON-RPC message and send it.

Connect a client to `https://agent-06-mcp-server.braylo777.workers.dev/mcp`. For stdio-only clients, use `npx mcp-remote <url>`.

## How it works
```
MCP client ──JSON-RPC 2.0 POST /mcp──▶ Worker
   initialize ─▶ capabilities {tools, resources}
   tools/list ─▶ JSON-schema tool catalog
   tools/call ─▶ run tool ─▶ {content:[{type:text}], isError}
```
- `src/mcp.js`: protocol handler with no SDK, so every message type is visible: initialize, ping, tools/list, tools/call, resources/list, resources/read and notifications (202, no body).
- Stateless: no sessions are needed, so it scales on Workers with zero storage.
- Tool failures return `isError: true` per the spec instead of protocol errors. CORS is enabled for browser clients, with 5 requests/min per IP.
- No LLM calls: the server only provides tools, so the demo costs nothing to run.

## Run it
```bash
npm install
npm test            # offline unit tests (Node 20+), LLM is mocked
npx wrangler dev    # local; Workers AI runs via your Cloudflare login
npx wrangler deploy # free tier, no paid API keys
```

## Stack
Cloudflare Workers · MCP 2025-06-18 (Streamable HTTP) · JSON-RPC 2.0 · vanilla JS · node:test (no LLM needed)

## License
MIT
