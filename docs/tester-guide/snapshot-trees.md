# Working with snapshot trees (git-like snapshots)

VMForge snapshots form a **tree**, not a linear timeline: you can snapshot,
revert to any earlier snapshot, and continue from there — creating a branch —
without losing the other line of history. This is the storage v1 surface
(merged in PR #5) exposed by the `vmforge-storage` CLI, and it is **stable**
under the wave-1 CLI freeze.

All commands below are offline operations: **the VM must be powered off.**
(Live snapshots of a running VM are exercised by the QA smoke suite and land
as first-class CLI verbs at M1.)

## Setup

```sh
cd storage && pip install -e .        # requires qemu-utils (qemu-img >= 6.x)
```

Storage lives under `$VMFORGE_HOME` (default `~/.vmforge`):

```
$VMFORGE_HOME/
├── images/                     # shared, read-only base images
└── vms/<vm>/
    ├── disks/<disk>.qcow2      # ACTIVE writable overlay (what QEMU opens)
    └── snapshots/<disk>/       # frozen snapshot layers (read-only)
```

## The model in one paragraph

The active disk is a qcow2 copy-on-write overlay. `snapshot create` freezes
that overlay as a read-only layer and starts a fresh empty overlay on top of
it. `snapshot revert` throws the active overlay away and starts a new one on
top of *any* snapshot in the tree. A snapshot's parent is its qcow2 backing
file — everything is derived from the filesystem plus qcow2 metadata, so
there is no index that can go stale.

## Create snapshots

```sh
vmforge-storage create dev root 10G                 # a fresh 10G disk
vmforge-storage snapshot create dev root clean-install
# ... boot the VM, install packages, power it off ...
vmforge-storage snapshot create dev root configured
```

## Inspect the tree

```sh
vmforge-storage snapshot list dev root      # or: vmforge-storage tree dev root
```

```
clean-install
`-- configured *        <- * = what the active disk is currently based on
```

Add `--json` for machine-readable output; `vmforge-storage info dev root`
shows the full qcow2 backing chain.

## Branch: revert and diverge

Reverting discards the active (unsnapshotted) state and re-bases the disk on
the snapshot you name. Snapshotting again after a revert creates a branch:

```sh
vmforge-storage snapshot revert dev root clean-install   # back to pristine
# ... try a different setup, power off ...
vmforge-storage snapshot create dev root experiment
vmforge-storage snapshot list dev root
```

```
clean-install
|-- configured
`-- experiment *
```

Both branches remain intact; revert between them at any time:

```sh
vmforge-storage snapshot revert dev root configured
```

> **Careful:** `snapshot revert` discards any changes made since the last
> snapshot. Snapshot first if you want to keep them.

## Delete snapshots

```sh
vmforge-storage snapshot delete dev root experiment
```

Allowed deletions:

- a **leaf** snapshot — removed outright;
- a **middle snapshot with exactly one child** — safely squashed into its
  child (`qemu-img rebase` under the hood).

Refused (exit 1, JSON error on stderr): deleting a snapshot with multiple
children, or the snapshot the active disk is currently based on (revert
elsewhere first). `vmforge-storage delete <vm> <disk> --force` deletes a disk
together with its whole snapshot tree.

## Verify disk health

```sh
vmforge-storage check dev root            # exit 3 if corruptions/leaks found
vmforge-storage check dev root --repair
```

Any snapshot-restore failure is a **P1** bug — see
[Reporting bugs](reporting-bugs.md).

Full flag/exit-code details: [CLI reference](cli-reference.md#2-vmforge-storage-qcow2-disks--snapshot-trees).
