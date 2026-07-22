# GUI Alpha User Guide

Verified against the GUI alpha skeleton (PR #7, `gui/`). The GUI is an
**alpha preview**: the VM manager window is real, but Start/Stop/Snapshot are
stubs that mutate a local state file — they do not launch QEMU yet. Only
**Console** touches a real process (it launches a VNC viewer). Treat the GUI
as a UX preview and use the CLI tools for real work (see the
[parity table](#cli--gui-feature-parity) below).

## 1. Install prerequisites & launch

The GUI is a Tauri v2 app (Rust backend + OS-webview frontend). On Linux:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev
cargo install tauri-cli --version '^2'

cd gui/src-tauri
cargo tauri dev        # or: cargo run
```

The manager window opens listing VMs from `gui/state/vms.json`
(override the path with `VMFORGE_STATE=/path/to/vms.json`). If the page shows
"Tauri runtime not detected", you opened `gui/ui/index.html` directly in a
browser — launch via `cargo tauri dev` instead.

## 2. The VM manager window

One table, one row per VM: **Name** (with the VM id), **State**
(`stopped` / `running` / `paused`), **vCPUs**, **Memory**, **Disk**,
**Snapshots** (count only), and per-row **Start / Stop / Snapshot / Console**
buttons. A status line under the table reports the result of the last action.

## 3. Creating a VM

Click **+ Create VM**. There is no wizard: the VM is created immediately with
defaults — 2 vCPU, 2 GiB RAM, 20 GiB qcow2, virtio devices, an auto-generated
name (`linux-vm-<n>`). Customization is edit-after, not wizard-before
(there is no edit UI yet; edit `vms.json` by hand or use the CLI).

Alpha caveat: creation only records the VM in the state file — the qcow2 disk
at `disk_path` is **not** actually created. Create real disks with
`vmforge-storage create <vm> <disk> <size>` (shipped on `main`, see the
[CLI reference](cli-reference.md)).

## 4. Start / Stop

- **Start** marks the VM `running` and assigns it a VNC display number.
  *Stub:* no QEMU process is launched.
- **Stop** marks it `stopped` and clears the VNC display. *Stub:* no ACPI
  powerdown / QMP is involved.

Errors from any action appear in the status line under the table.

## 5. Console

**Console** is enabled only for `running` VMs. It launches an **external VNC
client** (`vncviewer`, e.g. `sudo apt-get install tigervnc-viewer`) against
`127.0.0.1:<5900 + vnc_display>`. This is the one GUI action that works
end-to-end against a real VM today: start a QEMU guest with a VNC display
yourself, then point the GUI at it —

```sh
sudo apt-get install -y qemu-system-x86 tigervnc-viewer
./gui/spike/run_qemu_vnc.sh     # boots a QEMU machine with -vnc :1
# set the VM's "state": "running", "vnc_display": 1 in vms.json, click Console
```

If Console fails with `failed to launch vncviewer`, install a VNC client.
An embedded in-window console (noVNC over QEMU's websocket VNC) is tracked
for beta — not shipped.

## 6. Snapshots in the GUI

**Snapshot** appends a snapshot entry (`snapshot-<n>`) to the VM's flat
snapshot list; the table shows only the count. There is **no snapshot-tree
view in the GUI yet**: no branch/restore/delete, no naming, no tree
visualization. The stub does not touch the disk.

CLI fallback — the full snapshot tree shipped on `main` as `vmforge-storage`
(offline snapshots, VM powered off):

```sh
vmforge-storage snapshot create <vm> <disk> <name>   # freeze current state
vmforge-storage tree <vm> <disk>                     # show the tree (* = current base)
vmforge-storage snapshot revert <vm> <disk> <name>   # branch from a snapshot
vmforge-storage snapshot delete <vm> <disk> <name>   # delete leaf/single-child
```

## 7. Settings & networking panels

**Not implemented.** There is no settings dialog and no networking panel in
the alpha GUI. CLI fallbacks:

- VM hardware: edit `vms.json` fields (`cpus`, `memory_mb`, `disk_gb`) by hand.
- Disks: `vmforge-storage create/resize/import/clone/delete` (on `main`).
- Networking: user-mode NAT + port forwards via `vmforge-net` (PR #2, not yet
  merged) — see the [CLI reference](cli-reference.md#in-flight-not-on-main-yet).

## CLI ↔ GUI feature parity

| Feature | GUI alpha | CLI today | Notes / fallback |
|---|---|---|---|
| List VMs | ✅ manager table (from `vms.json`) | ⏳ `vmforge list` (engine PR #3) | GUI reads the state file directly |
| Create VM | ⚠️ one-click, state-file only (no disk created) | ✅ disk: `vmforge-storage create`; ⏳ VM: `vmforge create` (PR #3) | |
| Start / Stop VM | ⚠️ stub (state flag only, no QEMU) | ⏳ `vmforge start/stop` (PR #3); today: `qa/smoke/smoke_test.sh` | |
| Console | ✅ launches external `vncviewer` | n/a (`vncviewer 127.0.0.1:590N`) | needs a real QEMU `-vnc :N` guest |
| Snapshot create | ⚠️ stub (flat list entry, disk untouched) | ✅ `vmforge-storage snapshot create` (offline) | live snapshots: engine PR #3 |
| Snapshot tree / branch / restore / delete | ❌ not in GUI | ✅ `vmforge-storage tree` / `snapshot revert` / `snapshot delete` | |
| Disk resize / import / clone | ❌ | ✅ `vmforge-storage resize/import/clone` | |
| Settings panel | ❌ | edit `vms.json` / `vmforge-storage` | |
| Networking (NAT, port forwards) | ❌ | ⏳ `vmforge-net` (PR #2): NAT args + QMP hostfwd hot-add/remove | bridged/TAP is design-only |
| Guest tools (guest IP, exec, graceful shutdown) | ❌ | ⏳ `vmforgectl` (PR #4) | |

Legend: ✅ works · ⚠️ stub/partial · ❌ absent · ⏳ in an open PR, not on `main`.

## Filing GUI bugs

GUI issues are **P3** by default (UX friction) unless they block a golden-path
step (**P2**) or corrupt data (**P1**). Include the status-line error text and
your `vms.json` in the report — see [Reporting bugs](reporting-bugs.md).
