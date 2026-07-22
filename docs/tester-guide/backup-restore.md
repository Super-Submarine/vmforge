# Backup & Restore — whole-VM portability

> **Status: NOT YET MERGED.** `vmforge-storage backup` / `restore` are
> Storage v1.2, in review on PR
> [#28](https://github.com/Super-Submarine/vmforge/pull/28)
> (`devin/1784739282-storage-backup-restore`). They are **additive** to the
> frozen wave-1 surface and **experimental** until promoted in
> [`docs/cli-freeze-v1.0-beta.md`](../cli-freeze-v1.0-beta.md) — verb and flag
> shapes below are as implemented on the PR branch and may still change.
> Everything else in this page (`vmforge-storage` v1 verbs) is on `main`.

Beta builds can lose data. Before a risky experiment — or to move a VM to
another machine — export the whole VM (disk contents, the entire snapshot
tree with all branches, and the VM config) into one portable bundle file.

## Back up a VM

```sh
vmforge-storage backup <vm> <bundle-path>
```

- A `<bundle-path>` ending in `.tar.gz`/`.tgz` gzip-compresses the bundle;
  any other extension (e.g. `.tar`) writes plain tar.
- The bundle contains the active disk overlay(s), every snapshot layer, any
  shared base image the chain depends on (linked clones), the VM config
  files, and a `manifest.json` with a schema version and a SHA-256 checksum
  for every file.
- Refuses to overwrite an existing bundle file.

Export only part of the history (single-disk VMs only):

```sh
vmforge-storage backup <vm> <bundle-path> --snapshot <name>
```

This includes just the chain from the root up to `<name>`; other branches and
the current working state are left out. On restore, `<name>` becomes the
current snapshot with a fresh working overlay on top.

## Restore a VM

```sh
vmforge-storage restore <bundle-path> [--as <new-vm>] [--force]
```

- Recreates the VM under `$VMFORGE_HOME` (or `--home DIR`) with the snapshot
  tree intact — branches, current-snapshot marker, and config included.
- `--as <new-vm>` restores under a different name (e.g. to compare against
  the original side by side).
- **Refuses to overwrite an existing VM** of the same name unless you pass
  `--force`. With `--force` the existing VM is replaced; there is no undo.
- Every file is verified against the manifest checksums during extraction,
  and the restored disk chain must pass the same health pass as
  `vmforge-storage check` before the restore is declared successful. A
  corrupted or tampered bundle is rejected (exit code 1, `{"error": ...}`
  JSON on stderr) and nothing is left behind.
- Shared base images (`images/*.qcow2`) already present in the destination
  home are reused when their checksum matches the bundled copy; a mismatch
  aborts the restore rather than overwriting a shared image.

## Typical flows

Move a VM to another machine:

```sh
# machine A
vmforge-storage backup dev-vm /media/usb/dev-vm.tar.gz
# machine B
vmforge-storage restore /media/usb/dev-vm.tar.gz
```

Safety net before a risky experiment:

```sh
vmforge-storage backup dev-vm ~/backups/dev-vm-$(date +%F).tar
# ... things go wrong ...
vmforge-storage restore ~/backups/dev-vm-2026-07-22.tar --force
```

Both verbs honor the wave-1 CLI conventions: `--home`, `--json` (one JSON
document on stdout), `--contract-version`, and exit codes 0 (success),
1 (storage/backend error, JSON error on stderr), 2 (usage error).

## Until PR #28 merges: manual backup

The v1 verbs on `main` are enough for a manual (less convenient) backup:
stop the VM, then copy the whole VM directory and any base images the chain
depends on:

```sh
vmforge-storage info <vm> <disk>            # shows the full backing chain
cp -a ~/.vmforge/vms/<vm> /media/usb/       # disks + snapshot layers + config
cp -a ~/.vmforge/images /media/usb/         # only if the chain references images/
```

Restoring is the reverse copy. Note this preserves absolute backing-file
paths, so it is only safe when restoring to the same `$VMFORGE_HOME` path —
the portable bundle in PR #28 exists precisely to remove that limitation.
