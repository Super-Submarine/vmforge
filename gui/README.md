# VMForge GUI (alpha skeleton)

Cross-platform desktop GUI for VMForge: a VM manager window plus a console
viewer spike that connects to QEMU's `-vnc` display.

## Stack choice: Tauri (v2)

We chose **Tauri** over Electron and Qt:

- **vs Electron**: Tauri uses the OS webview (WebKitGTK on Linux, WKWebView on
  macOS, WebView2 on Windows) instead of bundling Chromium, giving ~10x
  smaller binaries and far lower RAM overhead — this matters for a
  virtualization product whose users want host RAM for their VMs, not the
  manager UI. The backend is Rust, which is also the natural language for the
  future QMP client / core-engine integration (no IPC through Node).
- **vs Qt**: Qt gives native widgets but has LGPL/commercial licensing
  friction for a proprietary-friendly product, a much steeper C++ toolchain,
  and slower UI iteration than web tech. Tauri is MIT/Apache-2.0.
- Tauri targets exactly our three host platforms (Linux/macOS/Windows) with
  one codebase and a first-class `command` IPC layer that maps cleanly onto
  "GUI calls core engine".

Trade-off noted: webview differences across platforms (WebKitGTK vs WebView2)
need cross-platform UI testing; acceptable at alpha stage.

## Layout

```
gui/
├── README.md            this file
├── src-tauri/           Rust backend (Tauri commands = engine stubs)
├── ui/                  plain HTML/CSS/JS frontend (no bundler needed yet)
├── state/vms.json       mock VM state file (schema below)
└── spike/               console viewer spike (QEMU VNC)
```

## Build & run

Prerequisites (Linux): Rust ≥1.77, and Tauri v2 system deps:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev
cargo install tauri-cli --version '^2'
```

Run the dev app:

```bash
cd gui/src-tauri
cargo tauri dev        # or: cargo run
```

The manager window lists VMs from `gui/state/vms.json` (override the path
with `VMFORGE_STATE=/path/to/vms.json`). The **Create VM**, **Start**,
**Stop**, **Snapshot** buttons drive stub Tauri commands that mutate that
JSON file — the same commands will later shell out to the core engine
(QMP/`qemu-img`) without UI changes. **Console** launches `vncviewer`
against the VM's `vnc_display`.

### One-click VM creation (Sprint 0 UX)

Sprint 0 research found the most-praised Parallels pattern is one-click VM
creation with sensible defaults instead of a multi-step wizard. **Create VM**
therefore immediately creates a VM with defaults (2 vCPU, 2 GiB RAM, 20 GiB
qcow2, virtio devices, auto-generated name) — customization is an edit-after,
not a wizard-before.

## VM state schema

`gui/state/vms.json` is the alpha contract between GUI and core engine.
The engine CLI will later emit this same document (e.g. `vmforge list --json`).

```jsonc
{
  "schema_version": 1,          // integer, bump on breaking changes
  "vms": [
    {
      "id": "vm-0001",          // stable unique id (string)
      "name": "ubuntu-24.04-dev",
      "state": "running",       // "stopped" | "running" | "paused"
      "cpus": 4,                 // vCPU count (integer ≥ 1)
      "memory_mb": 8192,         // RAM in MiB
      "disk_gb": 64,             // primary disk size in GiB
      "disk_path": "~/.vmforge/disks/vm-0001.qcow2",  // qcow2 path
      "vnc_display": 1,          // QEMU -vnc :N display; null when not running
                                 //   (VNC port = 5900 + N)
      "snapshots": [
        {
          "id": "vm-0001-snap-1",
          "name": "clean-install",
          "created_at": "2026-07-20T10:12:00Z"   // ISO-8601
        }
      ]
    }
  ]
}
```

## Console viewer spike (QEMU VNC)

Approach: at alpha we **launch an external VNC client** against QEMU's
`-vnc :N` display — simplest correct thing, works with any client. The
follow-up (tracked for beta) is embedding a JS RFB client (noVNC) in the
Tauri webview via QEMU's built-in websocket VNC (`-vnc :N,websocket=on`),
which removes the external dependency and gives an in-window console.

Reproduce the spike:

```bash
sudo apt-get install -y qemu-system-x86 tigervnc-viewer
./gui/spike/run_qemu_vnc.sh      # starts a local QEMU guest with -vnc :1
vncviewer 127.0.0.1:5901         # or click "Console" on a running VM in the GUI
```

`gui/spike/run_qemu_vnc.sh` boots a minimal QEMU machine (no disk needed —
the SeaBIOS/iPXE boot screen is enough to prove the VNC path end-to-end).
