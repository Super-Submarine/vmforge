# VMForge guest-agent wire protocol (v1.1)

Transport: one **virtio-serial** channel per VM, wired by the engine at QEMU
launch (see `README.md` for the exact flags — binding form in
[`docs/interface-contracts.md` §3](../docs/interface-contracts.md)):

- Host side: UNIX socket at `$VMFORGE_HOME/vms/<vm>/guest-agent.sock`
- Guest side: `/dev/virtio-ports/org.vmforge.agent.0`

## Framing & envelopes

Newline-delimited JSON, QMP-style (one JSON document per line):

```
request:  {"execute": "<command>", "id": <int>, "arguments": {...}?}\n
success:  {"id": <int>, "return": <object|array>}\n
failure:  {"id": <int>, "error": {"code": "<stable-code>", "message": "..."}}\n
```

- Requests are **strictly serialized per channel**; the agent answers in
  order and echoes `id` verbatim (contract G1).
- Unknown commands return `error.code = "unknown_command"` — the agent never
  disconnects or crashes on bad input (G1). Malformed JSON / non-object
  requests return `error.code = "invalid_request"` (no `id` echo is possible
  for unparseable lines).
- Clients key error handling on `error.code`; `error.message` is
  human-readable. (`error.desc` is also emitted for v0 back-compat and will
  be removed in protocol v2.)

### Stable error codes

| code | meaning |
|---|---|
| `unknown_command` | command not supported by this agent |
| `invalid_request` | malformed JSON or non-object request |
| `invalid_args` | arguments failed validation |
| `exec_not_found` | `exec`: executable does not exist |
| `exec_timeout` | `exec`: command exceeded its timeout |
| `exec_failed` | `exec`: OS-level spawn failure |
| `internal_error` | unexpected agent-side failure |

## Versioning & compatibility

- The protocol is versioned via **`info.agent_version`** (semver; contract
  G3). This document describes protocol **1.1** (`agent_version = 1.1.x`).
- **Additive changes only** within a major version: new commands, new
  optional arguments, new response fields. Clients must ignore unknown
  response fields.
- **Client behavior with older agents (v0):** v0 agents answer neither
  `info` nor report `agent_version`. `GuestAgentClient.check_protocol()`
  detects both cases (an `unknown_command`/legacy error to `info`, or an
  `info` reply without `agent_version`) and raises
  `GuestAgentIncompatible` (`code = "incompatible_agent"`) with an "upgrade
  guest tools" message — instead of failing obscurely downstream.
- **Client behavior with newer agents:** if `info.agent_version` has a major
  version greater than the client's `PROTOCOL_MAJOR`, the client refuses
  with `incompatible_agent`. Same-major newer minors are fine (additive).
- **Agent behavior with older clients:** unknown commands get
  `unknown_command`; nothing is ever removed within a major version, so a
  v1.0 client keeps working against a v1.1 agent.

## Commands

### `ping` → `{}`
Heartbeat. Also used by `wait_ready` boot polling and `wait_down` shutdown
polling.

### `info` → `{os, kernel, hostname, agent_version, arch, supported_commands}`
The first four fields are the M1 contract shape; the rest are additive.

### `interfaces` → `[{name, mac, ips: [<addr>, ...]}, ...]`
Per-NIC addresses (IPv4 + IPv6), from `ip -j addr`. Contract §3 shape; the
GUI/CLI join this with the engine's NAT port-forward table (contract §2) to
render connectivity info.

### `net-info` → `{hostname, ips}`
Convenience for the GUI/CLI connectivity panel: guest hostname plus all
non-loopback addresses, flattened.

### `shutdown {mode: "powerdown"|"reboot"|"halt"}` → `{}`
Graceful in-guest shutdown/reboot. The agent **replies before executing** so
the host always sees the ack, then invokes `poweroff`/`reboot`/`halt`.

**Host-side timeout + hard-stop fallback:** the agent cannot guarantee the
guest actually goes down (wedged init, etc.). `GuestAgentClient.
shutdown_graceful(mode, timeout, hard_stop)` sends `shutdown`, polls `ping`
until the agent goes silent, and on timeout invokes the caller-supplied
`hard_stop()` (the engine's QMP `quit` / SIGKILL of QEMU — QMP itself is
engine-owned, contract N2). CLI:
`vmforgectl.py --vm <vm> shutdown --wait --shutdown-timeout 60
--hard-stop-cmd '<qmp quit / kill>'`.

### `exec {argv, timeout?, stdin?, cwd?}` → `{exit_code, stdout, stderr, stdout_truncated, stderr_truncated}`
Run a command in the guest and capture the result.

- `argv`: non-empty list of strings, executed **without a shell**.
- `timeout`: seconds, default 30, max 300 → `exec_timeout` error on expiry.
- `stdin`: optional base64 bytes fed to the child.
- `cwd`: optional working directory.
- `stdout`/`stderr` are **base64** in the response (binary-safe), clamped to
  1 MiB per stream with the `*_truncated` flags set when clamped.
  `GuestAgentClient.exec_in_guest()` decodes them to text by default.
- Security: the agent runs as root inside the guest; `exec` is intentionally
  full-power (parity with VMware Tools/qemu-ga `guest-exec`). The only
  exposure is the host-side socket, which lives under `$VMFORGE_HOME`
  (user-private) — never in `/tmp`.

## Deprecated v0 aliases

`guest-ping`, `guest-info`, `guest-get-host-info`,
`guest-network-get-interfaces`, `guest-shutdown` are still served with their
v0 response shapes so existing scripts keep working; they will be dropped in
protocol v2. New code must use the contract names above.

## Conformance transcript

`tests/golden_transcript.jsonl` contains request/response pairs consumed by
`tests/test_protocol.py`; the QA conformance job (M1 plan CI gate 3) and the
future Rust client can replay the same file.
