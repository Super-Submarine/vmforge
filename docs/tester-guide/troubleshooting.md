# Troubleshooting & FAQ

Each entry: symptom → diagnose (copy-paste) → fix → the severity to file if the
fix doesn't work. Severity rubric: see the [tester guide](README.md#severity-rubric-used-everywhere-in-this-guide).

## T1. KVM not available or not writable

- **Symptom:** `vmforge info` prints `no hardware-accelerated backend available
  on this host` (exit 1); or QEMU errors with `Could not access KVM kernel
  module: Permission denied`; or everything is extremely slow (see T4).
- **Diagnose:**
  ```sh
  ls -l /dev/kvm                       # exists? group kvm? mode 660?
  [ -w /dev/kvm ] && echo writable || echo NOT-writable
  groups | grep -q kvm && echo in-kvm-group || echo not-in-kvm-group
  lsmod | grep kvm                     # kvm_intel / kvm_amd loaded?
  egrep -c '(vmx|svm)' /proc/cpuinfo   # >0 = CPU supports virtualization
  ```
- **Fix:**
  - `/dev/kvm` missing → enable VT-x/AMD-V in BIOS/UEFI; `sudo modprobe
    kvm_intel` (Intel) or `sudo modprobe kvm_amd` (AMD). Inside a VM you need
    nested virtualization enabled on the outer hypervisor.
  - Not writable → `sudo usermod -aG kvm "$USER"`, then log out/in (or
    `newgrp kvm`); re-check with `[ -w /dev/kvm ]`.
- **If unfixable:** file **P2** (blocks the golden path on your host) with the
  diagnose output above.

## T2. QEMU missing or too old

- **Symptom:** `qemu-system-x86_64: command not found`; or snapshot/restore
  misbehaves after a host QEMU upgrade.
- **Diagnose:**
  ```sh
  qemu-system-x86_64 --version
  which qemu-img && qemu-img --version
  ```
- **Fix:** Debian/Ubuntu `sudo apt install qemu-system-x86 qemu-utils`; Fedora
  `sudo dnf install qemu-system-x86 qemu-img`. If live snapshots already exist
  on disk, keep the host QEMU version pinned — RAM/device state is versioned.
- **Severity if stuck:** **P2**.

## T3. VM fails to boot (image / firmware issues)

- **Symptom:** QEMU exits immediately, or the smoke suite reports `boot did not
  reach ready state` / `QEMU exited during boot`.
- **Diagnose:**
  ```sh
  qemu-img check disk.qcow2            # header/refcount integrity
  qemu-img info disk.qcow2             # really qcow2? backing chain intact?
  tail -50 qa/smoke/.work/serial-*.log # what did the guest last print?
  ```
- **Fix:** re-download the image (delete the cached copy in `qa/smoke/.work/`);
  recreate overlays with `qemu-img create -f qcow2 -b BASE -F qcow2 OVERLAY`;
  make sure the QEMU binary matches the guest architecture. QEMU refusing to
  start on a missing/corrupt image is expected, verified behavior.
- **Severity:** boot failure with a healthy disk and image = **P2**; if a boot
  failure corrupted a disk that previously worked, that is data loss = **P1**.

## T4. Fell back to TCG (everything is very slow)

- **Symptom:** VM works but boots in minutes, not seconds (KVM ~10–25 s vs TCG
  ~60–180 s for the reference Alpine guest).
- **Diagnose:**
  ```sh
  cargo run -p vmforge-cli -- info                  # healthy: "accelerator: kvm"
  [ -w /dev/kvm ] && echo kvm || echo tcg           # mirrors the engine's choice
  env | grep FORCE_TCG
  ```
- **Fix:** fix T1. Cross-ISA guests (x86 guest on ARM host, or vice versa) are
  *always* TCG — use a same-ISA guest image.
- **Severity:** working-but-slow with an unfixable T1 = **P3** (note it in the
  survey); silent TCG fallback when KVM *is* healthy = **P2**.

## T5. qcow2 / snapshot chain errors

- **Symptom:** `Could not open backing file` / `No such file or directory` on a
  qcow2 path; snapshot create fails; restore fails with `snapshot not found: <tag>`.
- **Diagnose:**
  ```sh
  qemu-img info --backing-chain disk.qcow2   # first missing link is shown
  qemu-img snapshot -l disk.qcow2            # which tags actually exist?
  qemu-img check disk.qcow2
  ```
- **Fix:** restore the backing file at its recorded path, or if it merely moved:
  `qemu-img rebase -u -b NEWPATH -F qcow2 overlay.qcow2` (only when the content
  is identical). Never write to a base image that has overlays — create a new
  overlay instead. Wrong tag → list tags first. Restoring a nonexistent tag
  fails cleanly without harming the running VM (verified negative test F6).
- **Severity:** **any restore that fails or restores the wrong state is
  automatically P1.** Chain errors you caused by moving files and can repair =
  **P3**.

## T6. Network / port forward not reachable

- **Symptom:** `Connection refused` or timeout connecting to a forwarded host
  port; forward works from the host but not from another machine.
- **Diagnose:**
  ```sh
  ss -tlnp | grep <hostport>           # is anything listening on the host side?
  # inside the guest:
  ss -tlnp | grep <guestport>          # is the guest service actually up?
  ```
- **Fix:** make sure the guest service listens on `0.0.0.0`, not only
  `127.0.0.1`; user-mode NAT forwards bind host `127.0.0.1` by default, so they
  are reachable from the host only — that is by design in v0. ICMP (`ping`)
  does not work through user-mode NAT; test with TCP. Bridged/TAP networking
  (guest addressable from the LAN) is **not implemented yet** — it exists only
  as a design (`networking/DESIGN.md`); don't file its absence as a bug.
- **Severity:** documented NAT limitations = not a bug; a forward that matches
  the docs but doesn't work = **P2**.

## FAQ

**Q: Does VMForge phone home / collect telemetry?**
No. Nothing is collected or uploaded. Diagnostics exist only as text you
explicitly paste into a bug report ([Reporting bugs](reporting-bugs.md)).

**Q: The first smoke run downloaded ~200 MB — is that normal?**
Yes: the known-good Alpine guest image. It's cached in `qa/smoke/.work/` and
not downloaded again.

**Q: Where are logs?**
Serial console logs from the smoke suite: `qa/smoke/.work/serial-*.log`. QEMU
errors print to stderr of whatever launched it.

**Q: How do I find my VM's IP address / run commands inside the guest / shut it down gracefully?**
Guest tools (`vmforgectl` + the `vmforge-ga` agent, over virtio-serial) provide
`net-info` (guest IP), `exec`, and graceful `shutdown` with a hard-stop
fallback — but they are still in an open PR (#4), not on `main`. Until they
land: get the IP from inside the guest (`ip addr` on the serial console), and
shut down from inside the guest (`poweroff`).

**Q: Can I use bridged networking so the VM is reachable from my LAN?**
Not yet. Only user-mode NAT (host-only-reachable port forwards) exists;
bridged/TAP is design-only (`networking/DESIGN.md`).

**Q: Can I test on macOS?**
Not in wave 1. macOS/HVF joins wave 2, gated on HVF snapshot parity.

**Q: My VM was killed (power loss, `kill -9`) — is the disk toast?**
No: a VM killed mid-boot leaves the qcow2 consistent and relaunchable (verified
negative test F3). Run `qemu-img check` to confirm; if check reports errors
after a hard kill, file **P1** (data loss).

**Q: How long should boot / snapshot / restore take?**
Reference guest (Alpine, 512 MB, KVM): boot-to-login ~10–25 s. Snapshot time
scales with guest RAM. If restore takes > 30 s, mention the timing in an issue.

**Q: What do I do when a script step fails but I can keep going?**
That's the definition of **P2**: file it with the step ID (e.g. `AT-4.3`) and
continue with the rest of the script.
