"""Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, stateless). Spec: modelcontextprotocol.io"""
import json, re
from datetime import datetime, timezone

from .core import CalcError, calc, js_num

PROTOCOL = "2025-06-18"
NOTES = [
    "MCP servers expose tools, resources and prompts to any MCP client.",
    "Cloudflare Workers can host remote MCP servers on the free tier.",
    "JSON-RPC 2.0 messages carry a method, params and an id.",
]
dumps = lambda o: json.dumps(o, separators=(",", ":"), ensure_ascii=False)


class ToolError(Exception):
    pass


def _utf16_len(s):
    return len(s.encode("utf-16-le")) // 2


def _need_str(args, key, method):
    v = args.get(key)
    if v is None:
        raise ToolError(f"Cannot read properties of undefined (reading '{method}')")
    if not isinstance(v, str):
        raise ToolError(f"{key}.{method} is not a function")
    return v


def _calculator(args):
    return js_num(calc(args.get("expression") if args.get("expression") is not None else "undefined"))


def _word_stats(args):
    text = _need_str(args, "text", "match")
    return dumps({"words": len(re.findall(r"\S+", text)), "chars": _utf16_len(text), "sentences": len(re.findall(r"[.!?]+(?:\s|$)", text))})


def _search_notes(args):
    q = [w for w in re.split(r"[^A-Za-z0-9_]+", _need_str(args, "query", "toLowerCase").lower()) if w]
    hits = [n for n in NOTES if any(w in n.lower() for w in q)]
    return "\n".join(hits) if hits else "No matching notes."


def _utc_time(_a):
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


TOOLS = [
    {"name": "calculator", "description": "Evaluate arithmetic like (2+3)*4", "inputSchema": {"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}, "run": _calculator},
    {"name": "word_stats", "description": "Count words, characters and sentences in text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}, "run": _word_stats},
    {"name": "search_notes", "description": "Keyword search over the server's notes", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}, "run": _search_notes},
    {"name": "utc_time", "description": "Current UTC time", "inputSchema": {"type": "object", "properties": {}}, "run": _utc_time},
]
RESOURCES = [{"uri": "notes://all", "name": "All notes", "mimeType": "text/plain"}]
def _js_key(d, k):
    """JS template-literal rendering of d[k]: missing -> undefined, null -> null."""
    if k not in d:
        return "undefined"
    v = d[k]
    return "null" if v is None else (str(v).lower() if isinstance(v, bool) else v if isinstance(v, str) else dumps(v))


def handle(msg):
    """Returns the JSON-RPC response dict, or None for a notification."""
    d = msg if isinstance(msg, dict) else {}
    rid, method = d.get("id"), d.get("method")
    params = d["params"] if isinstance(d.get("params"), dict) else {}
    ok = lambda result: {"jsonrpc": "2.0", "id": rid, "result": result}
    err = lambda code, message: {"jsonrpc": "2.0", "id": rid, "error": {"code": code, "message": message}}
    if not d or d.get("jsonrpc") != "2.0" or not isinstance(method, str):
        return err(-32600, "Invalid Request")
    if "id" not in d:
        return None  # notification, e.g. notifications/initialized
    if method == "initialize":
        return ok({"protocolVersion": PROTOCOL, "capabilities": {"tools": {}, "resources": {}}, "serverInfo": {"name": "agent-06-mcp-server", "version": "1.1.0"}})
    if method == "ping":
        return ok({})
    if method == "tools/list":
        return ok({"tools": [{k: v for k, v in t.items() if k != "run"} for t in TOOLS]})
    if method == "tools/call":
        t = next((x for x in TOOLS if x["name"] == params.get("name")), None)
        if not t:
            return err(-32602, f"Unknown tool: {_js_key(params, 'name')}")
        try:
            args = params.get("arguments")
            return ok({"content": [{"type": "text", "text": t["run"](args if isinstance(args, dict) and args else {})}], "isError": False})
        except (ToolError, CalcError) as e:
            return ok({"content": [{"type": "text", "text": f"Error: {e}"}], "isError": True})
    if method == "resources/list":
        return ok({"resources": RESOURCES})
    if method == "resources/read":
        return ok({"contents": [{"uri": "notes://all", "mimeType": "text/plain", "text": "\n".join(NOTES)}]}) if params.get("uri") == "notes://all" else err(-32602, "Unknown resource")
    return err(-32601, f"Method not found: {method}")


def handle_http(body):
    """Body already parsed from JSON. Returns (status, json_or_None)."""
    if isinstance(body, list):
        out = [r for r in (handle(m) for m in body) if r is not None]
    else:
        out = handle(body)
    if out is None or (isinstance(out, list) and not out):
        return 202, None
    return 200, out
