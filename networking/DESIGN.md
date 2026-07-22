# VMForge Networking — v1 Design

Status: draft for review · Author: Dev (Networking) · Scope: what comes after
the v0 user-mode NAT backend shipped in this directory.

## 0. Recap: v0 (shipped)

v0 is QEMU's user-mode networking (SLIRP): a userspace TCP/IP stack inside the
QEMU process that NATs guest traffic onto the host's sockets. It requires no
host privileges and no host configuration, and supports host->guest port
forwarding via `hostfwd` (statically on the command line, dynamically via the
HMP `hostfwd_add`/`hostfwd_remove` commands reached through QMP
`human-monitor-command`).

Known limitations (why v1 exists):

- Performance: every packet traverses a userspace TCP/IP stack; throughput is
  far below TAP/bridged ([QEMU wiki: Documentation/Networking](https://wiki.qemu.org/Documentation/Networking)).
- The guest is not addressable from the LAN, and ICMP (ping) only works with
  extra sysctl configuration ([QEMU networking docs](https://wiki.qemu.org/Documentation/Networking#User_Networking_.28SLIRP.29)).
- No guest-to-guest traffic between VMs on separate netdevs.

## 1. Bridged / TAP mode (Linux)

### Architecture

A TAP device is a kernel virtual Ethernet interface whose "wire" side is a
file descriptor held by QEMU (`-netdev tap`). Frames the guest sends appear on
the TAP; attaching the TAP to a Linux bridge together with the host's physical
NIC puts the guest on the physical L2 segment — the classic "bridged
networking" of VMware/VirtualBox
([QEMU wiki: Documentation/Networking](https://wiki.qemu.org/Documentation/Networking#Tap),
[Linux kernel TUN/TAP docs](https://www.kernel.org/doc/html/latest/networking/tuntap.html)).

Proposed VMForge flow per bridged NIC:

1. Ensure bridge `vmforge-br0` exists and enslaves the chosen uplink
   (or reuse an existing host bridge the user selects).
2. Create TAP `vmfg-<vmid>-<n>`, owned by the VMForge user, attach to bridge.
3. Launch QEMU with `-netdev tap,id=...,ifname=vmfg-...,script=no,downscript=no`
   plus `-device virtio-net-pci`.
4. Tear down the TAP on VM shutdown (bridge persists).

`vhost=on` should be enabled for virtio-net + TAP to move the datapath into
the kernel vhost-net module for significantly better throughput
([QEMU virtio-net docs / vhost](https://www.linux-kvm.org/page/UsingVhost)).

### Required host privileges

Creating TAP devices and modifying bridges requires `CAP_NET_ADMIN`
([tuntap kernel doc](https://www.kernel.org/doc/html/latest/networking/tuntap.html)).
Options, in order of preference:

1. **qemu-bridge-helper** (ships with QEMU): a setuid-root helper that creates
   the TAP and attaches it to a bridge listed in an ACL file
   (`/etc/qemu/bridge.conf`, e.g. `allow vmforge-br0`). QEMU is then launched
   unprivileged with `-netdev bridge,br=vmforge-br0`
   ([qemu-bridge-helper docs](https://wiki.qemu.org/Features/HelperNetworking)).
   This is the default plan for VMForge on Linux.
2. **Privileged system daemon**: a small root `vmforge-netd` that pre-creates
   persistent TAPs (`ip tuntap add mode tap user vmforge`) and hands them to
   the unprivileged engine. Persistent TAPs owned by a user can be opened
   without root ([ip-tuntap man page](https://man7.org/linux/man-pages/man8/ip-tuntap.8.html)).
3. Granting the engine binary `CAP_NET_ADMIN` via file capabilities — rejected
   for the desktop product (too broad; lets the app reconfigure all host
   networking).

Caveat: bridging a **Wi-Fi** uplink generally does not work because 802.11
frames only allow the associated station's MAC unless 4-address (WDS) mode is
enabled; VMware/VirtualBox work around this with MAC-NAT. For Wi-Fi hosts we
will offer NAT or a routed mode instead
([Linux Foundation bridge docs](https://wiki.linuxfoundation.org/networking/bridge#it-doesnt-work-with-my-wireless-card)).

## 2. Host-only networks

Host-only = guests and host can talk to each other; no uplink to the LAN.
Implementation on Linux: a bridge with **no physical NIC enslaved**, host gets
an IP on the bridge, guest TAPs attach to it — same TAP machinery as bridged
mode minus the uplink. VMForge will run a small DHCP/DNS service (dnsmasq or
built-in) bound to the bridge, mirroring VirtualBox's host-only adapter +
DHCP server design
([VirtualBox manual §6.7 Host-Only Networking](https://www.virtualbox.org/manual/ch06.html#network_hostonly)).
Optionally, enabling IP forwarding + masquerade rules on the host converts a
host-only network into a routed/NAT network (libvirt's "default" network
pattern, [libvirt networking docs](https://wiki.libvirt.org/VirtualNetworking.html)).

Config model addition:

```jsonc
{ "mode": "host-only", "name": "vmforge-host0",
  "subnet": "192.168.56.0/24", "dhcp": {"start": "192.168.56.10"} }
```

## 3. macOS backend (vmnet)

macOS has no TAP/bridge kernel API for third parties; Apple provides
**vmnet.framework** with three modes that map 1:1 onto our model
([Apple vmnet docs](https://developer.apple.com/documentation/vmnet)):

- `VMNET_SHARED_MODE` -> NAT (host-managed DHCP + NAT, like v0 but kernel-backed),
- `VMNET_BRIDGED_MODE` -> bridged onto a chosen host interface,
- `VMNET_HOST_MODE` -> host-only.

QEMU has native vmnet netdevs since 7.1: `-netdev vmnet-shared`,
`-netdev vmnet-bridged,ifname=en0`, `-netdev vmnet-host`
([QEMU 7.1 changelog](https://wiki.qemu.org/ChangeLog/7.1#Networking),
[QEMU invocation docs](https://www.qemu.org/docs/master/system/invocation.html)).

Privileges: vmnet historically required root or the
`com.apple.vm.networking` entitlement (restricted; granted by Apple to
virtualization vendors). Since macOS 26 (Tahoe), unsigned/dev-signed apps can
use vmnet-shared without root
([QEMU invocation docs, vmnet section](https://www.qemu.org/docs/master/system/invocation.html);
[Apple entitlement doc](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.vm.networking)).
Plan: request the entitlement for the signed VMForge app; fall back to
user-mode NAT when unavailable.

## 4. Windows backend notes

Target accelerator is WHP; QEMU on Windows supports:

- **User-mode NAT (v0)** — works unchanged; default.
- **TAP via tap-windows6** (the OpenVPN NDIS 6 driver): QEMU `-netdev tap`
  attaches to an installed TAP-Windows adapter; bridging is then done with
  Windows' own bridge or ICS. Requires installing a signed kernel driver —
  acceptable since VMForge ships an installer, but adds servicing burden
  ([OpenVPN tap-windows6 repo](https://github.com/OpenVPN/tap-windows6),
  [QEMU Windows networking notes](https://wiki.qemu.org/Documentation/Networking)).
- **Hyper-V virtual switch**: only usable by Hyper-V VMs, not by QEMU/WHP
  guests, so a VMware-style "vmnet" experience needs our own driver or
  tap-windows6 ([Microsoft Hyper-V virtual switch docs](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/plan/plan-hyper-v-networking-in-windows-server)).

Plan: v1 ships NAT-only on Windows; bridged/host-only on Windows tracked as
v2 behind tap-windows6 bundling.

## 5. Config & API surface (v1)

Extend the v0 JSON config with a `mode` discriminator:

```jsonc
{ "mode": "nat" | "bridged" | "host-only",
  // nat: v0 fields (net, forwards, ...)
  // bridged: { "uplink": "eth0" } or { "bridge": "br0" }
  // host-only: { "name": "...", "subnet": "...", "dhcp": {...} }
}
```

`build_qemu_args()` grows per-mode/per-OS backends returning the right
`-netdev` (user / tap / bridge / vmnet-*) while the `-device virtio-net-pci`
half stays identical everywhere. Host-side setup/teardown (bridges, TAPs,
dnsmasq) moves into a `HostNetworkProvisioner` interface with Linux/macOS/
Windows implementations, invoked by the core engine before/after QEMU launch.

## 6. Open questions

- Do we require qemu-bridge-helper from distro QEMU packages, or ship our own
  (affects packaging and the setuid audit surface)?
- MAC-NAT fallback for Wi-Fi bridging: implement or document-as-unsupported?
- IPv6 story for NAT and host-only (SLIRP supports IPv6; vmnet NAT66 is
  macOS-version dependent).
