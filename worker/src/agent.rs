//! LLM-as-MCP-client: Llama 3.3 sees the server's tools/list, picks one, and the call goes through the same JSON-RPC handler.
use crate::mcp::{handle, tool_list};
use regex::Regex;
use serde_json::{json, Value};

pub const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";

#[allow(async_fn_in_trait)]
pub trait Ai {
    async fn run(&self, model: &str, input: Value) -> Result<Value, String>;
}

pub fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}
pub fn parse_json(s: &str) -> Option<Value> {
    Regex::new(r"[\[{][\s\S]*[\]}]").unwrap().find(s).and_then(|m| serde_json::from_str(m.as_str()).ok())
}
pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}

pub fn llm_tools() -> Value {
    Value::Array(tool_list().as_array().unwrap().iter().map(|t| json!({"name": t["name"], "description": t["description"], "parameters": t["inputSchema"]})).collect())
}

pub fn pick_call(r: &Value) -> Option<Value> {
    if let Some(tc) = r.get("tool_calls").and_then(|t| t.as_array()).and_then(|a| a.first()) {
        let f = tc.get("function");
        let nonnull = |v: Option<&Value>| v.filter(|x| !x.is_null()).cloned();
        let name = nonnull(tc.get("name")).or_else(|| nonnull(f.and_then(|f| f.get("name")))).unwrap_or(Value::Null);
        let a = nonnull(tc.get("arguments")).or_else(|| nonnull(f.and_then(|f| f.get("arguments"))));
        let arguments = match a {
            Some(Value::String(s)) => parse_json(&s).unwrap_or(json!({})),
            Some(o) => o,
            None => json!({}),
        };
        return Some(json!({"name": name, "arguments": arguments}));
    }
    let j = parse_json(&text(r))?;
    let n = j.get("name")?.as_str()?;
    if !tool_list().as_array().unwrap().iter().any(|t| t["name"] == n) {
        return None;
    }
    let truthy = |v: Option<&Value>| v.filter(|x| !x.is_null() && **x != json!(false) && **x != json!(0) && **x != json!("")).cloned();
    Some(json!({"name": n, "arguments": truthy(j.get("arguments")).or_else(|| truthy(j.get("parameters"))).unwrap_or(json!({}))}))
}

pub async fn run_agent<A: Ai>(ai: &A, task: &str) -> Result<Value, String> {
    let sys = "You can call tools from an MCP server. If a tool helps, call exactly one tool with correct arguments.";
    let first = ai.run(MODEL, json!({"messages":[{"role":"system","content":sys},{"role":"user","content":task}],"max_tokens":200,"tools":llm_tools()})).await?;
    let Some(call) = pick_call(&first) else {
        return Ok(json!({"tool": null, "arguments": null, "result": null, "answer": text(&first)}));
    };
    let res = handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":call})).unwrap();
    let is_err = res.get("error").is_some();
    let result = if is_err { format!("Error: {}", res["error"]["message"].as_str().unwrap_or("")) } else { res["result"]["content"][0]["text"].as_str().unwrap_or("").to_string() };
    let name = match &call["name"] { Value::String(s) => s.clone(), Value::Null => "undefined".into(), o => o.to_string() };
    let msgs = json!([{"role":"system","content":"Answer the user in one short sentence using the tool result."},{"role":"user","content":format!("Task: {task}\nTool {name} returned: {result}")}]);
    let answer = text(&ai.run(MODEL, json!({"messages": msgs, "max_tokens": 120})).await?);
    Ok(json!({"tool": call["name"], "arguments": call["arguments"], "result": result, "isError": is_err || res["result"]["isError"] == true, "answer": answer}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::Cell;

    struct Seq(Cell<u32>);
    impl Ai for Seq {
        async fn run(&self, _m: &str, _i: Value) -> Result<Value, String> {
            self.0.set(self.0.get() + 1);
            Ok(if self.0.get() == 1 { json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"17*23+4"}}]}) } else { json!({"response":"It is 395."}) })
        }
    }
    #[test]
    fn llm_tools_and_pick_call() {
        let t = llm_tools();
        assert_eq!(t.as_array().unwrap().len(), 4);
        assert!(t.as_array().unwrap().iter().all(|x| x["parameters"]["type"] == "object"));
        assert_eq!(pick_call(&json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"1+1"}}]})), Some(json!({"name":"calculator","arguments":{"expression":"1+1"}})));
        assert_eq!(pick_call(&json!({"tool_calls":[{"name":"calculator","arguments":"{\"expression\":\"2\"}"}]})).unwrap()["arguments"], json!({"expression":"2"}));
        assert_eq!(pick_call(&json!({"response":"{\"name\":\"utc_time\",\"parameters\":{}}"})).unwrap()["name"], "utc_time");
        assert_eq!(pick_call(&json!({"response":"hello"})), None);
    }
    #[test]
    fn run_agent_routes_through_mcp() {
        let r = block_on(run_agent(&Seq(Cell::new(0)), "17*23+4?")).unwrap();
        assert_eq!(r["tool"], "calculator");
        assert_eq!(r["result"], "395");
        assert_eq!(r["isError"], false);
        assert!(cap(Some("hi"), 500) && !cap(Some(""), 500));
    }
}
