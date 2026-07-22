# VMForge — Hypervisor Abstraction Layer (HAL) Architecture

**Status:** Architecture spike (Sprint 0 → Sprint 1) · **Author:** Arjun (VP Engineering) · **Date:** 2026-07-22
**Inputs:** Sprint 0 technical feasibility report ("Technical feasibility: hypervisor foundations for VMForge"), community research, market landscape.
**Targets for v1:** Linux/KVM and macOS/Hypervisor.framework (hvf). Windows/WHP is designed-for but out of scope for this spike.

## 1. Goals & non-goals

**Goals**

- A single, stable Rust trait (`Hypervisor`) that the rest of VMForge (GUI, CLI, snapshot engine) programs against, with per-OS backends selected at runtime.
- Support VMForge's USP hypotheses at the interface level from day one: instant-resume (save/restore of full VM state) and git-like snapshots/branching (content-addressed, tree-shaped snapshot graph), per project context.
- Keep VMForge application code proprietary-friendly: no linking against GPL code; GPL components (QEMU) run strictly out-of-process (mere aggregation, https://www.gnu.org/licenses/gpl-faq.html#MereAggregation).

**Non-goals (v1):** GPU passthrough, live migration between hosts, Windows backend implementation, cross-ISA emulation policy (exposed as a capability flag only).

## 2. Chosen stack & rationale

**Language: Rust.** The engine layer is systems code touching raw ioctls, memory mapping, and long-lived daemons.

- Memory safety without GC matters for a VMM control plane; this is the explicit rationale for the modern Rust VMM ecosystem (cloud-hypervisor: https://github.com/cloud-hypervisor/cloud-hypervisor#objectives; rust-vmm: https://github.com/rust-vmm).
- First-class KVM bindings already exist and are maintained by the rust-vmm project: `kvm-ioctls`/`kvm-bindings` (https://github.com/rust-vmm/kvm, Apache-2.0/MIT), so a future direct-KVM backend does not require writing FFI from scratch.
- macOS Hypervisor.framework is a plain C API (https://developer.apple.com/documentation/hypervisor) callable from Rust via bindgen; community crates exist (e.g. https://crates.io/crates/hv, MIT/Apache-2.0).
- Cargo workspaces give us the multi-crate layout below with one build/lint pipeline (`cargo build`, `cargo clippy`, `cargo fmt`) on both Linux and macOS runners.

**Engine strategy (two-phase, per the feasibility report):**

1. **Phase 1 (ship fast):** both backends drive **QEMU as a separate process** over the QMP JSON protocol (https://www.qemu.org/docs/master/interop/qmp-spec.html), using `-accel kvm` on Linux and `-accel hvf` on macOS (https://www.qemu.org/docs/master/system/introduction.html#virtualisation-accelerators). One device model, one disk format (qcow2 with internal/external snapshots, https://www.qemu.org/docs/master/system/images.html), full guest-OS coverage, and TCG fallback for cross-ISA guests (https://www.qemu.org/docs/master/devel/index-tcg.html).
2. **Phase 2 (differentiate):** where QEMU limits instant-resume latency or snapshot semantics, swap in direct backends behind the same trait — direct `/dev/kvm` ioctls via rust-vmm on Linux (https://docs.kernel.org/virt/kvm/api.html), and Hypervisor.framework / Virtualization.framework on macOS (https://developer.apple.com/documentation/virtualization). The trait is designed so callers cannot tell which engine is underneath.

This is why the abstraction layer exists at all: the trait is the seam that lets us start on QEMU everywhere and specialize per-OS later without touching product code.

**libvirt: deliberately skipped.** libvirt is daemon-centric and weakly supported on macOS/Windows desktops (https://libvirt.org/platforms.html); we speak QMP directly (feasibility report §2).

## 3. Component diagram

```
┌────────────────────────────────────────────────────────────────────┐
│                        VMForge product layer                       │
│   GUI (manager + console)      CLI (vmforge)      Automation API   │
└───────────────┬────────────────────┬───────────────────┬───────────┘
                │            vmforge-core (Rust)         │
┌───────────────▼────────────────────▼───────────────────▼───────────┐
│  Hypervisor trait  ·  VmConfig/VmState types  ·  lifecycle FSM     │
│  SnapshotStore (git-like snapshot DAG, content-addressed)          │
│  Backend registry (runtime selection by host OS + capabilities)   │
└───────┬───────────────────────────────┬────────────────────────────┘
        │ vmforge-backend-kvm           │ vmforge-backend-hvf
┌───────▼───────────────┐       ┌───────▼────────────────┐
│ Linux backend         │       │ macOS backend          │
│ Phase 1: QEMU child   │       │ Phase 1: QEMU child    │
│  process + QMP socket │       │  process + QMP socket  │
│  (-accel kvm)         │       │  (-accel hvf)          │
│ Phase 2: /dev/kvm     │       │ Phase 2: Hypervisor.fw │
│  ioctls via rust-vmm  │       │  / Virtualization.fw   │
└───────┬───────────────┘       └───────┬────────────────┘
        │ ioctl / child proc            │ syscall / child proc
┌───────▼───────────────┐       ┌───────▼────────────────┐
│ Linux kernel: KVM     │       │ macOS: hvf (no kexts)  │
│ (/dev/kvm, VT-x/SVM/  │       │ com.apple.security.    │
│  ARM64)               │       │  hypervisor entitlement│
└───────────────────────┘       └────────────────────────┘
```

Shared guest I/O across both backends uses **virtio** devices (net, blk, gpu, fs) per the OASIS VIRTIO spec (https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html); 3D via virtio-gpu + virgl/Venus (https://docs.mesa3d.org/drivers/venus.html, https://www.qemu.org/docs/master/system/devices/virtio-gpu.html).

## 4. VM lifecycle state machine

```
                 create()
   ┌─────────┐ ──────────► ┌─────────┐
   │ (none)  │             │ DEFINED │◄────────────────────┐
   └─────────┘             └────┬────┘                     │
        ▲                       │ boot()                   │ delete() only
        │ delete()              ▼                          │ from DEFINED
        │                  ┌─────────┐   panic/exit   ┌────┴────┐
        └───────────────── │ RUNNING │ ─────────────► │ STOPPED │
                           └─┬─┬─┬───┘   stop()/      └────┬────┘
              pause()        │ │ │      shutdown()         │ boot()
        ┌────────────────────┘ │ └───────────┐             ▼
        ▼                      │             ▼          (RUNNING)
   ┌─────────┐   resume()      │        ┌──────────────┐
   │ PAUSED  │ ────────────►(RUNNING)   │ SNAPSHOTTING │──► back to prior
   └────┬────┘                │         └──────────────┘    state (RUNNING
        │ snapshot() allowed  │ snapshot() (live,           or PAUSED)
        │ (consistent, fast)  │  QEMU snapshot-save)
        ▼                     ▼
      restore(snapshot_id) — from DEFINED/STOPPED/PAUSED → RUNNING (instant-resume)
```

Rules encoded in `vmforge-core` (backends cannot bypass them):

- `create(VmConfig) → DEFINED`; `boot: DEFINED|STOPPED → RUNNING`; `pause: RUNNING → PAUSED`; `resume: PAUSED → RUNNING`; `stop: RUNNING|PAUSED → STOPPED`; `delete: DEFINED|STOPPED → (none)`.
- `snapshot()` is valid in RUNNING (live) and PAUSED (quiesced); it transitions through a transient SNAPSHOTTING state and returns to the prior state. Snapshots capture disk + RAM + device state (QEMU `snapshot-save`/qcow2 internal snapshots, https://www.qemu.org/docs/master/system/images.html).
- `restore(snapshot_id)` boots directly into RUNNING from saved RAM state — this is the instant-resume primitive.
- Snapshots form a DAG in `SnapshotStore` keyed by content hash with parent pointers → branching = creating a new child of any node, exactly like git commits. (Community research: snapshots are UTM's #1 request and a top VMware pain point — see forum links in the Sprint 0 community report.)

## 5. The `Hypervisor` trait (interface sketch)

```rust
pub trait Hypervisor: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;      // accel, ISAs, live-snapshot, gpu
    fn create(&self, config: &VmConfig) -> Result<VmHandle, HvError>;
    fn boot(&self, vm: &VmHandle) -> Result<(), HvError>;
    fn pause(&self, vm: &VmHandle) -> Result<(), HvError>;
    fn resume(&self, vm: &VmHandle) -> Result<(), HvError>;
    fn stop(&self, vm: &VmHandle) -> Result<(), HvError>;
    fn snapshot(&self, vm: &VmHandle, parent: Option<SnapshotId>) -> Result<SnapshotId, HvError>;
    fn restore(&self, vm: &VmHandle, snapshot: SnapshotId) -> Result<(), HvError>;
    fn delete(&self, vm: VmHandle) -> Result<(), HvError>;
    fn state(&self, vm: &VmHandle) -> Result<VmState, HvError>;
}
```

Backends are compiled per-OS (`#[cfg(target_os = "linux")]` → KVM, `#[cfg(target_os = "macos")]` → HVF) but both stubs build on all platforms in this scaffold so CI on Linux runners exercises everything.

## 6. Repository layout

```
vmforge/
├── Cargo.toml                 # workspace
├── crates/
│   ├── vmforge-core/          # Hypervisor trait, VmConfig, VmState FSM, SnapshotStore, errors
│   ├── vmforge-backend-kvm/   # Linux backend stub (QEMU+KVM phase 1; rust-vmm phase 2)
│   ├── vmforge-backend-hvf/   # macOS backend stub (QEMU+hvf phase 1; Hypervisor.fw phase 2)
│   └── vmforge-cli/           # `vmforge` CLI driving the trait (create/boot/snapshot/...)
├── docs/architecture.md       # this document
├── .github/workflows/ci.yml   # build + clippy + rustfmt on push/PR
└── README.md
```

## 7. Dependency licensing table

| Component | Role | License | Obligation for proprietary VMForge | Source |
|---|---|---|---|---|
| Linux KVM | Kernel hypervisor (Linux) | GPL-2.0 (kernel) w/ syscall exception | None — userspace use via `/dev/kvm` ioctls does not propagate GPL | https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/LICENSES/exceptions/Linux-syscall-note · https://docs.kernel.org/virt/kvm/api.html |
| QEMU | Phase-1 VMM engine | GPL-2.0 | Run as separate process (mere aggregation); publish any QEMU patches we ship | https://www.qemu.org/license.html · https://www.gnu.org/licenses/gpl-faq.html#MereAggregation |
| Hypervisor.framework | macOS hypervisor API | Proprietary Apple system API | Apple developer agreement + `com.apple.security.hypervisor` entitlement; no copyleft | https://developer.apple.com/documentation/hypervisor · https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.hypervisor |
| Virtualization.framework | Optional macOS high-level backend | Proprietary Apple system API | Same as above; macOS guests only on Apple hardware, ≤2 instances per SLA | https://developer.apple.com/documentation/virtualization · https://www.apple.com/legal/sla/docs/macOSSequoia.pdf |
| libvirt | NOT used (daemon-centric; weak macOS/Windows desktop support) | LGPL-2.1+ | n/a (would permit dynamic linking) | https://libvirt.org/platforms.html · https://gitlab.com/libvirt/libvirt/-/blob/master/COPYING.LESSER |
| rust-vmm (kvm-ioctls, kvm-bindings) | Phase-2 direct KVM backend | Apache-2.0 OR MIT | Attribution only | https://github.com/rust-vmm/kvm |
| cloud-hypervisor | Optional hardened Linux backend (later) | Apache-2.0 / BSD-3 | Attribution only | https://github.com/cloud-hypervisor/cloud-hypervisor |
| virtio spec / drivers | Paravirt device model | OASIS spec (open); guest drivers vary | Spec is freely implementable | https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html |
| virglrenderer | Host-side 3D (virgl/Venus) | MIT | Attribution only | https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/main/COPYING |
| Mesa (virgl/venus guest drivers) | Guest 3D drivers | MIT | Attribution only | https://docs.mesa3d.org/license.html |
| MoltenVK | Vulkan-on-Metal for Venus on macOS hosts | Apache-2.0 | Attribution only | https://github.com/KhronosGroup/MoltenVK/blob/main/LICENSE |
| virtio-win guest drivers | Windows guest drivers (shipped as guest ISO) | Mixed GPL/BSD | Redistribute as separate guest ISO, unmodified or with source | https://github.com/virtio-win/kvm-guest-drivers-windows |
| Rust toolchain + crates (clap, serde, thiserror) | App code | MIT/Apache-2.0 | Attribution only | https://github.com/rust-lang/rust · standard crates.io licensing |

## 8. Key risks

1. **Instant-resume latency on QEMU path.** QEMU `snapshot-save`/`load` serializes full RAM; multi-GB guests may resume in seconds, not milliseconds. Mitigation: Phase-2 direct backends with mmap'd RAM images; measure early (spike benchmark in Sprint 1). (https://www.qemu.org/docs/master/system/images.html)
2. **Git-like snapshot DAG vs qcow2 semantics.** qcow2 internal snapshots are linear-ish and slow to delete at scale (a top VMware complaint per community research). Mitigation: external snapshots / backing-file chains managed by `SnapshotStore`, content-addressed.
3. **HVF is low-level.** Hypervisor.framework gives vCPUs + memory only — device model is on us; Phase 1 avoids this via QEMU, but Phase 2 macOS effort is significant. (https://developer.apple.com/documentation/hypervisor)
4. **Cross-ISA expectations on Apple silicon.** Only ARM64 guests are hardware-accelerated; x86 guests fall back to slow TCG. Must be explicit in UX (UTM precedent: https://docs.getutm.app/basics/basics/, https://www.qemu.org/docs/master/devel/index-tcg.html).
5. **GPL boundary discipline.** Any accidental linking (static or dynamic) of QEMU/GPL code into the app breaks the licensing model; enforce process boundary + CI dependency-license check. (https://www.gnu.org/licenses/gpl-faq.html#MereAggregation)
6. **macOS SLA limits.** macOS-as-guest only on Apple hardware, max 2 additional instances (https://www.apple.com/legal/sla/docs/macOSSequoia.pdf).
7. **Windows-guest 3D immaturity under virtio-gpu** — affects the "daily driver Windows VM" use case on Linux hosts (https://github.com/virtio-win/kvm-guest-drivers-windows).
