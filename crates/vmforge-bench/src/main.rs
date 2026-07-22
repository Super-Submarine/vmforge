//! Engine performance benchmark harness (docs/engine-benchmarks.md).
//!
//! Measures the resume/boot hot paths of the QEMU engine through the
//! `Hypervisor` trait, the same way the product drives it:
//!
//! - `boot_ready_ms`: cold boot — spawn QEMU + QMP handshake + `cont`
//!   until the run state is `running`
//! - `snapshot_save_ms`: live snapshot (pause window + external qcow2
//!   overlay + RAM state to file + resume)
//! - `restore_resume_ms`: instant resume — fresh QEMU with `-incoming
//!   defer`, load RAM state, `cont` -> `running`
//! - `branch_switch_ms`: restore to a *different* snapshot DAG node
//!   (fresh overlays on that node's frozen layers)
//!
//! Runs n iterations (default 5, `--iterations`) and reports medians and
//! p95 as machine-readable JSON plus a markdown summary. With
//! `--baseline`, exits non-zero if any median regresses by more than
//! `--threshold` percent (default 25) — the CI regression guard.
//!
//! Accelerator selection mirrors CI: `VMFORGE_BENCH_ACCEL=kvm|hvf|tcg`,
//! else KVM on aarch64 Linux hosts with /dev/kvm, HVF on macOS with
//! Hypervisor.framework, TCG otherwise (nested-virt friendly: TCG needs
//! no /dev/kvm, matching the wedge-validation methodology in bench/).

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use serde_json::json;
use vmforge_backend_hvf::HvfBackend;
use vmforge_core::{GuestArch, Hypervisor, VmConfig};
use vmforge_engine_qemu::Accel;

struct Args {
    iterations: usize,
    memory_mib: u64,
    json_out: Option<PathBuf>,
    md_out: Option<PathBuf>,
    baseline: Option<PathBuf>,
    threshold_pct: f64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        iterations: 5,
        memory_mib: 256,
        json_out: None,
        md_out: None,
        baseline: None,
        threshold_pct: 25.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("missing value for {name}"));
        match flag.as_str() {
            "--iterations" => {
                args.iterations = value("--iterations")?
                    .parse()
                    .map_err(|e| format!("bad --iterations: {e}"))?
            }
            "--memory-mib" => {
                args.memory_mib = value("--memory-mib")?
                    .parse()
                    .map_err(|e| format!("bad --memory-mib: {e}"))?
            }
            "--json" => args.json_out = Some(PathBuf::from(value("--json")?)),
            "--markdown" => args.md_out = Some(PathBuf::from(value("--markdown")?)),
            "--baseline" => args.baseline = Some(PathBuf::from(value("--baseline")?)),
            "--threshold" => {
                args.threshold_pct = value("--threshold")?
                    .parse()
                    .map_err(|e| format!("bad --threshold: {e}"))?
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    if args.iterations < 1 {
        return Err("--iterations must be >= 1".into());
    }
    Ok(args)
}

fn pick_accel() -> Accel {
    match std::env::var("VMFORGE_BENCH_ACCEL").as_deref() {
        Ok("kvm") => return Accel::Kvm,
        Ok("hvf") => return Accel::Hvf,
        Ok("tcg") => return Accel::Tcg,
        _ => {}
    }
    if cfg!(all(target_os = "linux", target_arch = "aarch64"))
        && std::path::Path::new("/dev/kvm").exists()
    {
        Accel::Kvm
    } else if HvfBackend::is_available() {
        Accel::Hvf
    } else {
        Accel::Tcg
    }
}

fn qemu_version() -> String {
    let binary =
        std::env::var("VMFORGE_QEMU_AARCH64").unwrap_or_else(|_| "qemu-system-aarch64".to_string());
    Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

const METRICS: [&str; 4] = [
    "boot_ready_ms",
    "snapshot_save_ms",
    "restore_resume_ms",
    "branch_switch_ms",
];

#[derive(Default)]
struct Samples {
    boot_ready_ms: Vec<f64>,
    snapshot_save_ms: Vec<f64>,
    restore_resume_ms: Vec<f64>,
    branch_switch_ms: Vec<f64>,
}

impl Samples {
    fn get(&self, metric: &str) -> &[f64] {
        match metric {
            "boot_ready_ms" => &self.boot_ready_ms,
            "snapshot_save_ms" => &self.snapshot_save_ms,
            "restore_resume_ms" => &self.restore_resume_ms,
            "branch_switch_ms" => &self.branch_switch_ms,
            _ => unreachable!(),
        }
    }
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Nearest-rank p95 (with small n this is effectively the max).
fn p95(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    let rank = ((0.95 * n as f64).ceil() as usize).clamp(1, n);
    sorted[rank - 1]
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// One full lifecycle iteration; appends one sample per metric.
fn run_iteration(
    backend: &HvfBackend,
    workdir: &std::path::Path,
    memory_mib: u64,
    iteration: usize,
    samples: &mut Samples,
) -> Result<(), String> {
    let disk = workdir.join(format!("bench-{iteration}.qcow2"));
    let status = Command::new("qemu-img")
        .args(["create", "-q", "-f", "qcow2"])
        .arg(&disk)
        .arg("64M")
        .status()
        .map_err(|e| format!("failed to run qemu-img: {e}"))?;
    if !status.success() {
        return Err("qemu-img create failed".into());
    }

    let config = VmConfig {
        name: format!("bench-{}-{iteration}", std::process::id()),
        arch: GuestArch::Aarch64,
        vcpus: 1,
        memory_mib,
        disks: vec![disk.to_string_lossy().into_owned()],
        gpu_3d: false,
    };
    let vm = backend.create(&config).map_err(|e| e.to_string())?;

    // Cold boot to engine-ready (spawn + QMP handshake + cont -> running).
    let t = Instant::now();
    backend.boot(&vm).map_err(|e| e.to_string())?;
    samples.boot_ready_ms.push(ms(t));

    // Live snapshot of the running VM (RAM state + disk overlay).
    let t = Instant::now();
    let snap = backend.snapshot(&vm, None).map_err(|e| e.to_string())?;
    samples.snapshot_save_ms.push(ms(t));

    // Instant resume: restore from Stopped (fresh process + state load).
    backend.stop(&vm).map_err(|e| e.to_string())?;
    let t = Instant::now();
    backend
        .restore(&vm, snap.clone())
        .map_err(|e| e.to_string())?;
    samples.restore_resume_ms.push(ms(t));

    // Branch: snapshot the restored timeline as a child of `snap`, then
    // switch back to the parent node — a cross-branch restore.
    let _child = backend
        .snapshot(&vm, Some(snap.clone()))
        .map_err(|e| e.to_string())?;
    backend.stop(&vm).map_err(|e| e.to_string())?;
    let t = Instant::now();
    backend.restore(&vm, snap).map_err(|e| e.to_string())?;
    samples.branch_switch_ms.push(ms(t));

    backend.stop(&vm).map_err(|e| e.to_string())?;
    backend.delete(vm).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&disk);
    Ok(())
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn check_baseline(
    baseline_path: &std::path::Path,
    results: &serde_json::Value,
    threshold_pct: f64,
) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(baseline_path)
        .map_err(|e| format!("cannot read baseline {}: {e}", baseline_path.display()))?;
    let baseline: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid baseline JSON: {e}"))?;
    let mut regressions = Vec::new();
    for metric in METRICS {
        let base = baseline["metrics"][metric]["median_ms"].as_f64();
        let cur = results["metrics"][metric]["median_ms"].as_f64();
        let (Some(base), Some(cur)) = (base, cur) else {
            continue;
        };
        let limit = base * (1.0 + threshold_pct / 100.0);
        let delta_pct = (cur - base) / base * 100.0;
        let verdict = if cur > limit { "REGRESSION" } else { "ok" };
        println!(
            "  {metric}: baseline {base:.2} ms -> current {cur:.2} ms ({delta_pct:+.1}%, limit +{threshold_pct:.0}%) {verdict}"
        );
        if cur > limit {
            regressions.push(format!(
                "{metric}: {base:.2} ms -> {cur:.2} ms ({delta_pct:+.1}%)"
            ));
        }
    }
    Ok(regressions)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!(
                "usage: vmforge-bench [--iterations N] [--memory-mib M] \
                 [--json PATH] [--markdown PATH] [--baseline PATH] [--threshold PCT]"
            );
            std::process::exit(2);
        }
    };

    let accel = pick_accel();
    let backend = HvfBackend::with_accel(accel);
    let workdir = std::env::temp_dir().join(format!("vmforge-bench-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).expect("cannot create workdir");

    eprintln!(
        "vmforge-bench: accel={} iterations={} memory={}MiB qemu={:?}",
        accel.as_str(),
        args.iterations,
        args.memory_mib,
        qemu_version()
    );

    // Warm-up iteration (untimed in the report): faults in the firmware
    // image and page cache so iteration 1 is not an outlier.
    let mut warmup = Samples::default();
    if let Err(e) = run_iteration(&backend, &workdir, args.memory_mib, 0, &mut warmup) {
        eprintln!("warm-up iteration failed: {e}");
        std::process::exit(1);
    }

    let mut samples = Samples::default();
    for i in 1..=args.iterations {
        eprintln!("iteration {i}/{}", args.iterations);
        if let Err(e) = run_iteration(&backend, &workdir, args.memory_mib, i, &mut samples) {
            eprintln!("iteration {i} failed: {e}");
            std::process::exit(1);
        }
    }
    let _ = std::fs::remove_dir_all(&workdir);

    let mut metrics = serde_json::Map::new();
    for metric in METRICS {
        let mut sorted = samples.get(metric).to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        metrics.insert(
            metric.to_string(),
            json!({
                "samples_ms": sorted.iter().copied().map(round2).collect::<Vec<_>>(),
                "median_ms": round2(median(&sorted)),
                "p95_ms": round2(p95(&sorted)),
            }),
        );
    }
    let results = json!({
        "schema": "vmforge-engine-bench/1",
        "timestamp_unix": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "accel": accel.as_str(),
        "qemu": qemu_version(),
        "iterations": args.iterations,
        "memory_mib": args.memory_mib,
        "metrics": serde_json::Value::Object(metrics),
    });

    let mut md = String::from(
        "# VMForge engine benchmark\n\n\
         | Metric | Median (ms) | p95 (ms) |\n|---|---:|---:|\n",
    );
    for metric in METRICS {
        md.push_str(&format!(
            "| {metric} | {:.2} | {:.2} |\n",
            results["metrics"][metric]["median_ms"].as_f64().unwrap(),
            results["metrics"][metric]["p95_ms"].as_f64().unwrap(),
        ));
    }
    md.push_str(&format!(
        "\naccel={} · iterations={} · memory={} MiB · {}\n",
        accel.as_str(),
        args.iterations,
        args.memory_mib,
        results["qemu"].as_str().unwrap_or("unknown"),
    ));

    println!("{md}");
    if let Some(path) = &args.json_out {
        std::fs::write(path, format!("{:#}\n", results)).expect("cannot write JSON output");
        eprintln!("wrote {}", path.display());
    }
    if let Some(path) = &args.md_out {
        std::fs::write(path, &md).expect("cannot write markdown output");
        eprintln!("wrote {}", path.display());
    }

    if let Some(baseline) = &args.baseline {
        println!(
            "Comparing against baseline {} (threshold +{}%):",
            baseline.display(),
            args.threshold_pct
        );
        match check_baseline(baseline, &results, args.threshold_pct) {
            Ok(regressions) if regressions.is_empty() => {
                println!("No regressions beyond {}%.", args.threshold_pct)
            }
            Ok(regressions) => {
                eprintln!("Performance regressions detected:");
                for r in &regressions {
                    eprintln!("  {r}");
                }
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("baseline check failed: {e}");
                std::process::exit(2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_p95() {
        let s = [1.0, 2.0, 3.0, 4.0, 100.0];
        assert_eq!(median(&s), 3.0);
        assert_eq!(p95(&s), 100.0);
        let even = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(median(&even), 2.5);
    }

    #[test]
    fn baseline_regression_detection() {
        let dir = std::env::temp_dir().join(format!("vmforge-bench-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let baseline_path = dir.join("baseline.json");
        std::fs::write(
            &baseline_path,
            r#"{"metrics":{"boot_ready_ms":{"median_ms":100.0},"snapshot_save_ms":{"median_ms":100.0},"restore_resume_ms":{"median_ms":100.0},"branch_switch_ms":{"median_ms":100.0}}}"#,
        )
        .unwrap();
        let ok = json!({"metrics": {
            "boot_ready_ms": {"median_ms": 120.0},
            "snapshot_save_ms": {"median_ms": 90.0},
            "restore_resume_ms": {"median_ms": 125.0},
            "branch_switch_ms": {"median_ms": 100.0},
        }});
        assert!(check_baseline(&baseline_path, &ok, 25.0)
            .unwrap()
            .is_empty());
        let bad = json!({"metrics": {
            "boot_ready_ms": {"median_ms": 126.0},
            "snapshot_save_ms": {"median_ms": 90.0},
            "restore_resume_ms": {"median_ms": 100.0},
            "branch_switch_ms": {"median_ms": 100.0},
        }});
        let regressions = check_baseline(&baseline_path, &bad, 25.0).unwrap();
        assert_eq!(regressions.len(), 1);
        assert!(regressions[0].starts_with("boot_ready_ms"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
