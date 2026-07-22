//! QEMU process construction and launch.

use std::path::Path;
use std::process::Command;

use crate::config::VmConfig;
use crate::error::{Error, Result};

pub const QEMU_BINARY: &str = "qemu-system-x86_64";

/// Pick the best accelerator: KVM when /dev/kvm is usable, TCG otherwise
/// (e.g. CI runners without nested virtualization).
pub fn choose_accel() -> &'static str {
    if kvm_available() {
        "kvm"
    } else {
        "tcg"
    }
}

pub fn kvm_available() -> bool {
    Path::new("/dev/kvm").exists()
}

/// Build the QEMU command line for a VM.
///
/// * QMP unix socket at `qmp_sock` (server, non-blocking)
/// * daemonized with a pidfile so the CLI returns immediately
/// * virtio disk + virtio NIC (user-mode networking)
/// * serial console captured to `serial_log`
pub fn build_command(
    config: &VmConfig,
    qmp_sock: &Path,
    pidfile: &Path,
    serial_log: &Path,
    accel: &str,
) -> Command {
    let mut cmd = Command::new(QEMU_BINARY);
    cmd.arg("-name").arg(&config.name);
    cmd.arg("-machine").arg(format!("q35,accel={accel}"));
    if accel == "kvm" {
        cmd.arg("-cpu").arg("host");
    }
    cmd.arg("-smp").arg(config.cpus.to_string());
    cmd.arg("-m").arg(format!("{}M", config.memory_mib));
    cmd.arg("-drive").arg(format!(
        "file={},if=virtio,format=qcow2",
        config.disk.display()
    ));
    if let Some(iso) = &config.iso {
        cmd.arg("-cdrom").arg(iso);
        cmd.arg("-boot").arg("order=cd");
    }
    cmd.arg("-netdev").arg("user,id=net0");
    cmd.arg("-device").arg("virtio-net-pci,netdev=net0");
    cmd.arg("-qmp")
        .arg(format!("unix:{},server,nowait", qmp_sock.display()));
    cmd.arg("-serial")
        .arg(format!("file:{}", serial_log.display()));
    cmd.arg("-display").arg("none");
    cmd.arg("-daemonize");
    cmd.arg("-pidfile").arg(pidfile);
    cmd.args(&config.extra_args);
    cmd
}

/// Launch QEMU (daemonized) and wait for it to fork off successfully.
pub fn launch(mut cmd: Command) -> Result<()> {
    let output = cmd.output().map_err(|source| Error::Launch {
        binary: QEMU_BINARY.into(),
        source,
    })?;
    if !output.status.success() {
        return Err(Error::EarlyExit(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn args_of(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn command_line_includes_core_devices() {
        let cfg = VmConfig {
            name: "alpine".into(),
            cpus: 2,
            memory_mib: 512,
            disk: PathBuf::from("/tmp/a.qcow2"),
            iso: Some(PathBuf::from("/tmp/alpine.iso")),
            extra_args: vec!["-nodefaults".into()],
        };
        let cmd = build_command(
            &cfg,
            Path::new("/run/qmp.sock"),
            Path::new("/run/pid"),
            Path::new("/run/serial.log"),
            "tcg",
        );
        let args = args_of(&cmd).join(" ");
        assert!(args.contains("q35,accel=tcg"));
        assert!(args.contains("-smp 2"));
        assert!(args.contains("-m 512M"));
        assert!(args.contains("file=/tmp/a.qcow2,if=virtio,format=qcow2"));
        assert!(args.contains("-cdrom /tmp/alpine.iso"));
        assert!(args.contains("unix:/run/qmp.sock,server,nowait"));
        assert!(args.contains("-daemonize"));
        assert!(args.contains("-nodefaults"));
        assert!(!args.contains("-cpu host")); // tcg: no host cpu
    }

    #[test]
    fn kvm_adds_host_cpu() {
        let cfg = VmConfig {
            name: "v".into(),
            cpus: 1,
            memory_mib: 256,
            disk: PathBuf::from("/d.qcow2"),
            iso: None,
            extra_args: vec![],
        };
        let cmd = build_command(
            &cfg,
            Path::new("/q"),
            Path::new("/p"),
            Path::new("/s"),
            "kvm",
        );
        let args = args_of(&cmd).join(" ");
        assert!(args.contains("accel=kvm"));
        assert!(args.contains("-cpu host"));
        assert!(!args.contains("-cdrom"));
    }
}
