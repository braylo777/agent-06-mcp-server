"""LLM-as-MCP-client: Llama 3.3 sees the server's tools/list, picks one, and the call goes through the same JSON-RPC handler."""
import json, re
from .mcp import TOOLS, handle

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
_dumps = lambda o: json.dumps(o, separators=(",", ":"), ensure_ascii=False)


def text(r):
    v = r.get("response") if isinstance(r, dict) else None
    return v if isinstance(v, str) else _dumps(v if v is not None else r)


def parse_json(s, fallback=None):
    m = re.search(r"[\[{][\s\S]*[\]}]", str(s))
    try:
        return json.loads(m.group(0)) if m else fallback
    except ValueError:
        return fallback


def cap(s, n=500):
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def llm_tools():
    return [{"name": t["name"], "description": t["description"], "parameters": t["inputSchema"]} for t in handle({"jsonrpc": "2.0", "id": 0, "method": "tools/list"})["result"]["tools"]]


def pick_call(r):
    tcs = (r or {}).get("tool_calls")
    tc = tcs[0] if isinstance(tcs, list) and tcs else None
    if tc:
        fn = tc.get("function") or {}
        name = tc["name"] if tc.get("name") is not None else fn.get("name")
        a = tc["arguments"] if tc.get("arguments") is not None else fn.get("arguments")
        return {"name": name, "arguments": (parse_json(a, {}) if isinstance(a, str) else (a if a is not None else {}))}
    j = parse_json(text(r), None)
    if isinstance(j, dict) and isinstance(j.get("name"), str) and any(t["name"] == j["name"] for t in TOOLS):
        return {"name": j["name"], "arguments": j.get("arguments") or j.get("parameters") or {}}
    return None


def run_agent(ai, task):
    """ai(model, input) -> dict is injected so tests run offline."""
    sys = "You can call tools from an MCP server. If a tool helps, call exactly one tool with correct arguments."
    first = ai(MODEL, {"messages": [{"role": "system", "content": sys}, {"role": "user", "content": task}], "max_tokens": 200, "tools": llm_tools()})
    call = pick_call(first)
    if not call:
        return {"tool": None, "arguments": None, "result": None, "answer": text(first)}
    res = handle({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": call})
    result = f"Error: {res['error']['message']}" if "error" in res else res["result"]["content"][0]["text"]
    msgs = [{"role": "system", "content": "Answer the user in one short sentence using the tool result."}, {"role": "user", "content": f"Task: {task}\nTool {call['name']} returned: {result}"}]
    answer = text(ai(MODEL, {"messages": msgs, "max_tokens": 120}))
    return {"tool": call["name"], "arguments": call["arguments"], "result": result, "isError": bool("error" in res or res["result"]["isError"]), "answer": answer}
