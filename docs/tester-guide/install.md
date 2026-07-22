# Installing VMForge from signed release artifacts

Wave-1 releases ship as GPG-signed Linux artifacts built by the release
pipeline (`.github/workflows/release.yml`, see
[`docs/release-pipeline.md`](../release-pipeline.md)). Each release for a tag
`v<ver>` contains:

| File | What it is |
|---|---|
| `vmforge_<ver>_amd64.deb` | Debian/Ubuntu package (`Depends: qemu-system-x86 (>= 1:6.2)`) |
| `vmforge-<ver>-x86_64.AppImage` | Portable AppImage (requires host-installed QEMU/KVM) |
| `vmforge-thirdparty-src-<ver>.tar` | Corresponding third-party sources |
| `SHA256SUMS` | SHA-256 checksums of all artifacts |
| `<artifact>.asc` | Detached ASCII-armored GPG signature, one per artifact + one for `SHA256SUMS` |
| `vmforge-release-signing-key.pub.asc` | Public key used to sign this release |

Wave 1 does **not** bundle QEMU — both install paths use your distro's QEMU
(see [Quickstart §1](quickstart-linux.md#1-prerequisites) for the packages).

## 1. Download

From the GitHub release page for the tag, download the artifact you want,
`SHA256SUMS`, `SHA256SUMS.asc`, the artifact's own `.asc`, and
`vmforge-release-signing-key.pub.asc` into one directory.

## 2. Verify — do this before installing

### Trusting the signing key

> **TODO (placeholder state):** the production release signing key
> (`VMFORGE_RELEASE_GPG_PRIVATE_KEY`, provisioning requested from IT Security)
> is **not yet provisioned**, and its public key is **not yet committed to
> this repo or published on the download page**. Until then, tagged builds
> fall back to an ephemeral key literally named
> `VMForge PLACEHOLDER release key (DO NOT TRUST)` — such builds prove
> pipeline mechanics only and **must not be installed or distributed**. Once
> the real key lands, this section will be updated with the committed key
> path and its fingerprint; verify the fingerprint out-of-band against the
> download page before trusting it.

Import the release public key and check whose key it is:

```sh
gpg --import vmforge-release-signing-key.pub.asc
gpg --show-keys vmforge-release-signing-key.pub.asc   # inspect uid + fingerprint
```

If the uid contains `PLACEHOLDER` / `DO NOT TRUST`, **stop** — this is not a
distributable build.

### Verify signatures and checksums

```sh
# 1. The checksum manifest is authentic:
gpg --verify SHA256SUMS.asc SHA256SUMS

# 2. The artifact matches the manifest:
sha256sum --check --ignore-missing SHA256SUMS

# 3. (Optional, belt-and-braces) the artifact's own detached signature:
gpg --verify vmforge_<ver>_amd64.deb.asc vmforge_<ver>_amd64.deb
# or
gpg --verify vmforge-<ver>-x86_64.AppImage.asc vmforge-<ver>-x86_64.AppImage
```

All three must succeed (`Good signature`, `OK`). A `BAD signature` or checksum
mismatch means the file is corrupt or tampered with — delete it, re-download,
and if it still fails, report it immediately per
[Reporting bugs](reporting-bugs.md) (do **not** install it).

`gpg` may warn `This key is not certified with a trusted signature` — expected
unless you have signed the key yourself; trust comes from the out-of-band
fingerprint check above.

## 3a. Install the .deb (Debian/Ubuntu)

```sh
sudo apt install ./vmforge_<ver>_amd64.deb   # resolves the qemu-system-x86 dependency
vmforge info                                  # sanity check — see the CLI reference
```

Uninstall with `sudo apt remove vmforge`.

## 3b. Install the .AppImage (any distro)

```sh
chmod +x vmforge-<ver>-x86_64.AppImage
./vmforge-<ver>-x86_64.AppImage info
```

The AppImage does not install anything system-wide; QEMU/KVM must already be
installed on the host ([Quickstart §1](quickstart-linux.md#1-prerequisites)).
If it fails to start with a FUSE error, install your distro's `libfuse2`
package or run it with `--appimage-extract-and-run`.

## 4. Next steps

Verify KVM and run your first VM per the
[Linux quickstart](quickstart-linux.md#2-verify-kvm-do-this-before-anything-else).
Building from source (the path most wave-1 docs assume until a release is
published) is covered in [Quickstart §3](quickstart-linux.md#3-build-and-check-the-backend).
