//! Cloudflare Worker entry (wasm32 only). Same routes, CORS and JSON as the old src/index.js.
use crate::agent::{cap, run_agent, Ai};
use crate::mcp::handle_http;
use crate::page::PAGE;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use worker::*;

struct CfAi(worker::Ai);
impl Ai for CfAi {
    async fn run(&self, model: &str, input: Value) -> std::result::Result<Value, String> {
        self.0.run::<Value, Value>(model, input).await.map_err(|e| e.to_string())
    }
}

async fn rate_ok(env: &Env, ip: &str) -> bool {
    let rl = match js_sys::Reflect::get(env.as_ref(), &"RATE_LIMITER".into()) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"key".into(), &ip.into());
    let Ok(f) = js_sys::Reflect::get(&rl, &"limit".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) else { return true };
    let Ok(p) = f.call1(&rl, &opts) else { return true };
    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await {
        Ok(r) => js_sys::Reflect::get(&r, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(true),
        Err(_) => true,
    }
}

fn cors(mut r: Response) -> Result<Response> {
    let h = r.headers_mut();
    h.set("access-control-allow-origin", "*")?;
    h.set("access-control-allow-headers", "content-type, mcp-session-id, mcp-protocol-version")?;
    h.set("access-control-allow-methods", "POST, GET, OPTIONS")?;
    Ok(r)
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let path = req.url()?.path().to_string();
    let method = req.method();
    let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_else(|| "anon".into());
    if path == "/mcp" {
        if method == Method::Options {
            return cors(Response::empty()?);
        }
        if method != Method::Post {
            return cors(Response::error("Use POST (Streamable HTTP, stateless)", 405)?);
        }
        if !rate_ok(&env, &ip).await {
            return Ok(Response::from_json(&json!({"jsonrpc":"2.0","id":null,"error":{"code":-32000,"message":"Rate limited"}}))?.with_status(429));
        }
        let msg: Value = req.json().await.unwrap_or(json!({}));
        let (status, out) = handle_http(&msg);
        if status == 202 {
            return cors(Response::empty()?.with_status(202));
        }
        return cors(Response::from_json(&out)?);
    }
    if method == Method::Post && path == "/api/agent" {
        if !rate_ok(&env, &ip).await {
            return Ok(Response::from_json(&json!({"error":"Rate limit: try again in a minute."}))?.with_status(429));
        }
        let body: Value = req.json().await.unwrap_or(json!({}));
        let t = body.get("task").and_then(|v| v.as_str());
        if !cap(t, 500) {
            return Ok(Response::from_json(&json!({"error":"task required (<=500 chars)"}))?.with_status(400));
        }
        return match run_agent(&CfAi(env.ai("AI")?), t.unwrap()).await {
            Ok(v) => Response::from_json(&v),
            Err(e) => Ok(Response::from_json(&json!({"error": format!("Agent error: {e}")}))?.with_status(502)),
        };
    }
    let h = Headers::new();
    h.set("content-type", "text/html;charset=utf-8")?;
    Ok(Response::ok(PAGE)?.with_headers(h))
}
