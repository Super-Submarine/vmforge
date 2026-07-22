# Port forwarding & SSH into a guest

> **Status: NOT YET MERGED, EXPERIMENTAL.** The `vmforge net` verbs and the
> `--forward` flag are Networking v1.2, in review on PR
> [#15](https://github.com/Super-Submarine/vmforge/pull/15)
> (`devin/1784736622-net-port-forwarding`). Per the wave-1 freeze doc
> ([`docs/cli-freeze-v1.0-beta.md`](../cli-freeze-v1.0-beta.md) §4) the
> networking CLI is **experimental** — do not script against it. SSH
> port-forward UAT-6 is out of wave-1 scope. Shapes below are as implemented
> on the PR branch.

VMForge's default networking is user-mode NAT (QEMU SLIRP): no host
privileges, no TAP devices. The guest can reach out, but the host cannot
reach in — unless you add **port forwards** (`hostfwd`), mapping a host
TCP/UDP port to a port inside the guest. The canonical use case is SSH into
the guest.

## Forward spec syntax

```
[tcp|udp:][HOSTIP:]HOSTPORT:GUESTPORT
```

| Spec | Meaning |
|---|---|
| `2222:22` | tcp, host 127.0.0.1:2222 → guest :22 |
| `udp:5353:53` | udp, host 127.0.0.1:5353 → guest :53 |
| `tcp:0.0.0.0:8080:80` | tcp, all host interfaces :8080 → guest :80 |

The flag is repeatable. Forwards bind **loopback only** by default
(local-first privacy); pass an explicit `0.0.0.0` host IP to expose a guest
port on the LAN.

## Inspecting the generated QEMU arguments

```sh
$ vmforge net args --forward 2222:22
-netdev user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22 -device virtio-net-pci,netdev=net0

$ vmforge net args --forward 2222:22 --forward udp:5353:53 --json
{"nic":{...},"qemu_args":["-netdev","user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=udp:127.0.0.1:5353-:53","-device","virtio-net-pci,netdev=net0"]}
```

`net args` is a pure, deterministic function of its flags (contract invariant
N1) — the argv is identical to what the engine passes to QEMU at boot.

## SSH into a guest

1. Create/boot the VM with a forward mapping guest port 22
   (`--forward 2222:22`; the flag attaches to `vmforge create` once the
   engine lifecycle verbs — PR #3, experimental — merge alongside).
2. Make sure an SSH server runs in the guest (`sshd` enabled, a user with a
   password or authorized key).
3. Connect — or let the helper print the command for you:

```sh
$ vmforge net ssh-command --forward 2222:22 --user alice
ssh -p 2222 alice@127.0.0.1
```

If you already know the host port, `vmforge net ssh-command --host-port 2222`
works without repeating the forward spec.

Forwards are persisted in `vm.json` under `nics[].port_forwards`; on a
running VM, forwards are added/removed live over QMP.

## Available on `main` today: manual QEMU forwards

Until PR #15 merges, add a `hostfwd` rule directly to your QEMU command line
(the smoke suite and manual boots):

```sh
qemu-system-x86_64 -accel kvm -m 1024 -drive file=disk.qcow2,if=virtio \
  -netdev user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22 \
  -device virtio-net-pci,netdev=net0
ssh -p 2222 root@127.0.0.1
```

The Python reference implementation on PR #2 (`python -m vmforge_net args`)
generates the same arguments.

## Troubleshooting forwards

- **`Could not set up host forwarding rule ...` / boot fails** — the host
  port is already in use. `ss -tlnp | grep 2222` shows the owner; pick
  another port. (Engine error class `port_in_use`, exit 19 — see the
  [error-code troubleshooting guide](error-codes.md#port_in_use--exit-19).)
- **`Connection refused` on the host port** — QEMU isn't running or the
  forward wasn't configured; check for the `hostfwd=` rule on the QEMU
  command line.
- **Connection accepted, then hangs** — the forward is fine but nothing
  listens on the guest port. SLIRP accepts on the host before connecting
  inside the guest, so a missing in-guest `sshd` looks like a stalled
  connection, not a refusal.
- **Works on 127.0.0.1 but not from another machine** — forwards bind
  loopback by default; use `tcp:0.0.0.0:2222:22` (mind your firewall).
- **`ssh` host-key warnings after rebuilding a VM** — the guest generated
  new host keys: `ssh-keygen -R "[127.0.0.1]:2222"`.
- **Guest has no network at all** — user-mode NAT gives the guest
  10.0.2.15/24 via built-in DHCP; gateway 10.0.2.2, DNS 10.0.2.3.
- **UDP forwards seem flaky** — SLIRP UDP forwarding is best-effort;
  prefer TCP for anything stateful.

For deeper connectivity issues, run the network doctor — see
[Diagnostics](diagnostics.md).
