# Release pipeline v1 — Linux wave 1

Implements the wave-1 packaging decisions from the distribution & packaging
plan (company doc `019f8a7c-5388`): GPG-signed `.deb` + `.AppImage` with
SHA-256 checksums; Flatpak deferred; macOS `.dmg`/notarization out of scope
until wave 2.

## Workflow

`.github/workflows/release.yml` runs on every `v*` tag push and produces, in
a single job (single build → no binary/source drift):

| Artifact | Notes |
|---|---|
| `vmforge_<ver>_amd64.deb` | `packaging/build-deb.sh`; `Depends: qemu-system-x86 (>= 1:6.2)` |
| `vmforge-<ver>-x86_64.AppImage` | `packaging/build-appimage.sh`; pinned appimagetool (1.9.1, checksum-verified) |
| `vmforge-thirdparty-src-<ver>.tar` | `packaging/build-thirdparty-src.sh`; corresponding third-party sources (GAP-4 download-page input) |
| `SHA256SUMS` | checksums of all artifacts |
| `*.asc` | detached ASCII-armored GPG signature per artifact + for `SHA256SUMS` |
| `vmforge-release-signing-key.pub.asc` | public key used for this release |

All artifacts are attached to a **draft** GitHub release for the tag;
publishing (and copying to the gated download page) is a human gate.

## QEMU dependency (wave 1) and GPL §3(a)

Wave 1 **does not redistribute QEMU**. The `.deb` depends on the distro
package `qemu-system-x86`; the `.AppImage` requires host-installed
QEMU/KVM. Because we distribute no QEMU binaries, GPL-2.0 §3 source
obligations for QEMU do not attach to wave-1 artifacts
(<https://www.gnu.org/licenses/gpl-faq.html#UnchangedJustBinary> applies only
when binaries are conveyed).

**Implication:** the moment any artifact bundles QEMU (planned pinned,
pruned source build — single target, pruned firmware/devices, per plan
§2.2–2.3), its exact pinned source tree + patches + build scripts **must**
be added to `vmforge-thirdparty-src-<ver>.tar` in the same workflow run, so
source ships alongside binaries behind the same gate per GPL §3(a)
(<https://www.gnu.org/licenses/gpl-faq.html#SourceAndBinaryOnDifferentSites>).
The bundle already ships the exact vendored Rust crate sources compiled
into the binaries, and `QEMU-NOT-REDISTRIBUTED` states the wave-1 posture.

## Signing key

Production key: GitHub Actions secret `VMFORGE_RELEASE_GPG_PRIVATE_KEY`
(ASCII-armored private key; optional `VMFORGE_RELEASE_GPG_PASSPHRASE`).
Provisioning has been requested from IT Security (Cass) via the company
secrets registry (`VMFORGE_RELEASE_GPG_PRIVATE_KEY`).

**Placeholder fallback (clearly flagged):** if the secret is absent, the
workflow generates an ephemeral 7-day Ed25519 key named
`VMForge PLACEHOLDER release key (DO NOT TRUST)`, emits a workflow warning,
and stamps the draft release notes with a placeholder-signing warning. Such
releases prove pipeline mechanics only and must not be distributed.

## Cutting a release

```sh
git tag v0.1.0 && git push origin v0.1.0
```

Then review the draft release, verify `SHA256SUMS` + signatures, and publish.
Dry runs use pre-release tags (e.g. `v0.1.0-beta.rc`) and stay draft.
