// Parity helper: runs the same JSON-RPC corpus as the legacy JS harness; prints JSON.
use agent_06_mcp_server::mcp::{handle, handle_http};
use serde_json::{json, Value};
fn norm(mut r: Value) -> Value {
    if let Some(t) = r.pointer("/result/content/0/text").and_then(|t| t.as_str()).map(String::from) {
        if t.ends_with('Z') && t.len() > 10 && t.as_bytes()[4] == b'-' && t.as_bytes()[10] == b'T' { r["result"]["content"][0]["text"] = json!("<ts>"); }
    }
    r
}
fn main() {
    let msgs: Vec<Value> = serde_json::from_str(&std::fs::read_to_string("/tmp/msgs6.json").unwrap()).unwrap();
    let batches: Vec<Vec<Value>> = serde_json::from_str(&std::fs::read_to_string("/tmp/batches6.json").unwrap()).unwrap();
    let single: Vec<Value> = msgs.iter().map(|m| norm(handle(m).unwrap_or(Value::Null))).collect();
    let batch: Vec<Value> = batches.iter().map(|b| { let (s, o) = handle_http(&Value::Array(b.clone())); if s == 202 { Value::Null } else { o } }).collect();
    println!("{}", serde_json::to_string(&json!({"single": single, "batch": batch})).unwrap());
}
