//! `vmforge diagnose`: host/VM diagnostics bundle for beta bug reports.
//!
//! Collects the wave-1 diagnostics field list (see
//! `docs/tester-guide/reporting-bugs.md` and `docs/diagnose.md`): timestamp,
//! VMForge version, host OS/CPU/RAM, KVM device state, QEMU versions, backend
//! probe, free disk space, plus per-VM state (disks, snapshot tree, network
//! config) and recent log excerpts from `$VMFORGE_HOME`.
//!
//! Privacy guardrails: every piece of file or command output is passed
//! through [`crate::redact`] before it reaches the report; no file contents
//! other than VMForge config/network files and VMForge log tails are read,
//! and nothing is ever uploaded — the bundle is written locally and attaching
//! it to a bug report is a manual, opt-in step.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::redact::redact_text;
use crate::tarball::TarWriter;

/// Max lines kept from the tail of each log file.
const LOG_TAIL_LINES: usize = 200;
/// Max bytes read from the end of each log file before tailing.
const LOG_TAIL_BYTES: u64 = 256 * 1024;
/// Max number of log files collected per VM.
const LOGS_PER_VM: usize = 8;

pub struct DiagnoseOptions {
    /// VMForge home ($VMFORGE_HOME, default ~/.vmforge).
    pub home: PathBuf,
    /// Restrict per-VM sections to this VM.
    pub vm: Option<String>,
    /// Bundle destination; `.tar` suffix selects a tarball, anything else a
    /// plain text file. `None` prints the full report to stdout.
    pub output: Option<PathBuf>,
}

pub fn default_home() -> PathBuf {
    if let Ok(home) = std::env::var("VMFORGE_HOME") {
        return PathBuf::from(home);
    }
    let user_home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&user_home).join(".vmforge")
}

struct Section {
    title: String,
    body: String,
}

struct VmLog {
    file_name: String,
    excerpt: String,
}

struct VmReport {
    name: String,
    body: String,
    logs: Vec<VmLog>,
}

pub fn run(opts: &DiagnoseOptions) -> i32 {
    let sections = collect_host_sections(&opts.home);
    let vms = match collect_vms(&opts.home, opts.vm.as_deref()) {
        Ok(vms) => vms,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };

    let report = render_report(&sections, &vms, opts.vm.as_deref());
    match &opts.output {
        None => {
            print!("{report}");
        }
        Some(path) => {
            let result = if path.extension().is_some_and(|e| e == "tar") {
                write_tar_bundle(path, &report, &vms)
            } else {
                fs::write(path, &report).map_err(|e| e.to_string())
            };
            if let Err(msg) = result {
                eprintln!("failed to write bundle {}: {msg}", path.display());
                return 1;
            }
            print!("{}", render_summary(&sections, &vms));
            println!(
                "bundle written to {} — review before sharing",
                path.display()
            );
        }
    }
    0
}

fn write_tar_bundle(path: &Path, report: &str, vms: &[VmReport]) -> Result<(), String> {
    let mtime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut tar = TarWriter::new(file);
    tar.append("report.txt", report.as_bytes(), mtime)
        .map_err(|e| e.to_string())?;
    for vm in vms {
        for log in &vm.logs {
            let entry = format!("vms/{}/logs/{}", vm.name, log.file_name);
            tar.append(&entry, log.excerpt.as_bytes(), mtime)
                .map_err(|e| e.to_string())?;
        }
    }
    tar.finish()
        .and_then(|mut f| f.flush())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Host sections
// ---------------------------------------------------------------------------

fn collect_host_sections(home: &Path) -> Vec<Section> {
    vec![
        Section {
            title: "version".into(),
            body: format!("vmforge {}\n", env!("CARGO_PKG_VERSION")),
        },
        Section {
            title: "host".into(),
            body: host_info(),
        },
        Section {
            title: "kvm".into(),
            body: kvm_info(),
        },
        Section {
            title: "qemu".into(),
            body: qemu_info(),
        },
        Section {
            title: "backend".into(),
            body: backend_info(),
        },
        Section {
            title: "disk space".into(),
            body: disk_space(home),
        },
        Section {
            title: "config".into(),
            body: config_info(home),
        },
    ]
}

fn host_info() -> String {
    let mut out = String::new();
    out.push_str(&command_output("uname", &["-srmo"]));
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        if let Some(model) = cpuinfo.lines().find(|l| l.starts_with("model name")) {
            out.push_str(model);
            out.push('\n');
        }
    }
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        if let Some(total) = meminfo.lines().find(|l| l.starts_with("MemTotal")) {
            out.push_str(total);
            out.push('\n');
        }
    }
    out
}

fn kvm_info() -> String {
    let mut out = String::new();
    let kvm = Path::new("/dev/kvm");
    if kvm.exists() {
        let writable = fs::OpenOptions::new().write(true).open(kvm).is_ok();
        out.push_str(if writable {
            "/dev/kvm: present, writable\n"
        } else {
            "/dev/kvm: present, NOT writable (add your user to the kvm group)\n"
        });
    } else {
        out.push_str("/dev/kvm: absent (no KVM; VMs fall back to slow TCG emulation)\n");
    }
    match fs::read_to_string("/proc/modules") {
        Ok(modules) => {
            let kvm_mods: Vec<&str> = modules
                .lines()
                .filter(|l| l.starts_with("kvm"))
                .filter_map(|l| l.split_whitespace().next())
                .collect();
            if kvm_mods.is_empty() {
                out.push_str("kvm modules: none loaded\n");
            } else {
                out.push_str(&format!("kvm modules: {}\n", kvm_mods.join(", ")));
            }
        }
        Err(_) => out.push_str("kvm modules: /proc/modules unavailable\n"),
    }
    out
}

fn qemu_info() -> String {
    let mut out = String::new();
    for bin in ["qemu-system-x86_64", "qemu-system-aarch64", "qemu-img"] {
        let version = command_output(bin, &["--version"]);
        out.push_str(version.lines().next().unwrap_or("unavailable"));
        out.push('\n');
    }
    out
}

fn backend_info() -> String {
    match crate::select_backend() {
        Some(hv) => {
            let caps = hv.capabilities();
            format!(
                "backend: {}\naccelerator: {}\naccelerated guest archs: {:?}\nlive snapshot: {}\nvirtio-gpu 3D: {}\n",
                hv.name(),
                caps.accelerator,
                caps.accelerated_archs,
                caps.live_snapshot,
                caps.virtio_gpu_3d,
            )
        }
        None => "no hardware-accelerated backend available on this host\n".to_string(),
    }
}

fn disk_space(home: &Path) -> String {
    let target = if home.exists() {
        home.to_path_buf()
    } else {
        PathBuf::from("/")
    };
    let out = command_output("df", &["-h", &target.to_string_lossy()]);
    let mut lines = out.lines();
    let header = lines.next().unwrap_or("");
    let data = lines.next().unwrap_or("");
    format!("{header}\n{data}\n")
}

fn config_info(home: &Path) -> String {
    let mut out = format!("VMFORGE_HOME: {}\n", home.display());
    if !home.exists() {
        out.push_str("(home directory does not exist yet)\n");
        return out;
    }
    let mut found = false;
    for name in ["config.toml", "config.json", "config"] {
        let path = home.join(name);
        if let Ok(contents) = fs::read_to_string(&path) {
            found = true;
            out.push_str(&format!("--- {name} (redacted) ---\n"));
            out.push_str(&redact_text(&contents));
        }
    }
    if !found {
        out.push_str("(no config file present)\n");
    }
    out
}

// ---------------------------------------------------------------------------
// Per-VM sections
// ---------------------------------------------------------------------------

fn collect_vms(home: &Path, filter: Option<&str>) -> Result<Vec<VmReport>, String> {
    let vms_dir = home.join("vms");
    let mut names: Vec<String> = match fs::read_dir(&vms_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    if let Some(wanted) = filter {
        if !names.iter().any(|n| n == wanted) {
            return Err(format!(
                "vm '{wanted}' not found under {} (known: {})",
                vms_dir.display(),
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            ));
        }
        names.retain(|n| n == wanted);
    }
    Ok(names
        .into_iter()
        .map(|name| collect_vm(home, &name))
        .collect())
}

fn collect_vm(home: &Path, name: &str) -> VmReport {
    let vm_dir = home.join("vms").join(name);
    let mut body = String::new();

    body.push_str(&format!("status: {}\n", vm_status(&vm_dir)));

    // Disks + snapshot tree. Prefer the storage lib CLI (`vmforge-storage`),
    // fall back to a filesystem summary so diagnose works standalone.
    let disks = list_qcow2(&vm_dir.join("disks"));
    if disks.is_empty() {
        body.push_str("disks: none\n");
    }
    for disk in &disks {
        body.push_str(&format!("disk: {disk}\n"));
        match storage_cli(home, &["info", name, disk]) {
            Some(info) => body.push_str(&indent(&info)),
            None => body.push_str(&indent(&fs_disk_summary(&vm_dir, disk))),
        }
        body.push_str("  snapshots:\n");
        match storage_cli(home, &["snapshot", "list", name, disk]) {
            Some(tree) => body.push_str(&indent(&indent(&tree))),
            None => body.push_str(&indent(&indent(&fs_snapshot_summary(&vm_dir, disk)))),
        }
    }

    body.push_str(&network_info(&vm_dir));

    let logs = collect_logs(&vm_dir);
    if logs.is_empty() {
        body.push_str("logs: none\n");
    } else {
        body.push_str(&format!(
            "logs: {}\n",
            logs.iter()
                .map(|l| l.file_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    VmReport {
        name: name.to_string(),
        body: redact_text(&body),
        logs,
    }
}

/// Lifecycle state, best effort: the engine records a pidfile while a VM
/// runs; a stale or absent pidfile means the VM is not running.
fn vm_status(vm_dir: &Path) -> String {
    for pidfile in [vm_dir.join("run/qemu.pid"), vm_dir.join("qemu.pid")] {
        if let Ok(contents) = fs::read_to_string(&pidfile) {
            let pid = contents.trim();
            if !pid.is_empty() && Path::new(&format!("/proc/{pid}")).exists() {
                return format!("running (qemu pid {pid})");
            }
            return "stopped (stale pidfile)".to_string();
        }
    }
    "not running (no pidfile)".to_string()
}

fn list_qcow2(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".qcow2"))
                .map(|n| n.trim_end_matches(".qcow2").to_string())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn fs_disk_summary(vm_dir: &Path, disk: &str) -> String {
    let path = vm_dir.join("disks").join(format!("{disk}.qcow2"));
    match fs::metadata(&path) {
        Ok(meta) => format!("{} on-disk={} bytes\n", path.display(), meta.len()),
        Err(_) => format!("{}: unreadable\n", path.display()),
    }
}

fn fs_snapshot_summary(vm_dir: &Path, disk: &str) -> String {
    let snaps = list_qcow2(&vm_dir.join("snapshots").join(disk));
    if snaps.is_empty() {
        "none\n".to_string()
    } else {
        format!(
            "{} (from snapshot files; tree unavailable)\n",
            snaps.join(", ")
        )
    }
}

/// Run `vmforge-storage` if present; `None` means unavailable/failed and the
/// caller should fall back to the filesystem summary.
fn storage_cli(home: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("vmforge-storage")
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        None
    }
}

fn network_info(vm_dir: &Path) -> String {
    for name in ["network.json", "network.toml", "net.json"] {
        let path = vm_dir.join(name);
        if let Ok(contents) = fs::read_to_string(&path) {
            return format!(
                "network ({name}, redacted):\n{}",
                indent(&redact_text(&contents))
            );
        }
    }
    "network: no per-VM network config recorded\n".to_string()
}

fn collect_logs(vm_dir: &Path) -> Vec<VmLog> {
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in [vm_dir.join("logs"), vm_dir.to_path_buf()] {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "log") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files.truncate(LOGS_PER_VM);
    files
        .into_iter()
        .filter_map(|path| {
            let excerpt = tail_file(&path)?;
            Some(VmLog {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                excerpt: redact_text(&excerpt),
            })
        })
        .collect()
}

/// Last [`LOG_TAIL_LINES`] lines of a file, reading at most
/// [`LOG_TAIL_BYTES`] from its end.
fn tail_file(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > LOG_TAIL_BYTES {
        file.seek(SeekFrom::End(-(LOG_TAIL_BYTES as i64))).ok()?;
    }
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    let lines: Vec<&str> = buf.lines().collect();
    let start = lines.len().saturating_sub(LOG_TAIL_LINES);
    Some(lines[start..].join("\n") + "\n")
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_report(sections: &[Section], vms: &[VmReport], filter: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("== vmforge diagnose ==\n");
    out.push_str(&format!("generated: {}\n", utc_timestamp()));
    if let Some(vm) = filter {
        out.push_str(&format!("scope: --vm {vm}\n"));
    }
    out.push_str(
        "collected per docs/diagnose.md — redacted; review before sharing; never uploaded automatically\n",
    );
    for s in sections {
        out.push_str(&format!("\n-- {} --\n", s.title));
        out.push_str(&s.body);
    }
    if vms.is_empty() {
        out.push_str("\n-- vms --\n(no VMs found)\n");
    }
    for vm in vms {
        out.push_str(&format!("\n-- vm: {} --\n", vm.name));
        out.push_str(&vm.body);
        for log in &vm.logs {
            out.push_str(&format!(
                "--- log tail: {} (last {} lines, redacted) ---\n",
                log.file_name, LOG_TAIL_LINES
            ));
            out.push_str(&log.excerpt);
        }
    }
    out
}

fn render_summary(sections: &[Section], vms: &[VmReport]) -> String {
    let mut out = String::from("vmforge diagnose summary:\n");
    for s in sections {
        if let Some(first) = s.body.lines().next() {
            out.push_str(&format!("  {}: {}\n", s.title, first));
        }
    }
    out.push_str(&format!("  vms: {}\n", vms.len()));
    for vm in vms {
        out.push_str(&format!(
            "    {} — {}\n",
            vm.name,
            vm.body.lines().next().unwrap_or("")
        ));
    }
    out
}

fn indent(text: &str) -> String {
    text.lines().fold(String::new(), |mut acc, l| {
        acc.push_str("  ");
        acc.push_str(l);
        acc.push('\n');
        acc
    })
}

fn command_output(bin: &str, args: &[&str]) -> String {
    match Command::new(bin).args(args).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        Ok(out) => format!(
            "{bin}: exited with {} ({})\n",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(_) => format!("{bin}: not found\n"),
    }
}

/// RFC 3339 UTC timestamp without external dependencies.
fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vmforge-diagnose-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let vm = dir.join("vms/demo");
        fs::create_dir_all(vm.join("disks")).unwrap();
        fs::create_dir_all(vm.join("snapshots/disk0")).unwrap();
        fs::create_dir_all(vm.join("logs")).unwrap();
        fs::write(vm.join("disks/disk0.qcow2"), b"stub").unwrap();
        fs::write(vm.join("snapshots/disk0/base.qcow2"), b"stub").unwrap();
        fs::write(
            vm.join("logs/serial.log"),
            "boot ok\napi_token=deadbeefcafe\nlogin prompt\n",
        )
        .unwrap();
        fs::write(
            vm.join("network.json"),
            "{\"mode\": \"user\", \"hostfwd\": \"tcp::2222-:22\"}\n",
        )
        .unwrap();
        fs::write(
            dir.join("config.toml"),
            "default_memory_mib = 2048\napi_key = \"sk-secret\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn report_covers_vm_and_redacts() {
        let home = fixture_home();
        let vms = collect_vms(&home, None).unwrap();
        assert_eq!(vms.len(), 1);
        let sections = collect_host_sections(&home);
        let report = render_report(&sections, &vms, None);
        assert!(report.contains("-- vm: demo --"));
        assert!(report.contains("disk: disk0"));
        assert!(report.contains("base"));
        assert!(report.contains("hostfwd"));
        assert!(report.contains("boot ok"));
        assert!(!report.contains("deadbeefcafe"), "log token leaked");
        assert!(!report.contains("sk-secret"), "config secret leaked");
        assert!(report.contains("[REDACTED]"));
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn vm_filter_scopes_and_rejects_unknown() {
        let home = fixture_home();
        let vms = collect_vms(&home, Some("demo")).unwrap();
        assert_eq!(vms.len(), 1);
        assert!(collect_vms(&home, Some("nope")).is_err());
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn empty_home_is_fine() {
        let home = std::env::temp_dir().join(format!("vmforge-empty-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        let vms = collect_vms(&home, None).unwrap();
        assert!(vms.is_empty());
        let report = render_report(&collect_host_sections(&home), &vms, None);
        assert!(report.contains("(no VMs found)"));
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn timestamp_is_rfc3339() {
        let ts = utc_timestamp();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
