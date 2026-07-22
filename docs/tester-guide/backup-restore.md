# Backing up & restoring a whole VM

> Verbs `vmforge-storage backup` / `vmforge-storage restore` are **new in
> storage v1.2** (additive to the frozen wave-1 surface, per
> `docs/cli-freeze-v1.0-beta.md`). Treat them as **experimental** until they
> are promoted in the freeze doc — flag names may still change.

Beta builds can lose data. Before trying anything risky, take a full backup of
your VM — disk contents, the entire snapshot tree (all branches), and the VM
config — as a single portable file you can copy to another disk or machine.

## Back up

```sh
vmforge-storage backup <vm> <bundle-path>
```

- `<bundle-path>` ending in `.tar.gz` / `.tgz` gzip-compresses the bundle;
  any other extension (e.g. `.tar`) writes a plain tar.
- The bundle contains the active disk overlay(s), every snapshot layer, any
  shared base image the chain depends on (linked clones), your VM config
  files, and a `manifest.json` with a schema version and a SHA-256 checksum
  for every file.
- Refuses to overwrite an existing bundle file.

Export only part of the history:

```sh
vmforge-storage backup <vm> <bundle-path> --snapshot <name>
```

This includes just the chain from the root up to `<name>` (other branches and
the current working state are left out). On restore, `<name>` becomes the
current snapshot with a fresh working overlay on top. `--snapshot` requires a
single-disk VM.

## Restore

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
  corrupted or tampered bundle is rejected with exit code 1 and a
  `{"error": ...}` JSON object on stderr, and nothing is left behind.
- Shared base images (`images/*.qcow2`) already present in the destination
  home are reused if their checksum matches the bundled copy; a mismatch
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
