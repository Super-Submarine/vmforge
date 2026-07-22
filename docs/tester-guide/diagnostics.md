# Diagnostics: `vmforge doctor`, `vmforge-net doctor`, `vmforge diagnose`

VMForge ships (or is about to ship) three diagnostic tools. They answer
different questions — use this table to pick:

| Tool | Question it answers | Output | Status |
|---|---|---|---|
| `vmforge doctor` | "Can this host run VMs at all?" — preflight: KVM, QEMU, disk space, disk images | PASS/FAIL probes, taxonomy exit codes | **Not merged** — PR [#30](https://github.com/Super-Submarine/vmforge/pull/30) |
| `vmforge-net doctor` | "Why does my VM have no network / broken SSH forward / broken DNS?" | PASS/FAIL/SKIP table with fix hints | **Not merged, no PR yet** — branch `devin/1784739117-net-doctor` |
| `vmforge diagnose` | "What do I attach to a bug report?" — redacted host+VM state bundle | Text report or `.tar` bundle | **Not merged** — PR [#17](https://github.com/Super-Submarine/vmforge/pull/17) |

Rule of thumb: **doctor before you boot, net doctor when the guest can't
talk, diagnose when you file a bug.**

None of the three is on `main` yet; all are additive to the frozen wave-1 CLI
surface and **experimental** until promoted in
[`docs/cli-freeze-v1.0-beta.md`](../cli-freeze-v1.0-beta.md). Shapes below
are as implemented on their branches. Until they merge, use the manual
diagnostics block in [Reporting bugs](reporting-bugs.md).

## `vmforge doctor` — host preflight (PR #30)

```sh
vmforge doctor [--json] [--disk PATH]...
```

Runs the preflight probes in order and exits with the first failing probe's
taxonomy exit code (0 when all pass):

1. **kvm** — `/dev/kvm` exists (`kvm_unavailable`, exit 10) and opens
   read/write (`kvm_permission_denied`, exit 11)
2. **qemu** — a `qemu-system-x86_64`/`qemu-system-aarch64` binary is found
   (`qemu_binary_missing`, exit 12)
3. **home** — `$VMFORGE_HOME` is writable and its volume has ≥ 512 MiB free
   (`disk_full`, exit 15)
4. **disk** (per `--disk PATH`) — image exists (`disk_image_missing`,
   exit 16) and has a valid qcow2 header (`disk_image_corrupt`, exit 17)

With `--json`, the full probe report is one JSON document on stdout
(`{"ok": bool, "probes": [...]}`); on failure the first error object is also
emitted on stderr in the taxonomy wire format. Error codes and recovery steps
are keyed to the engine error taxonomy — see
[`docs/error-taxonomy.md`](../error-taxonomy.md) (PR #30) and the
[error-code troubleshooting guide](error-codes.md).

## `vmforge-net doctor` — guest connectivity (branch, no PR yet)

```sh
pip install ./networking     # once, from the repo checkout
vmforge-net doctor [--json] [--vm NAME] [--config FILE] \
                   [--guest-exec CMD] [--timeout SECS] [--home PATH]
```

Every check reports **PASS**, **FAIL**, or **SKIP**; each FAIL includes a
`hint:` line with the fix. Exit codes: 0 = no failures, 1 = at least one
FAIL, 2 = usage error. Checks:

- `host.tun` — `/dev/net/tun` present/openable (bridged/TAP only; SLIRP
  doesn't need it)
- `host.bridge_helper` — `qemu-bridge-helper` setuid/caps +
  `/etc/qemu/bridge.conf` (bridged only)
- `host.nat_firewall` — `nft`/`iptables` present (routed/bridged NAT only)
- `config.valid` — per-VM `network.json` (or `--config FILE`) against the
  NAT schema
- `host.mtu` — default-route interface exists, sane MTU
- `forwards.health` — cross-VM host-port conflicts; port bindable (VM
  stopped) or connectable (VM running)
- `nat.guest_to_host` / `nat.guest_to_internet` — probes from inside the
  guest via `--guest-exec` (SKIP without it)
- `dns.guest` — resolves `example.com` in the guest (via `--guest-exec`)

`--json` prints one machine-readable document (schema v1); `vmforge diagnose`
embeds it in its bundle as `net-doctor.json` when `vmforge-net` is installed.

## `vmforge diagnose` — bug-report bundle (PR #17)

```sh
vmforge diagnose                          # full text report to stdout (paste-able)
vmforge diagnose --vm win11-test          # scope per-VM sections to one VM
vmforge diagnose --output diag.txt        # write text bundle; summary to stdout
vmforge diagnose --output diag.tar        # tarball: report.txt + per-VM log excerpts
vmforge diagnose --home /path/to/home     # override $VMFORGE_HOME
```

Collects host facts (kernel, CPU, RAM), KVM/QEMU probes, `vmforge info`
output, `$VMFORGE_HOME` disk space, redacted config, and per-VM status,
disk/snapshot info and redacted log excerpts. **Never** collected: guest disk
contents, files outside `$VMFORGE_HOME`, environment variables, shell
history, credentials.

Privacy guardrails: opt-in only (nothing is ever uploaded automatically), and
every config/log excerpt passes a redaction filter (sensitive keys,
`Bearer`/`Basic` values, PEM blocks, high-entropy tokens → `[REDACTED]`).
The filter is a guardrail, not a guarantee — review before sharing.

## Which tool for which symptom

| Symptom | Run |
|---|---|
| Fresh install; VM won't boot / `vmforge info` exits 1 | `vmforge doctor` |
| Boot fails with exit code 10–20 | Look up the code in the [error-code guide](error-codes.md); `vmforge doctor` confirms host state |
| Guest boots but has no internet / DNS broken | `vmforge-net doctor` |
| SSH forward refuses/hangs | `vmforge-net doctor` (see also [Port forwarding](port-forwarding.md)) |
| Filing a bug report | `vmforge diagnose --output diag.tar`, attach the tarball |
