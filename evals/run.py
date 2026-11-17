"""Live eval of the deployed MCP server.
 protocol compliance: JSON-RPC 2.0 / MCP behaviour of /mcp (envelope, id echo, error codes, notifications, batches).
 tool-call success: Llama 3.3 (Workers AI) acting as an MCP client picks a tool for a natural-language task;
 pass = correct tool chosen AND the call ran without error AND the tool result contains the expected value.
Usage: BASE=https://... python3 evals/run.py"""
import json, math, os, re, time, urllib.error, urllib.request
from datetime import datetime, timezone
from pathlib import Path

here = Path(__file__).parent
BASE = os.environ.get("BASE", "https://agent-06-mcp-server.braylo777.workers.dev")
GAP = float(os.environ.get("EVAL_GAP_MS", "12500")) / 1000
C = json.loads((here / "cases.json").read_text())
lat = {"mcp": [], "agent": []}


def post(path, body):
    for _ in range(6):
        time.sleep(GAP)
        t = time.time()
        req = urllib.request.Request(BASE + path, json.dumps(body).encode(), {"content-type": "application/json"})
        try:
            r = urllib.request.urlopen(req, timeout=60); status = r.status; txt = r.read().decode()
        except urllib.error.HTTPError as e:
            status = e.code; txt = e.read().decode()
        ms = (time.time() - t) * 1000
        if status == 429:
            time.sleep(30); continue
        try:
            b = json.loads(txt) if txt else None
        except ValueError:
            b = None
        return {"status": status, "ms": ms, "body": b}
    raise RuntimeError("rate limited repeatedly")


pct = lambda xs, p: round(sorted(xs)[max(0, math.ceil(p / 100 * len(xs)) - 1)]) if xs else None
ratio = lambda k, n: {"correct": k, "n": n, "rate": round(k / n, 4) if n else None}
has = lambda s, keys: any(k.lower() in str(s or "").lower() for k in keys)
g = lambda o, *ks: (lambda f: f(f, o, list(ks)))(lambda f, o, ks: o if not ks else (f(f, o.get(ks[0]) if isinstance(o, dict) else (o[ks[0]] if isinstance(o, list) and isinstance(ks[0], int) and ks[0] < len(o) else None), ks[1:])))


def check(c, r):
    b = r["body"]
    env = lambda x: isinstance(x, dict) and x.get("jsonrpc") == "2.0" and (("result" in x) != ("error" in x))
    req = c["req"]
    id_ok = b.get("id") == req["id"] if (not isinstance(req, list) and isinstance(req, dict) and "id" in req and isinstance(b, dict)) else (not (not isinstance(req, list) and isinstance(req, dict) and "id" in req))
    chk = c["check"]
    k, _, v = chk.partition("=")
    specific = {
        "result.protocolVersion": lambda: g(b, "result", "protocolVersion") == v,
        "result": lambda: isinstance(b, dict) and "result" in b,
        "tools4": lambda: (lambda t: isinstance(t, list) and len(t) == 4 and all(x.get("name") and x.get("inputSchema") and "run" not in x for x in t))(g(b, "result", "tools")),
        "text": lambda: g(b, "result", "content", 0, "text") == v and g(b, "result", "isError") is False,
        "isError": lambda: g(b, "result", "isError") is True,
        "error": lambda: g(b, "error", "code") == int(v),
        "status": lambda: r["status"] == int(v) and b is None,
        "batch2": lambda: isinstance(b, list) and len(b) == 2 and all(env(x) for x in b) and ",".join(str(x.get("id")) for x in b) == "9,10",
        "resource": lambda: g(b, "result", "contents", 0, "uri") == "notes://all",
    }[k]()
    return {"specific": bool(specific), "envelope": k == "status" or isinstance(b, list) or env(b), "idOk": id_ok}


P, A = [], []
for c in C["protocol"]:
    r = post("/mcp", c["req"])
    if r["status"] < 300:
        lat["mcp"].append(r["ms"])
    k = check(c, r)
    P.append({"name": c["name"], "status": r["status"], **k, "pass": k["specific"] and k["envelope"] and k["idOk"], "response": r["body"]})
    print("proto", c["name"], P[-1]["pass"], flush=True)
for c in C["agent"]:
    r = post("/api/agent", {"task": c["task"]})
    if r["status"] == 200:
        lat["agent"].append(r["ms"])
    b = r["body"] or {}
    tool_ok = b.get("tool") == c["tool"]
    ran = tool_ok and b.get("isError") is False
    res_ok = ran and has(re.sub(r"\s", "", str(b.get("result"))), [re.sub(r"\s", "", c["expect"])])
    A.append({**c, "chosen": b.get("tool"), "arguments": b.get("arguments"), "result": b.get("result"), "answer": b.get("answer"), "error": b.get("error"), "tool_selected": tool_ok, "pass": bool(res_ok)})
    print("agent", c["tool"], b.get("tool"), bool(res_ok), flush=True)
out = {"project": "agent-06-mcp-server", "base": BASE, "model": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "protocol": "MCP 2025-06-18, JSON-RPC 2.0 over Streamable HTTP (stateless)",
       "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"), "n_cases": len(P) + len(A),
       "metrics": {"protocol_compliance": ratio(sum(x["pass"] for x in P), len(P)), "tool_call_success": ratio(sum(x["pass"] for x in A), len(A)),
                   "tool_selection_accuracy": ratio(sum(x["tool_selected"] for x in A), len(A)),
                   "latency_ms": {"mcp": {"n": len(lat["mcp"]), "p50": pct(lat["mcp"], 50), "p95": pct(lat["mcp"], 95)}, "agent": {"n": len(lat["agent"]), "p50": pct(lat["agent"], 50), "p95": pct(lat["agent"], 95)}, "note": "end-to-end from the eval client"}},
       "cases": {"protocol": P, "agent": A}}
(here / "results.json").write_text(json.dumps(out, indent=2))
print(json.dumps(out["metrics"], indent=2))
