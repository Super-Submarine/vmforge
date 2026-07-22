"""Protocol unit tests for the guest agent (no VM required)."""

import base64
import json
from pathlib import Path

import pytest
from conftest import load_agent

agent = load_agent()

TRANSCRIPT = Path(__file__).parent / "golden_transcript.jsonl"


def test_ping_returns_empty():
    assert agent.handle({"execute": "ping"}) == {}


def test_info_contract_shape():
    info = agent.handle({"execute": "info"})
    for key in ("os", "kernel", "hostname", "agent_version"):
        assert key in info, key
    assert info["agent_version"] == agent.AGENT_VERSION
    major = int(info["agent_version"].split(".")[0])
    assert major == 1


def test_unknown_command_error_code():
    resp = agent.process_line(b'{"execute": "no-such-cmd", "id": 42}')
    assert resp["id"] == 42
    assert resp["error"]["code"] == "unknown_command"


def test_malformed_json_never_raises():
    resp = agent.process_line(b"{{{{")
    assert resp["error"]["code"] == "invalid_request"


def test_non_object_request():
    resp = agent.process_line(b"[1,2]")
    assert resp["error"]["code"] == "invalid_request"


def test_id_echo_verbatim():
    resp = agent.process_line(b'{"execute": "ping", "id": 7}')
    assert resp == {"return": {}, "id": 7}


def test_shutdown_invalid_mode():
    with pytest.raises(agent.AgentError) as e:
        agent.handle({"execute": "shutdown", "arguments": {"mode": "hibernate"}})
    assert e.value.code == "invalid_args"


def test_exec_captures_stdout_stderr_exit_code():
    out = agent.handle(
        {
            "execute": "exec",
            "arguments": {"argv": ["sh", "-c", "echo out; echo err >&2; exit 3"]},
        }
    )
    assert out["exit_code"] == 3
    assert base64.b64decode(out["stdout"]) == b"out\n"
    assert base64.b64decode(out["stderr"]) == b"err\n"
    assert out["stdout_truncated"] is False


def test_exec_stdin_roundtrip():
    out = agent.handle(
        {
            "execute": "exec",
            "arguments": {
                "argv": ["cat"],
                "stdin": base64.b64encode(b"hello\n").decode(),
            },
        }
    )
    assert out["exit_code"] == 0
    assert base64.b64decode(out["stdout"]) == b"hello\n"


def test_exec_not_found():
    with pytest.raises(agent.AgentError) as e:
        agent.handle({"execute": "exec", "arguments": {"argv": ["/nonexistent/binary"]}})
    assert e.value.code == "exec_not_found"


def test_exec_timeout():
    with pytest.raises(agent.AgentError) as e:
        agent.handle(
            {"execute": "exec", "arguments": {"argv": ["sleep", "5"], "timeout": 0.2}}
        )
    assert e.value.code == "exec_timeout"


def test_exec_invalid_args():
    for bad in ({}, {"argv": []}, {"argv": "ls"}, {"argv": ["ls"], "timeout": -1},
                {"argv": ["ls"], "stdin": "not-base64!!"}):
        with pytest.raises(agent.AgentError) as e:
            agent.handle({"execute": "exec", "arguments": bad})
        assert e.value.code == "invalid_args", bad


def test_exec_output_truncation():
    out = agent.handle(
        {
            "execute": "exec",
            "arguments": {
                "argv": ["sh", "-c", "head -c %d /dev/zero" % (agent.EXEC_MAX_OUTPUT + 10)]
            },
        }
    )
    assert out["stdout_truncated"] is True
    assert len(base64.b64decode(out["stdout"])) == agent.EXEC_MAX_OUTPUT


def test_golden_transcript():
    """Replay the conformance transcript byte-for-byte (M1 CI gate 3)."""
    with open(TRANSCRIPT) as f:
        for lineno, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            case = json.loads(line)
            resp = agent.process_line(case["send"].encode())
            assert resp == case["expect"], "transcript line %d" % lineno


def test_legacy_aliases_still_served():
    assert agent.handle({"execute": "guest-ping"}) == {}
    info = agent.handle({"execute": "guest-info"})
    assert info["agent_version"] == agent.AGENT_VERSION
