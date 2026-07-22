# VMForge CLI Freeze — v1.0-beta (wave 1)

**Status:** FROZEN for wave-1 beta (Linux-only per decision `019f8a7b-75b1`)
**Scope:** every command-line surface a wave-1 tester or script can touch: the
`vmforge` binary (`crates/vmforge-cli`), the `vmforge-storage` CLI
(`storage/`), and the QA smoke-suite entry point.
**Contract:** anything marked **stable** below may not change verb name, flag
name, positional-argument order, or exit-code meaning until the wave-1 beta
ends. Additive changes (new verbs, new optional flags, new exit codes for new
failure classes) are allowed but must update this doc and
`qa/cli-freeze/frozen-surface.json` in the same PR (CI enforces the manifest;
see §5). Anything marked **experimental** may change without notice — tester
docs and UAT/AT scripts must not depend on it.

**Baseline note (PR #3):** the M1 lifecycle verbs live on PR #3
(`devin/1784731213-core-engine-v0`, `core/src/main.rs`) and are **not on
`main`** at freeze time. Per the contract they are enumerated here from that
branch and marked experimental until the PR merges; on merge they can be
promoted to stable via a PR that updates this doc + the manifest.

---

## 1. `vmforge` (Rust CLI, `crates/vmforge-cli` — on `main`)

### 1.1 Frozen verbs — **stable**

| Verb | Args/flags | Behavior | Status |
|---|---|---|---|
| `info` | none | Print selected backend + capabilities (backend, accelerator, accelerated archs, live snapshot, virtio-gpu 3D) | **stable** |
| *(no verb)* | none | Alias of `info` | **stable** |

There are **no other verbs and no flags** on `main` — no `--help`, no
`--version`, no `--json`. Unknown verbs (including `--help`/`--version`)
exit 2.

### 1.2 Frozen exit codes — **stable**

| Exit | Meaning |
|---|---|
| 0 | Backend found; capabilities printed on stdout |
| 1 | No hardware-accelerated backend on this host (message on stderr) |
| 2 | Unknown command (message on stderr names the verb) |

### 1.3 M1 lifecycle verbs (PR #3, `core/src/main.rs`) — **experimental**

Enumerated for step-ID reconciliation; **do not script against these** until
promoted. Shapes as implemented on the PR #3 branch:

```
vmforge [--root PATH] create <name> [--cpus N] [--memory MiB] --disk PATH [--disk-size SIZE] [--iso PATH]
vmforge [--root PATH] start <name>
vmforge [--root PATH] stop <name> [--grace SECS]
vmforge [--root PATH] status <name>
vmforge [--root PATH] list
vmforge [--root PATH] snapshot create  <name> <tag>
vmforge [--root PATH] snapshot restore <name> <tag>
vmforge [--root PATH] snapshot delete  <name> <tag>
vmforge [--root PATH] snapshot list    <name>
```

Exit codes on PR #3: 0 success, 1 any error (message `error: ...` on stderr),
2 usage error (clap). Known deltas vs the binding interface contract
(`docs/interface-contracts.md` §4) that must be resolved before promotion:
contract says `boot` (PR #3: `start`), `stop --force` (PR #3: `--grace`),
`pause`/`resume`/`tree`/`delete`/`restore <vm> <snap>` (absent), `--json` on
every verb (absent), `--forward tcp:HOST:GUEST` (absent),
`vmforge --contract-version` (absent), JSON `{"error": ...}` on stderr
(absent). These contract-§4 shapes are likewise **experimental** until
implemented.

## 2. `vmforge-storage` (Python CLI, `storage/` — on `main`)

### 2.1 Global flags — **stable**

| Flag | Meaning |
|---|---|
| `--home PATH` | VMForge home (default `$VMFORGE_HOME` or `~/.vmforge`) |
| `--json` | Machine-readable JSON on stdout (exactly one document) |
| `--contract-version` | Print interface-contract major version (`1`) and exit 0 |

### 2.2 Frozen verbs — **stable**

| Verb | Positionals | Flags |
|---|---|---|
| `create` | `vm disk size` | `--preallocation {off,metadata,falloc,full}`, `--cluster-size` |
| `resize` | `vm disk size` | `--shrink` |
| `import` | `src` | `--name`, `--vm`, `--disk`, `--format`, `--compress` |
| `clone` | `base vm disk` | `--size` |
| `delete` | `vm disk` | `--force` |
| `info` | `vm disk` | — |
| `check` | `vm disk` | `--repair` |
| `tree` | `vm disk` | — (alias of `snapshot list`) |
| `snapshot create` | `vm disk name` | — |
| `snapshot list` | `vm disk` | — |
| `snapshot revert` | `vm disk name` | — |
| `snapshot delete` | `vm disk name` | — |

### 2.3 Frozen exit codes — **stable**

| Exit | Meaning |
|---|---|
| 0 | Success (with `--json`: one JSON document on stdout) |
| 1 | Storage/backend error — JSON error object `{"error": {"code", "message", ...}}` on stderr |
| 2 | Usage error (argparse) |
| 3 | `check` completed and found corruptions/leaks |

## 3. QA smoke suite (`qa/smoke/smoke_test.sh` — on `main`) — **stable**

Frozen because `docs/tester-guide` (PR #11) directs wave-1 testers to it as
the create→boot→snapshot→restore golden path until M1 merges:
`qa/smoke/smoke_test.sh [--negative]` and env vars `FORCE_TCG`,
`GUEST_IMAGE_URL`, `GUEST_LOGIN_REGEX`, `BOOT_TIMEOUT`, `WORKDIR`, `VM_MEM`,
`DRIVER`. Exit 0 = all steps passed, nonzero = failure.

## 4. `vmforge-net` (PR #2, not on `main`) — **experimental**

`args`, `hostfwd-add`, `hostfwd-remove` with `--config/--netdev-id/--forward/
--format/--qmp-unix/--qmp-tcp`. Not frozen; UAT-6 (SSH port-forward) is
**out of wave 1** — see decision recorded via `POST /api/decisions`
(referenced in `docs/uat-step-id-reconciliation.md` §3).

## 5. Enforcement

`qa/cli-freeze/check.py` verifies the live surface against
`qa/cli-freeze/frozen-surface.json` (vmforge verb/exit-code behavior probed by
executing the built binary; vmforge-storage verbs/flags introspected from its
argparse parser). CI runs it on every PR/push (`cli-freeze` job in
`.github/workflows/ci.yml`) and fails if a frozen verb, flag, or exit code
changes without updating the manifest + this doc.
