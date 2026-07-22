//! Snapshot operations.
//!
//! Live (VM running): `savevm`/`loadvm`/`delvm` via QMP's
//! `human-monitor-command` — full machine state (RAM + devices + disk)
//! stored inside the qcow2.
//! Offline (VM stopped): `qemu-img snapshot` on the qcow2 (disk-only).

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::qmp::QmpClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: String,
    pub tag: String,
    pub vm_size: String,
    pub date: String,
    pub vm_clock: String,
}

// --- live (QMP/HMP) ---

pub fn save_live(qmp: &mut QmpClient, tag: &str) -> Result<()> {
    qmp.hmp(&format!("savevm {tag}"))?;
    Ok(())
}

pub fn restore_live(qmp: &mut QmpClient, tag: &str) -> Result<()> {
    qmp.hmp(&format!("loadvm {tag}"))?;
    Ok(())
}

pub fn delete_live(qmp: &mut QmpClient, tag: &str) -> Result<()> {
    qmp.hmp(&format!("delvm {tag}"))?;
    Ok(())
}

pub fn list_live(qmp: &mut QmpClient) -> Result<Vec<SnapshotInfo>> {
    let out = qmp.hmp("info snapshots")?;
    Ok(parse_snapshot_table(&out))
}

// --- offline (qemu-img) ---

pub fn save_offline(disk: &Path, tag: &str) -> Result<()> {
    qemu_img(&["snapshot", "-c", tag], disk)?;
    Ok(())
}

pub fn restore_offline(disk: &Path, tag: &str) -> Result<()> {
    qemu_img(&["snapshot", "-a", tag], disk)?;
    Ok(())
}

pub fn delete_offline(disk: &Path, tag: &str) -> Result<()> {
    qemu_img(&["snapshot", "-d", tag], disk)?;
    Ok(())
}

pub fn list_offline(disk: &Path) -> Result<Vec<SnapshotInfo>> {
    let out = qemu_img(&["snapshot", "-l", disk.to_string_lossy().as_ref()], None)?;
    Ok(parse_snapshot_table(&out))
}

/// Create a qcow2 disk image of the given size (e.g. "8G").
pub fn create_disk(path: &Path, size: &str) -> Result<()> {
    qemu_img(
        &[
            "create",
            "-f",
            "qcow2",
            path.to_string_lossy().as_ref(),
            size,
        ],
        None,
    )?;
    Ok(())
}

fn qemu_img<'a>(args: &[&str], trailing: impl Into<Option<&'a Path>>) -> Result<String> {
    let mut cmd = Command::new("qemu-img");
    cmd.args(args);
    if let Some(p) = trailing.into() {
        cmd.arg(p);
    }
    let output = cmd.output().map_err(|e| Error::QemuImg(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::QemuImg(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse the snapshot table shared by `qemu-img snapshot -l` and HMP
/// `info snapshots`:
/// ```text
/// Snapshot list:
/// ID        TAG               VM SIZE      DATE                  VM CLOCK
/// 1         base                 214M      2026-07-22 14:00:00   00:00:12.345
/// ```
pub fn parse_snapshot_table(out: &str) -> Vec<SnapshotInfo> {
    out.lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 5 || cols[0] == "ID" || !cols[0].chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(SnapshotInfo {
                id: cols[0].to_string(),
                tag: cols[1].to_string(),
                vm_size: cols[2].to_string(),
                date: format!("{} {}", cols[3], cols.get(4).unwrap_or(&"")),
                vm_clock: cols.get(5).unwrap_or(&"").to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_snapshot_table() {
        let out = "Snapshot list:\n\
                   ID        TAG               VM SIZE                DATE     VM CLOCK\n\
                   1         base                 214M 2026-07-22 14:00:00 00:00:12.345\n\
                   2         after-boot           220M 2026-07-22 14:05:00 00:04:01.000\n";
        let snaps = parse_snapshot_table(out);
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].tag, "base");
        assert_eq!(snaps[1].id, "2");
        assert_eq!(snaps[1].tag, "after-boot");
    }

    #[test]
    fn parses_empty() {
        assert!(parse_snapshot_table("").is_empty());
        assert!(parse_snapshot_table("There is no snapshot available.\n").is_empty());
    }

    #[test]
    fn offline_snapshot_roundtrip_with_qemu_img() {
        // Integration-ish test; skipped when qemu-img is absent.
        if Command::new("qemu-img").arg("--version").output().is_err() {
            eprintln!("qemu-img not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let disk = dir.path().join("d.qcow2");
        create_disk(&disk, "64M").unwrap();
        save_offline(&disk, "s1").unwrap();
        let snaps = list_offline(&disk).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].tag, "s1");
        restore_offline(&disk, "s1").unwrap();
        delete_offline(&disk, "s1").unwrap();
        assert!(list_offline(&disk).unwrap().is_empty());
    }
}
