# `vmforge diagnose` — diagnostics bundle for bug reports

Produces a single shareable report of host and VM state for beta bug reports
(paste it into the "Diagnostics output" field of the
[bug report template](../.github/ISSUE_TEMPLATE/), per the tester guide's
[Reporting Bugs](tester-guide/reporting-bugs.md) page).

## Usage

```sh
vmforge diagnose                          # full text report to stdout (paste-able)
vmforge diagnose --vm win11-test          # scope per-VM sections to one VM
vmforge diagnose --output diag.txt        # write text bundle; summary to stdout
vmforge diagnose --output diag.tar        # tarball: report.txt + per-VM log excerpts
vmforge diagnose --home /path/to/home     # override $VMFORGE_HOME (default ~/.vmforge)
```

The `.tar` bundle is plain uncompressed ustar — inspect it with
`tar -tf diag.tar` / `tar -xf diag.tar` before attaching it anywhere.

## What is collected (complete list)

Matches the wave-1 diagnostics field list in the tester guide — no telemetry,
no guest data, no arbitrary file contents:

| Section | Contents |
|---|---|
| `version` | VMForge version |
| `host` | kernel/OS (`uname -srmo`), CPU model, total RAM |
| `kvm` | `/dev/kvm` presence/writability, loaded `kvm*` modules |
| `qemu` | first version line of `qemu-system-x86_64`, `qemu-system-aarch64`, `qemu-img` |
| `backend` | `vmforge info` backend probe (accelerator, capabilities) |
| `disk space` | one `df -h` line for `$VMFORGE_HOME`'s filesystem |
| `config` | `$VMFORGE_HOME/config.{toml,json}` contents, **redacted** |
| per VM | status (pidfile probe), disk list with backing chain and snapshot tree (via `vmforge-storage` when installed, filesystem summary otherwise), per-VM network config file (**redacted**) |
| per VM logs | last 200 lines of up to 8 `*.log` files under `vms/<vm>/` and `vms/<vm>/logs/`, **redacted** |

What is **never** collected: guest disk contents, files outside
`$VMFORGE_HOME` (other than the read-only host probes above), environment
variables, shell history, or credentials of any kind.

## Privacy guardrails

- **Opt-in only.** Nothing is uploaded automatically, ever. The report is
  printed or written locally; attaching it to a bug report is a manual step.
- **Redaction before output.** Every config file, network file, and log
  excerpt passes through the redaction filter
  (`crates/vmforge-cli/src/redact.rs`) before it reaches the report:
  1. values of sensitive-looking keys (`password`, `secret`, `token`,
     `api_key`, `private_key`, `access_key`, `credential`, `authorization`,
     `cookie`, `passphrase`, ...) are replaced with `[REDACTED]`;
  2. `Bearer`/`Basic` authorization values are replaced wherever they appear;
  3. PEM blocks (`-----BEGIN ... KEY-----`) are removed entirely;
  4. long high-entropy tokens (40+ base64/hex chars) are replaced even
     without a recognizable key name.
- **Residual data.** The report still contains your CPU model, kernel
  version, RAM/disk sizes, VM/disk/snapshot names, and log lines that are not
  secret-shaped. **Review the output before sharing** and delete anything you
  are not comfortable with — the redaction filter is a guardrail, not a
  guarantee.

## CI

`cargo test -p vmforge-cli` covers the redaction rules, the tar writer, and
end-to-end `diagnose` runs against a fixture `$VMFORGE_HOME` (including
secret-leak checks). Nothing requires KVM, QEMU, or the network.
