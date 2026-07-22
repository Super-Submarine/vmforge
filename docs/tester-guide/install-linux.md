# Installing VMForge on Linux (.deb / .AppImage)

Wave-1 release artifacts are built by the tag-triggered release pipeline
(`.github/workflows/release.yml`, PR #18) and attached to GitHub releases at
https://github.com/Super-Submarine/vmforge/releases. Each release contains:

| Artifact | What it is |
|---|---|
| `vmforge_<version>_amd64.deb` | Debian/Ubuntu package (installs `/usr/bin/vmforge`; depends on `qemu-system-x86 >= 1:6.2`) |
| `vmforge-<version>-amd64.AppImage` | Portable single-file binary for any modern x86-64 Linux |
| `SHA256SUMS` | SHA-256 checksums of all artifacts |
| `<artifact>.asc` | Detached GPG signature, one per artifact (incl. `SHA256SUMS.asc`) |
| `vmforge-release-signing-key.pub.asc` | The public signing key used for this release |
| `vmforge-thirdparty-src-<version>.tar` | Third-party source bundle (license compliance) |

> **Signing-key status:** the permanent VMForge release GPG key is still being
> provisioned by IT Security. Until it lands, releases are signed with an
> **ephemeral placeholder key clearly flagged "DO NOT TRUST"** in the release
> notes. The verification steps below are the same either way; once the
> permanent key is provisioned, its public key will be committed to this repo
> and published on the download page — verify against *that* copy, not only
> the one attached to the release.

## 1. Verify the download (do this first)

```sh
# 1. Import the release public key (from the repo/download page once provisioned;
#    the release-attached copy is a fallback)
gpg --import vmforge-release-signing-key.pub.asc

# 2. Verify the checksum file's signature
gpg --verify SHA256SUMS.asc SHA256SUMS

# 3. Verify the artifact's checksum
sha256sum --check --ignore-missing SHA256SUMS

# (optional) verify the artifact's own detached signature too
gpg --verify vmforge_<version>_amd64.deb.asc vmforge_<version>_amd64.deb
```

`gpg --verify` must report `Good signature`. A `WARNING: This key is not
certified with a trusted signature` is expected until you have verified the
key fingerprint against the one published on the download page and marked it
trusted. If verification **fails**, do not install — file a **P1** bug.

## 2a. Install the .deb (Debian/Ubuntu)

```sh
sudo apt install ./vmforge_<version>_amd64.deb   # resolves the qemu-system-x86 dependency
vmforge info                                      # sanity check
```

Uninstall with `sudo apt remove vmforge`.

## 2b. Or run the AppImage (any distro)

The AppImage bundles the `vmforge` binary but **not QEMU** — install QEMU from
your distro first:

```sh
sudo apt install -y qemu-system-x86 qemu-utils    # Debian/Ubuntu
sudo dnf install -y qemu-system-x86 qemu-img      # Fedora

chmod +x vmforge-<version>-amd64.AppImage
./vmforge-<version>-amd64.AppImage info
```

Optionally move it onto your `PATH`:

```sh
sudo install -m 0755 vmforge-<version>-amd64.AppImage /usr/local/bin/vmforge
```

## 3. After installing

Verify KVM and run your first VM: continue with the
[Linux quickstart](quickstart-linux.md) from step 2 (you can skip the
build-from-source step). Expected `vmforge info` output and exit codes are in
the [CLI reference](cli-reference.md).

The `vmforge-storage` disk/snapshot CLI is not yet packaged — it is installed
from the repo (`cd storage && pip install -e .`); see
[Working with snapshot trees](snapshot-trees.md).
