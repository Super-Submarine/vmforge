# VMForge Storage v0

qcow2 disk & snapshot-tree management for VMForge. A Python library
(`vmforge_storage`) plus a CLI (`vmforge-storage`) that wrap `qemu-img`.
No third-party Python dependencies; requires `qemu-utils` (qemu-img ≥ 6.x).

## Install

```bash
cd storage
pip install -e .[dev]     # dev extras: pytest, ruff
```

## Disk layout conventions (contract for core engine & GUI)

All VMForge storage lives under a single home directory:
`$VMFORGE_HOME`, defaulting to `~/.vmforge`.

```
$VMFORGE_HOME/
├── images/                                # shared, read-only base images
│   └── <image>.qcow2                      # imported via `vmforge-storage import`
└── vms/
    └── <vm>/
        ├── disks/
        │   └── <disk>.qcow2               # ACTIVE writable overlay (attach this to QEMU)
        └── snapshots/
            └── <disk>/
                └── <snapshot>.qcow2       # frozen snapshot layers (read-only, chmod 0444)
```

Rules the engine and GUI can rely on:

1. **Attach point**: the file QEMU should open for a VM disk is always
   `vms/<vm>/disks/<disk>.qcow2` — never a snapshot file directly.
2. **Names**: VM, disk, image, and snapshot names match
   `[A-Za-z0-9][A-Za-z0-9._-]*` (validated; no path traversal possible).
3. **Snapshots are external qcow2 layers.** A snapshot's *parent* is its
   qcow2 backing file (read from image metadata, not a sidecar DB). The
   active disk's backing file identifies the *current* snapshot.
4. **Snapshot files are immutable** (mode 0444). Only the active overlay
   is written to. Base images in `images/` must also never be modified
   while clones reference them.
5. **Offline only**: all snapshot operations here require the VM to be
   powered off. Live/QMP snapshots are a core-engine concern (v1).
6. Everything is derivable from the filesystem + qcow2 metadata; there is
   no separate index that can go stale.

## CLI

```bash
vmforge-storage [--home DIR] [--json] <command> ...

# disks
vmforge-storage create  <vm> <disk> <size> [--preallocation off|metadata|falloc|full] [--cluster-size 64k]
vmforge-storage resize  <vm> <disk> <size> [--shrink]
vmforge-storage import  <src> --name <image> [--format raw|vmdk|vdi|...] [--compress]   # → images/
vmforge-storage import  <src> --vm <vm> --disk <disk> [--format ...]                    # → VM disk
vmforge-storage clone   <base-image-or-path> <vm> <disk> [--size 20G]                   # linked clone
vmforge-storage delete  <vm> <disk> [--force]
vmforge-storage info    <vm> <disk>          # full backing chain
vmforge-storage check   <vm> <disk> [--repair]

# whole-VM backup/restore (portable bundle; see docs/tester-guide/backup-restore.md)
vmforge-storage backup  <vm> <bundle-path> [--snapshot <name>]
vmforge-storage restore <bundle-path> [--as <new-vm>] [--force]

# snapshot tree (offline)
vmforge-storage snapshot create <vm> <disk> <name>
vmforge-storage snapshot list   <vm> <disk>      # renders the tree; * marks current
vmforge-storage snapshot revert <vm> <disk> <name>
vmforge-storage snapshot delete <vm> <disk> <name>
```

`--json` makes every command emit machine-readable JSON — that is the
integration surface for the core engine until the storage API moves in-process.

### Snapshot model (git-like)

`snapshot create` freezes the active overlay as a read-only layer and starts a
fresh overlay on top. `snapshot revert` throws away the active overlay and
starts a new one on top of *any* snapshot — reverting to an ancestor and
snapshotting again creates a **branch**, so history is a tree:

```
base
|-- feature-a
`-- feature-b *        ← * = current base of the active disk
```

`snapshot delete` removes a leaf, or squashes a single-child middle snapshot
into its child (safe `qemu-img rebase`). Deleting a snapshot with multiple
children or the current snapshot is refused.

## Tests

```bash
cd storage && pip install -e .[dev] && pytest
```

All tests are pure `qemu-img`/`qemu-io` operations — **no KVM, no root, no
networking** — so they run in any CI container. CI: `.github/workflows/storage.yml`.

## Demo: boot a VM from a linked clone

`demo/boot_linked_clone.sh` downloads a Cirros cloud image, imports it as a
base image, creates a linked clone, boots it with QEMU (TCG, no KVM needed),
and verifies the guest reaches its login prompt while the base image stays
untouched.
