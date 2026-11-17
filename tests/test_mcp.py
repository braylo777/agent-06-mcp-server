import json, re
import pytest
from agent06.mcp import handle, handle_http, PROTOCOL
from agent06.core import calc
from agent06.agent import pick_call, llm_tools, run_agent

rpc = lambda method, params=None, id=7: handle({"jsonrpc": "2.0", "id": id, "method": method, **({"params": params} if params is not None else {})})
text = lambda r: r["result"]["content"][0]["text"]


def test_initialize_and_list_hides_run():
    assert rpc("initialize", {})["result"]["protocolVersion"] == PROTOCOL
    t = rpc("tools/list")["result"]["tools"]
    assert len(t) == 4 and "run" not in t[0]


def test_calculator_and_errors_are_iserror():
    assert text(rpc("tools/call", {"name": "calculator", "arguments": {"expression": "(2+3)*4"}})) == "20"
    assert rpc("tools/call", {"name": "calculator", "arguments": {"expression": "x"}})["result"]["isError"] is True
    assert text(rpc("tools/call", {"name": "calculator", "arguments": {"expression": "0.1+0.2"}})) == "0.30000000000000004"


def test_notifications_and_unknown_method():
    assert handle({"jsonrpc": "2.0", "method": "notifications/initialized"}) is None
    assert rpc("nope")["error"]["code"] == -32601


def test_calc_precedence_and_junk():
    assert calc("2+3*4") == 14 and calc("-(2+3)") == -5 and calc("10%4") == 2 and calc("1.5*2") == 3
    for e in ["2+", "(1+2", "alert(1)", ""]:
        with pytest.raises(Exception):
            calc(e)


def test_id_echo_and_invalid_request_and_unknown_tool():
    assert rpc("ping", {}, "abc")["id"] == "abc"
    assert handle({"id": 1, "method": "ping"})["error"]["code"] == -32600
    assert rpc("tools/call", {"name": "nope"})["error"]["code"] == -32602


def test_word_stats_and_search_notes():
    assert json.loads(text(rpc("tools/call", {"name": "word_stats", "arguments": {"text": "Hi there. Bye!"}}))) == {"words": 3, "chars": 14, "sentences": 2}
    assert "JSON-RPC" in text(rpc("tools/call", {"name": "search_notes", "arguments": {"query": "json-rpc"}}))
    assert text(rpc("tools/call", {"name": "search_notes", "arguments": {"query": "zebra"}})) == "No matching notes."


def test_resources():
    assert rpc("resources/list")["result"]["resources"][0]["uri"] == "notes://all"
    assert "MCP" in rpc("resources/read", {"uri": "notes://all"})["result"]["contents"][0]["text"]
    assert rpc("resources/read", {"uri": "x://y"})["error"]["code"] == -32602


def test_llm_tools_and_pick_call():
    t = llm_tools()
    assert len(t) == 4 and all(x["parameters"]["type"] == "object" for x in t)
    assert pick_call({"tool_calls": [{"name": "calculator", "arguments": {"expression": "1+1"}}]}) == {"name": "calculator", "arguments": {"expression": "1+1"}}
    assert pick_call({"tool_calls": [{"name": "calculator", "arguments": '{"expression":"2"}'}]})["arguments"] == {"expression": "2"}
    assert pick_call({"response": '{"name":"utc_time","parameters":{}}'})["name"] == "utc_time"
    assert pick_call({"response": "hello"}) is None


def test_run_agent_routes_through_mcp():
    n = [0]
    def ai(m, i):
        n[0] += 1
        return {"tool_calls": [{"name": "calculator", "arguments": {"expression": "17*23+4"}}]} if n[0] == 1 else {"response": "It is 395."}
    r = run_agent(ai, "17*23+4?")
    assert r["tool"] == "calculator" and r["result"] == "395" and r["isError"] is False


def test_http_batch_and_notification():
    s, out = handle_http([{"jsonrpc": "2.0", "id": 1, "method": "ping"}, {"jsonrpc": "2.0", "method": "notifications/initialized"}])
    assert s == 200 and len(out) == 1
    assert handle_http({"jsonrpc": "2.0", "method": "notifications/initialized"}) == (202, None)
