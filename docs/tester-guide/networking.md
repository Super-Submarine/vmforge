# Networking: port forwarding & connecting two VMs

> Wave-1 networking is **user-mode (SLIRP) NAT with port forwarding** via the
> experimental `vmforge net` CLI (networking v1.2). Host-only / internal
> networks are **not shipped yet** — the multi-VM task below uses the
> supported host-forward path instead. When host-only networks land, this
> page will gain a `vmforge net create` section.

## Port forwarding basics

`vmforge net args` prints the QEMU argv fragment for one user-mode NIC:

```sh
vmforge net args --forward 2222:22
# -netdev user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22 -device virtio-net-pci,netdev=net0
```

`SPEC` is `[tcp|udp:][HOSTIP:]HOSTPORT:GUESTPORT`. A bad spec fails with exit
code 2 and a one-line usage hint (no stack trace). To reach a guest's SSH:

```sh
vmforge net ssh-command --forward 2222:22        # prints the ssh invocation
```

## Task: connect two VMs (AT-7)

Goal: prove guest→guest reachability with what is shipped today. VM-B exposes
its SSH port on the host; VM-A reaches it through the SLIRP gateway
(`10.0.2.2` is the host as seen from inside a user-mode guest).

1. Boot **VM-B** with a forward, using the fragment from
   `vmforge net args --forward 2222:22` in your QEMU/driver invocation.
2. From the **host**, verify the forward: `nc -z 127.0.0.1 2222` → exit 0.
3. Boot **VM-A** with a plain user-mode NIC.
4. Inside VM-A: `nc -z 10.0.2.2 2222` → exit 0 means VM-A reached VM-B's
   sshd through the host forward.

**Expected outcome:** both `nc` probes succeed; `ssh -p 2222 <user>@10.0.2.2`
from VM-A gives VM-B's login prompt. If step 2 fails, file a **P2** bug with
the exact `vmforge net args` output and your QEMU command line.

## Known limitations (wave 1)

- Guests are NAT-isolated: VMs cannot see each other directly, only via host
  forwards as above.
- No host-only/internal networks, bridged/TAP is Linux-only and undocumented
  for testers, no static leases.
