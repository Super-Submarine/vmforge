use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// VM state document schema (see gui/README.md#vm-state-schema).
/// This file is the alpha-stage contract between the GUI and the core
/// engine: today the GUI reads/writes it directly with stub commands;
/// later the core engine CLI emits the same document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStateFile {
    pub schema_version: u32,
    pub vms: Vec<Vm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vm {
    pub id: String,
    pub name: String,
    /// one of: "stopped" | "running" | "paused"
    pub state: String,
    pub cpus: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub disk_path: String,
    /// VNC display exposed by QEMU (-vnc :N), null when not running
    pub vnc_display: Option<i32>,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

fn state_path() -> PathBuf {
    if let Ok(p) = std::env::var("VMFORGE_STATE") {
        return PathBuf::from(p);
    }
    // default: gui/state/vms.json relative to the src-tauri dir in dev
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("state/vms.json");
    p
}

fn load_state() -> Result<VmStateFile, String> {
    let raw = std::fs::read_to_string(state_path()).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save_state(s: &VmStateFile) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(state_path(), raw).map_err(|e| e.to_string())
}

fn now_iso() -> String {
    // coarse ISO-8601 timestamp without pulling in chrono
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix:{}", d.as_secs())
}

#[tauri::command]
fn vm_list() -> Result<VmStateFile, String> {
    load_state()
}

/// One-click VM creation with sensible defaults (Sprint 0 UX finding:
/// Parallels-style instant create is the #1 praised pattern).
#[tauri::command]
fn vm_create(name: Option<String>) -> Result<VmStateFile, String> {
    let mut s = load_state()?;
    let n = s.vms.len() + 1;
    let name = name.unwrap_or_else(|| format!("linux-vm-{n}"));
    let id = format!("vm-{:04}", n);
    s.vms.push(Vm {
        id: id.clone(),
        name,
        state: "stopped".into(),
        cpus: 2,
        memory_mb: 2048,
        disk_gb: 20,
        disk_path: format!("~/.vmforge/disks/{id}.qcow2"),
        vnc_display: None,
        snapshots: vec![],
    });
    save_state(&s)?;
    Ok(s)
}

#[tauri::command]
fn vm_start(id: String) -> Result<VmStateFile, String> {
    let mut s = load_state()?;
    let next = s
        .vms
        .iter()
        .filter_map(|v| v.vnc_display)
        .max()
        .unwrap_or(0)
        + 1;
    for v in s.vms.iter_mut() {
        if v.id == id {
            // stub: real impl will spawn `qemu-system-* -accel kvm ... -vnc :N`
            // and manage it over QMP
            v.state = "running".into();
            v.vnc_display = Some(next);
        }
    }
    save_state(&s)?;
    Ok(s)
}

#[tauri::command]
fn vm_stop(id: String) -> Result<VmStateFile, String> {
    let mut s = load_state()?;
    for v in s.vms.iter_mut() {
        if v.id == id {
            // stub: real impl sends QMP `system_powerdown` / `quit`
            v.state = "stopped".into();
            v.vnc_display = None;
        }
    }
    save_state(&s)?;
    Ok(s)
}

#[tauri::command]
fn vm_snapshot(id: String) -> Result<VmStateFile, String> {
    let mut s = load_state()?;
    for v in s.vms.iter_mut() {
        if v.id == id {
            let sn = v.snapshots.len() + 1;
            // stub: real impl runs `qemu-img snapshot -c` (offline) or QMP
            // `snapshot-save` (live)
            v.snapshots.push(Snapshot {
                id: format!("{}-snap-{}", v.id, sn),
                name: format!("snapshot-{sn}"),
                created_at: now_iso(),
            });
        }
    }
    save_state(&s)?;
    Ok(s)
}

/// Console viewer spike: launch a VNC client against QEMU's -vnc display.
#[tauri::command]
fn open_console(id: String) -> Result<String, String> {
    let s = load_state()?;
    let vm = s
        .vms
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("no such vm: {id}"))?;
    let display = vm
        .vnc_display
        .ok_or_else(|| format!("{} is not running (no VNC display)", vm.name))?;
    let target = format!("127.0.0.1:{}", 5900 + display);
    Command::new("vncviewer")
        .arg(&target)
        .spawn()
        .map_err(|e| format!("failed to launch vncviewer: {e}"))?;
    Ok(target)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            vm_list,
            vm_create,
            vm_start,
            vm_stop,
            vm_snapshot,
            open_console
        ])
        .run(tauri::generate_context!())
        .expect("error while running VMForge GUI");
}
