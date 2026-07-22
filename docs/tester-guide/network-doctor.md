# Diagnosing guest connectivity: `vmforge-net doctor`

> **Surface stability: EXPERIMENTAL.** `vmforge-net` is not part of the
> wave-1 CLI freeze (see [`docs/cli-freeze-v1.0-beta.md`](../cli-freeze-v1.0-beta.md) §4).
> Verb names, flags, and output shapes may change between beta builds —
> don't script against it.

If your VM has no network, SSH port-forwarding doesn't connect, or DNS is
broken inside the guest, run the network doctor before filing a bug:

```console
$ pip install ./networking          # once, from the repo checkout
$ vmforge-net doctor
RESULT  CHECK                  DETAIL
PASS    host.tun               /dev/net/tun present and openable read/write
SKIP    host.bridge_helper     qemu-bridge-helper not installed
PASS    host.nat_firewall      found: nft, iptables
...
```

- Every check reports **PASS**, **FAIL**, or **SKIP**; each FAIL includes a
  `hint:` line with the fix.
- Exit code: `0` = no failures, `1` = at least one FAIL, `2` = usage error.
- `--json` prints one machine-readable JSON document (schema v1) instead of
  the table. `vmforge diagnose` runs `vmforge-net doctor --json`
  automatically when it is installed and includes the report in its bundle
  as `net-doctor.json` — so attaching a diagnose bundle to a bug report
  already contains these results.

## Options

| Flag | Meaning |
|---|---|
| `--home PATH` | VMForge home (default `$VMFORGE_HOME` or `~/.vmforge`) |
| `--vm NAME` | Scope per-VM checks (config, forwards) to one VM |
| `--config FILE` | Validate one NAT JSON config instead of scanning per-VM configs |
| `--guest-exec CMD` | Host command prefix that runs its argument inside the guest (e.g. `vmforge guest exec myvm --`); enables the guest-side probes |
| `--timeout SECS` | Per-probe timeout (default 3) |
| `--json` | Machine-readable output |

## The checks

### `host.tun` — tun/tap device availability

Verifies `/dev/net/tun` exists and can be opened read/write.

- **FAIL, missing:** `sudo modprobe tun` (persist via
  `/etc/modules-load.d/`). Note that user-mode (SLIRP) NAT — the VMForge
  default — works without tun; this only blocks bridged/TAP mode.
- **FAIL, not openable:** grant your user access with a udev rule, or use
  user-mode NAT.

### `host.bridge_helper` — bridge helper permissions

Only matters for bridged/TAP networking. Looks for `qemu-bridge-helper`
and checks it is setuid root (or has `cap_net_admin`), plus that
`/etc/qemu/bridge.conf` exists.

- **SKIP:** helper not installed — fine unless you use bridged mode.
- **FAIL, no setuid/caps:** `sudo chmod u+s /usr/lib/qemu/qemu-bridge-helper`.
- **FAIL, missing bridge.conf:** `echo 'allow br0' | sudo tee /etc/qemu/bridge.conf`.

### `host.nat_firewall` — iptables/nftables presence

Routed/bridged NAT needs `nft` or `iptables` for masquerading.

- **FAIL:** install `nftables`. User-mode (SLIRP) NAT does not need either.

### `config.valid` — per-VM network config validity

Parses each VM's `network.json` (or the file given via `--config`) against
the NAT config schema (netdev id, MAC, subnets, forward rules).

- **FAIL:** the detail names the file and the exact validation error; see
  `networking/examples/nat.json` for the expected shape.

### `host.mtu` — MTU/interface sanity

Finds the host's default-route interface and checks its MTU is within
[1280, 65536].

- **FAIL, no default route:** the host itself is offline — guests can't have
  internet without it.
- **FAIL, bad MTU:** `sudo ip link set <iface> mtu 1500`.

### `forwards.health` — port-forward rule health

For every `hostfwd` rule in every VM's config:

- flags host `proto:ip:port` combinations claimed by two different VMs;
- **VM not running:** checks the host port is still bindable — a FAIL here
  means another process already holds the port and the VM will fail to
  start (`ss -ltnp 'sport = :<port>'` finds the culprit);
- **VM running** (per its pidfile): makes a TCP connection to the host
  port — a FAIL means the forward is missing from the running QEMU or no
  guest service is listening on the guest port.

UDP rules are validated but not probed.

### `nat.guest_to_host` / `nat.guest_to_internet` — NAT reachability

Run from **inside the guest** via `--guest-exec`, so they need the guest
agent up. Without it both are SKIP.

- `guest_to_host` probes the SLIRP gateway `10.0.2.2`. FAIL: check the guest
  got its DHCP lease (`10.0.2.15` by default) and the NIC is user-mode NAT.
- `guest_to_internet` probes `1.1.1.1:443`. FAIL: check the host's own
  internet access, that the NIC config does not set `restrict=on`, and host
  firewall egress rules.

### `dns.guest` — DNS resolution from guest

Resolves `example.com` inside the guest (via `--guest-exec`; SKIP without
it). FAIL: point the guest resolver at the SLIRP DNS forwarder `10.0.2.3`
(normally set by DHCP) or set an explicit `dns=` in the NIC config.

## Including results in a bug report

Prefer attaching the full diagnose bundle, which embeds the doctor JSON:

```console
$ vmforge diagnose --output vmforge-diag.tar
```

Otherwise paste the `vmforge-net doctor` table output directly.
