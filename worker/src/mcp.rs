//! Minimal MCP server (JSON-RPC 2.0 over Streamable HTTP, stateless). Spec: modelcontextprotocol.io
use crate::core::{calc, js_num};
use regex::Regex;
use serde_json::{json, Map, Value};

pub const PROTOCOL: &str = "2025-06-18";
pub const NOTES: [&str; 3] = [
    "MCP servers expose tools, resources and prompts to any MCP client.",
    "Cloudflare Workers can host remote MCP servers on the free tier.",
    "JSON-RPC 2.0 messages carry a method, params and an id.",
];

pub fn tool_list() -> Value {
    json!([
      {"name":"calculator","description":"Evaluate arithmetic like (2+3)*4","inputSchema":{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}},
      {"name":"word_stats","description":"Count words, characters and sentences in text","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}},
      {"name":"search_notes","description":"Keyword search over the server's notes","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
      {"name":"utc_time","description":"Current UTC time","inputSchema":{"type":"object","properties":{}}}
    ])
}

fn need_str<'a>(args: &'a Value, key: &str, method: &str) -> Result<&'a str, String> {
    match args.get(key) {
        None | Some(Value::Null) => Err(format!("Cannot read properties of undefined (reading '{method}')")),
        Some(Value::String(s)) => Ok(s),
        Some(_) => Err(format!("{key}.{method} is not a function")),
    }
}

fn now_iso() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        String::from(js_sys::Date::new_0().to_iso_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z.rem_euclid(146097);
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", if m <= 2 { y + 1 } else { y }, m, d, rem / 3_600_000, rem % 3_600_000 / 60_000, rem % 60_000 / 1000, rem % 1000)
    }
}

fn run_tool(name: &str, args: &Value) -> Option<Result<String, String>> {
    Some(match name {
        "calculator" => {
            let e = match args.get("expression") {
                None | Some(Value::Null) => "undefined".to_string(),
                Some(Value::String(s)) => s.clone(),
                Some(o) => o.to_string(),
            };
            calc(&e).map(js_num)
        }
        "word_stats" => need_str(args, "text", "match").map(|t| {
            let words = Regex::new(r"\S+").unwrap().find_iter(t).count();
            let sentences = Regex::new(r"[.!?]+(?:\s|$)").unwrap().find_iter(t).count();
            json!({"words": words, "chars": t.encode_utf16().count(), "sentences": sentences}).to_string()
        }),
        "search_notes" => need_str(args, "query", "toLowerCase").map(|q| {
            let ql = q.to_lowercase();
            let words: Vec<&str> = Regex::new(r"[^A-Za-z0-9_]+").unwrap().split(&ql).filter(|w| !w.is_empty()).collect();
            let hits: Vec<&str> = NOTES.iter().copied().filter(|n| words.iter().any(|w| n.to_lowercase().contains(w))).collect();
            if hits.is_empty() { "No matching notes.".to_string() } else { hits.join("\n") }
        }),
        "utc_time" => Ok(now_iso()),
        _ => return None,
    })
}

fn js(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::String(s)) => s.clone(),
        Some(o) => o.to_string(),
    }
}

/// Returns the JSON-RPC response, or None for a notification.
pub fn handle(msg: &Value) -> Option<Value> {
    let empty = Map::new();
    let d = msg.as_object().unwrap_or(&empty);
    let rid = d.get("id").cloned().unwrap_or(Value::Null);
    let method = d.get("method").and_then(|m| m.as_str());
    let noparams = json!({});
    let params = d.get("params").filter(|p| p.is_object()).unwrap_or(&noparams);
    let ok = |r: Value| Some(json!({"jsonrpc":"2.0","id":rid,"result":r}));
    let err = |c: i32, m: String| Some(json!({"jsonrpc":"2.0","id":rid,"error":{"code":c,"message":m}}));
    if d.is_empty() || d.get("jsonrpc") != Some(&json!("2.0")) || method.is_none() {
        return err(-32600, "Invalid Request".into());
    }
    if !d.contains_key("id") {
        return None; // notification, e.g. notifications/initialized
    }
    match method.unwrap() {
        "initialize" => ok(json!({"protocolVersion": PROTOCOL, "capabilities": {"tools": {}, "resources": {}}, "serverInfo": {"name": "agent-06-mcp-server", "version": "1.1.0"}})),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tool_list()})),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or(json!({}));
            match run_tool(name, &args) {
                None => err(-32602, format!("Unknown tool: {}", js(params.get("name")))),
                Some(Ok(t)) => ok(json!({"content": [{"type":"text","text":t}], "isError": false})),
                Some(Err(e)) => ok(json!({"content": [{"type":"text","text":format!("Error: {e}")}], "isError": true})),
            }
        }
        "resources/list" => ok(json!({"resources": [{"uri":"notes://all","name":"All notes","mimeType":"text/plain"}]})),
        "resources/read" => {
            if params.get("uri").and_then(|u| u.as_str()) == Some("notes://all") {
                ok(json!({"contents": [{"uri":"notes://all","mimeType":"text/plain","text":NOTES.join("\n")}]}))
            } else {
                err(-32602, "Unknown resource".into())
            }
        }
        m => err(-32601, format!("Method not found: {m}")),
    }
}

/// Body already parsed from JSON. Returns (status, json); status 202 has no body.
pub fn handle_http(body: &Value) -> (u16, Value) {
    let out: Value = match body {
        Value::Array(a) => Value::Array(a.iter().filter_map(handle).collect()),
        other => handle(other).unwrap_or(Value::Null),
    };
    if out.is_null() || out.as_array().map_or(false, |a| a.is_empty()) {
        return (202, Value::Null);
    }
    (200, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rpc(method: &str, params: Value, id: Value) -> Value {
        handle(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})).unwrap()
    }
    fn text(r: &Value) -> String { r["result"]["content"][0]["text"].as_str().unwrap().to_string() }

    #[test]
    fn initialize_list_calculator() {
        assert_eq!(rpc("initialize", json!({}), json!(1))["result"]["protocolVersion"], PROTOCOL);
        let t = rpc("tools/list", json!({}), json!(1));
        assert_eq!(t["result"]["tools"].as_array().unwrap().len(), 4);
        assert!(t["result"]["tools"][0].get("run").is_none());
        assert_eq!(text(&rpc("tools/call", json!({"name":"calculator","arguments":{"expression":"(2+3)*4"}}), json!(7))), "20");
        assert_eq!(text(&rpc("tools/call", json!({"name":"calculator","arguments":{"expression":"0.1+0.2"}}), json!(7))), "0.30000000000000004");
        assert_eq!(rpc("tools/call", json!({"name":"calculator","arguments":{"expression":"x"}}), json!(7))["result"]["isError"], true);
    }
    #[test]
    fn notifications_ids_and_errors() {
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        assert_eq!(rpc("nope", json!({}), json!(7))["error"]["code"], -32601);
        assert_eq!(rpc("ping", json!({}), json!("abc"))["id"], "abc");
        assert_eq!(handle(&json!({"id":1,"method":"ping"})).unwrap()["error"]["code"], -32600);
        assert_eq!(rpc("tools/call", json!({"name":"nope"}), json!(7))["error"]["code"], -32602);
    }
    #[test]
    fn word_stats_search_resources() {
        let w: Value = serde_json::from_str(&text(&rpc("tools/call", json!({"name":"word_stats","arguments":{"text":"Hi there. Bye!"}}), json!(7)))).unwrap();
        assert_eq!(w, json!({"words":3,"chars":14,"sentences":2}));
        assert!(text(&rpc("tools/call", json!({"name":"search_notes","arguments":{"query":"json-rpc"}}), json!(7))).contains("JSON-RPC"));
        assert_eq!(text(&rpc("tools/call", json!({"name":"search_notes","arguments":{"query":"zebra"}}), json!(7))), "No matching notes.");
        assert_eq!(rpc("resources/list", json!({}), json!(7))["result"]["resources"][0]["uri"], "notes://all");
        assert!(rpc("resources/read", json!({"uri":"notes://all"}), json!(7))["result"]["contents"][0]["text"].as_str().unwrap().contains("MCP"));
        assert_eq!(rpc("resources/read", json!({"uri":"x://y"}), json!(7))["error"]["code"], -32602);
    }
    #[test]
    fn calc_rules() {
        assert_eq!(calc("2+3*4"), Ok(14.0));
        assert_eq!(calc("-(2+3)"), Ok(-5.0));
        assert_eq!(calc("10%4"), Ok(2.0));
        for e in ["2+", "(1+2", "alert(1)", ""] {
            assert!(calc(e).is_err(), "{e}");
        }
    }
    #[test]
    fn http_batch_and_notification() {
        let (s, out) = handle_http(&json!([{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"}]));
        assert_eq!((s, out.as_array().unwrap().len()), (200, 1));
        assert_eq!(handle_http(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).0, 202);
    }
}
