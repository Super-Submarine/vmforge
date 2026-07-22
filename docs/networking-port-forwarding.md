# User-mode port forwarding & guest SSH access (EXPERIMENTAL)

> **Status:** experimental. The `vmforge net` commands and the `--forward`
> flag are behind an experimental marker until the wave-1 CLI-freeze decision;
> the flag shape follows the binding interface contracts
> ([`interface-contracts.md`](interface-contracts.md) §2/§4), so it can ship
> stable without changes if the freeze approves it.

VMForge's default networking mode is user-mode NAT (QEMU SLIRP): no host
privileges, no TAP devices. The guest can reach out, but the host cannot reach
in — unless you add **port forwards** (`hostfwd`), which map a host TCP/UDP
port to a port inside the guest. The canonical use case is SSH into the guest
(UAT-6).

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
N1) — the argv is identical to what the engine passes to QEMU at boot, and to
the Python `vmforge_net` reference implementation's output.

## Configuring forwards at VM create/boot

Once the Phase 1 engine's lifecycle commands land, the same flag attaches to
`create` (per the binding CLI contract):

```sh
vmforge create dev --cpus 2 --memory 2048 --disk-size 10737418240 --forward 2222:22
vmforge boot dev
```

Forwards are persisted in `vm.json` under `nics[].port_forwards` and reported
by `vmforge status --json`. On a **running** VM, forwards are added/removed
live over QMP (`human-monitor-command` wrapping HMP
`hostfwd_add`/`hostfwd_remove`) via `vmforge_core::net::NetworkBackend`.

## SSH into a guest

1. Create/boot the VM with a forward mapping guest port 22, e.g.
   `--forward 2222:22`.
2. Make sure an SSH server runs in the guest (`sshd` enabled, a user with a
   password or authorized key).
3. Connect — or let the helper print the command for you:

```sh
$ vmforge net ssh-command --forward 2222:22 --user alice
ssh -p 2222 alice@127.0.0.1
```

If you already know the host port, `vmforge net ssh-command --host-port 2222`
works without repeating the forward spec.

## Manually running QEMU with a forward (debugging)

```sh
qemu-system-x86_64 -accel kvm -m 1024 -drive file=disk.qcow2,if=virtio \
  $(vmforge net args --forward 2222:22)
ssh -p 2222 root@127.0.0.1
```

## Troubleshooting

- **`Could not set up host forwarding rule ...` / boot fails** — the host
  port is already in use. Pick another port (`ss -tlnp | grep 2222` shows the
  owner). The engine does not pre-check port availability (invariant N3); the
  QEMU/HMP error is surfaced as-is.
- **`Connection refused` on the host port** — QEMU isn't running, or the
  forward wasn't configured. `vmforge status --json` (engine) or your QEMU
  command line should show the `hostfwd=` rule.
- **Connection accepted, then hangs or resets** — the forward is fine but
  nothing listens on the guest port. SLIRP accepts on the host before
  connecting inside the guest, so a missing in-guest `sshd` looks like a
  stalled connection, not a refusal. Check `systemctl status sshd` in the
  guest console.
- **Works on 127.0.0.1 but not from another machine** — forwards bind
  loopback by default. Use `tcp:0.0.0.0:2222:22` to bind all interfaces
  (mind your firewall).
- **`ssh` host-key warnings after rebuilding a VM** — the guest generated new
  host keys. `ssh-keygen -R "[127.0.0.1]:2222"` clears the stale entry.
- **Guest has no network at all** — user-mode NAT gives the guest
  10.0.2.15/24 via built-in DHCP; gateway 10.0.2.2, DNS 10.0.2.3. If the
  guest uses static networking, point it at those.
- **UDP forwards seem flaky** — SLIRP's UDP forwarding is best-effort and
  session-based; prefer TCP for anything stateful.

## Testing

- Unit + conformance tests: `cargo test -p vmforge-core net`
  (argv generation, spec parsing, mocked QMP `hostfwd_add`/`hostfwd_remove`
  including host-port-conflict errors).
- CLI integration tests: `cargo test -p vmforge-cli --test net_cli`.
- Both run in plain CI without KVM.
