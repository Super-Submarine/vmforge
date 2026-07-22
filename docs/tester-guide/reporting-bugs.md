# Reporting Bugs

All bug reports and feedback go to GitHub issues on
[Super-Submarine/vmforge](https://github.com/Super-Submarine/vmforge/issues).
Chat is for quick questions only — anything actionable becomes an issue within
24 h; the issue tracker is the system of record.

## Which template?

| Use | When |
|---|---|
| **Beta bug report** | Something broke, failed, or behaved wrong. Restore failures, data loss, and running-VM crashes are always **P1** — file immediately. |
| **Beta feedback / friction / feature request** | Nothing is broken, but something is confusing, annoying, or missing (**P3**, batched weekly). |

The bug template asks for: severity (P1/P2/P3 per the
[rubric](README.md#severity-rubric-used-everywhere-in-this-guide) — pick "Not
sure" and we'll triage), the AT/UAT step ID if you were running a script (e.g.
`AT-4.3`), host platform and details, VMForge version, guest image, steps to
reproduce, expected vs. actual, and timing if performance-related.

## Diagnostics to attach (`vmforge diagnose`)

Diagnostics are **opt-in only** — nothing is ever uploaded automatically. Wave 1
is self-report: the minimal diagnostic bundle is the output below, pasted into
the "Logs / diagnostics" field of the bug template.

The `vmforge diagnose` subcommand ships with the M1 CLI. Until it lands, this
copy-paste block produces the same minimal output:

```sh
{
  echo "== vmforge diagnose (manual, wave-1) =="
  date -u +"%Y-%m-%dT%H:%M:%SZ"
  echo "-- version --"
  git -C "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" rev-parse --short HEAD 2>/dev/null
  echo "-- host --"
  uname -srmo; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  echo "-- kvm --"
  ls -l /dev/kvm 2>&1; [ -w /dev/kvm ] && echo kvm:writable || echo kvm:NOT-writable
  lsmod | grep -E '^kvm' || echo "kvm modules: none loaded"
  echo "-- qemu --"
  qemu-system-x86_64 --version | head -1; qemu-img --version | head -1
  echo "-- backend --"
  cargo run -q -p vmforge-cli -- info 2>&1
  echo "-- disk space --"
  df -h "$HOME" | tail -1
} 2>&1
```

That is the complete field list (per the wave-1 diagnostics descope — no
telemetry, no guest data, no file contents): timestamp, VMForge version/commit,
host OS/CPU/RAM, KVM device state, QEMU versions, backend probe output, and
free disk space. **Review the output before pasting — it contains your
hostname/CPU model but no secrets; redact anything you're not comfortable
sharing.**

If the failure involved a specific VM, also attach:

```sh
tail -50 qa/smoke/.work/serial-*.log          # last guest console output
qemu-img info --backing-chain <disk.qcow2>    # for disk/snapshot issues
qemu-img check <disk.qcow2>
```

## A good bug report in 60 seconds

1. Pick the right template and severity (or "Not sure").
2. One-line title stating symptom + where: `[Bug]: restore hangs at 90% on AT-4.3`.
3. Paste the step ID if you were running a script.
4. Numbered repro steps, expected vs. actual.
5. Paste the diagnostics block output.
6. For anything performance-related, include your wall-clock measurement.

P1s are acknowledged within 24 hours. If you filed a P1 and hear nothing in a
day, ping the beta chat with the issue link.
