# Shared folders (host ↔ guest)

> **Status: NOT AVAILABLE — no implementation in the repository.**
> Shared folders v0 (guest tools v1.3, virtiofs/9p) has been announced by
> engineering, but as of this writing **no code or design branch for it has
> been pushed** to `Super-Submarine/vmforge` — there is nothing merged, and
> no open PR. This page documents the status honestly rather than describing
> unmerged (indeed, unpushed) behavior; it will be rewritten with the real
> CLI surface once the feature lands. Do not expect any `vmforge` shared
> folder verbs or flags in current builds — they do not exist.

## What is planned

A v0 shared-folders capability mapping a host directory into the guest,
built on the standard QEMU mechanisms:

- **virtiofs** (preferred; needs `virtiofsd` on the host and a
  `virtiofs`-capable guest kernel), or
- **9p** (`-virtfs`, wider guest compatibility, slower).

Watch the [release notes](../release-notes.md) and the repository's open PRs
for the feature branch.

## Workaround available today

Until shared folders ship, the practical ways to move files host ↔ guest:

1. **Over an SSH port forward** (see [Port forwarding](port-forwarding.md),
   experimental PR #15 — or a manual QEMU `hostfwd` rule on `main`):

   ```sh
   scp -P 2222 myfile user@127.0.0.1:/home/user/
   ```

2. **Bake files into the disk image** before boot with `virt-copy-in`
   (libguestfs-tools), while the VM is powered off.

3. **Guest agent exec** (guest tools, PR #4, experimental): small text
   payloads can be written via `vmforgectl.py --vm <name> exec`.
