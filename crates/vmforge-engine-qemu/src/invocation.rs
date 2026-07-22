//! Builds the QEMU command line for the aarch64 `virt` machine.
//!
//! The invocation is identical across accelerators except for `-accel` and
//! `-cpu` (per the HVF port plan: the QMP control plane, machine type,
//! virtio device set and block layer are accelerator-independent), which is
//! what lets Linux CI exercise the macOS/HVF invocation via `-accel tcg`.

use std::path::{Path, PathBuf};

use vmforge_core::{GuestArch, HvError, VmConfig};

/// QEMU accelerator selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    /// Linux KVM.
    Kvm,
    /// macOS Hypervisor.framework.
    Hvf,
    /// Portable software emulation (used to test the shared invocation
    /// path on hosts without the target accelerator, e.g. Linux CI).
    Tcg,
}

impl Accel {
    pub fn as_str(self) -> &'static str {
        match self {
            Accel::Kvm => "kvm",
            Accel::Hvf => "hvf",
            Accel::Tcg => "tcg",
        }
    }

    /// CPU model paired with this accelerator on the `virt` machine:
    /// `-cpu host` is available "with KVM and HVF only"; TCG uses `max`
    /// (https://www.qemu.org/docs/master/system/arm/virt.html).
    pub fn cpu_model(self) -> &'static str {
        match self {
            Accel::Kvm | Accel::Hvf => "host",
            Accel::Tcg => "max",
        }
    }
}

/// A fully-resolved QEMU invocation for one VM.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub binary: String,
    pub args: Vec<String>,
    pub qmp_socket: PathBuf,
}

impl Invocation {
    /// Start with incoming migration deferred (`-incoming defer`): the VM
    /// waits for a QMP `migrate-incoming` before loading saved RAM state.
    /// Used by `restore` to boot directly from a snapshot's state file.
    pub fn with_incoming_defer(mut self) -> Self {
        self.args.push("-incoming".into());
        self.args.push("defer".into());
        self
    }
}

/// Default UEFI firmware image name for aarch64 guests. A bare filename is
/// resolved by QEMU against its own data directories (`-L`), so this works
/// with distro QEMU on Linux, Homebrew QEMU on macOS, and the QEMU build
/// VMForge will bundle. Override with `VMFORGE_FIRMWARE`.
pub const DEFAULT_AARCH64_FIRMWARE: &str = "edk2-aarch64-code.fd";

/// Locations where distros ship the edk2/AAVMF aarch64 firmware outside
/// QEMU's own data directory (e.g. Debian/Ubuntu `qemu-efi-aarch64`).
const AARCH64_FIRMWARE_PATHS: &[&str] = &[
    "/usr/share/qemu/edk2-aarch64-code.fd",
    "/usr/share/AAVMF/AAVMF_CODE.fd",
    "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
    "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
    "/usr/local/share/qemu/edk2-aarch64-code.fd",
];

fn resolve_aarch64_firmware() -> String {
    if let Ok(fw) = std::env::var("VMFORGE_FIRMWARE") {
        return fw;
    }
    for path in AARCH64_FIRMWARE_PATHS {
        if Path::new(path).exists() {
            return (*path).to_string();
        }
    }
    // Bare name: QEMU resolves it against its own data directories.
    DEFAULT_AARCH64_FIRMWARE.to_string()
}

/// Build the qemu-system-aarch64 command line for `config` per the port
/// plan: machine `virt`, the given accelerator, edk2/AAVMF UEFI firmware,
/// and the same virtio device set as the KVM backend (virtio-blk,
/// virtio-net over user-mode NAT, virtio-serial, virtio-rng).
pub fn build_aarch64_virt(
    config: &VmConfig,
    accel: Accel,
    qmp_socket: &Path,
) -> Result<Invocation, HvError> {
    if config.arch != GuestArch::Aarch64 {
        return Err(HvError::Unsupported {
            backend: accel.as_str(),
            reason: format!(
                "guest arch {:?} not supported by the aarch64 virt invocation (MVP: aarch64 Linux guests only)",
                config.arch
            ),
        });
    }

    let firmware = resolve_aarch64_firmware();

    let mut args: Vec<String> = vec![
        "-name".into(),
        config.name.clone(),
        "-machine".into(),
        "virt".into(),
        "-accel".into(),
        accel.as_str().into(),
        "-cpu".into(),
        accel.cpu_model().into(),
        "-smp".into(),
        config.vcpus.to_string(),
        "-m".into(),
        format!("{}M", config.memory_mib),
        // edk2/AAVMF UEFI firmware, as on Linux/aarch64.
        "-bios".into(),
        firmware,
        // Headless engine: the console/GUI attaches separately.
        "-display".into(),
        "none".into(),
        "-serial".into(),
        "none".into(),
        // QMP control plane; spawn paused so the caller owns the
        // Defined/Stopped -> Running transition via `cont`.
        "-qmp".into(),
        format!("unix:{},server=on,wait=off", qmp_socket.display()),
        "-S".into(),
        // virtio devices matching the KVM backend.
        "-device".into(),
        "virtio-serial-pci".into(),
        "-device".into(),
        "virtio-rng-pci".into(),
        "-netdev".into(),
        "user,id=net0".into(),
        "-device".into(),
        "virtio-net-pci,netdev=net0".into(),
    ];

    for (i, disk) in config.disks.iter().enumerate() {
        args.push("-blockdev".into());
        args.push(format!(
            "driver=qcow2,node-name=disk{i},file.driver=file,file.filename={disk}"
        ));
        args.push("-device".into());
        args.push(format!("virtio-blk-pci,drive=disk{i}"));
    }

    if config.gpu_3d {
        args.push("-device".into());
        args.push("virtio-gpu-pci".into());
    }

    Ok(Invocation {
        binary: std::env::var("VMFORGE_QEMU_AARCH64")
            .unwrap_or_else(|_| "qemu-system-aarch64".to_string()),
        args,
        qmp_socket: qmp_socket.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> VmConfig {
        VmConfig {
            name: "testvm".into(),
            arch: GuestArch::Aarch64,
            vcpus: 2,
            memory_mib: 512,
            disks: vec!["/tmp/disk.qcow2".into()],
            gpu_3d: false,
        }
    }

    fn joined(accel: Accel) -> String {
        build_aarch64_virt(&config(), accel, Path::new("/tmp/qmp.sock"))
            .unwrap()
            .args
            .join(" ")
    }

    #[test]
    fn hvf_invocation_matches_port_plan() {
        let cmd = joined(Accel::Hvf);
        assert!(cmd.contains("-machine virt"));
        assert!(cmd.contains("-accel hvf"));
        assert!(cmd.contains("-cpu host"));
        assert!(cmd.contains("-bios "));
        assert!(cmd.contains("virtio-blk-pci,drive=disk0"));
        assert!(cmd.contains("virtio-net-pci,netdev=net0"));
    }

    #[test]
    fn tcg_differs_only_in_accel_and_cpu() {
        let hvf = joined(Accel::Hvf);
        let tcg = joined(Accel::Tcg);
        assert_eq!(
            hvf.replace("-accel hvf", "-accel tcg")
                .replace("-cpu host", "-cpu max"),
            tcg
        );
    }

    #[test]
    fn non_aarch64_guest_rejected() {
        let cfg = VmConfig {
            arch: GuestArch::X86_64,
            ..config()
        };
        assert!(build_aarch64_virt(&cfg, Accel::Hvf, Path::new("/tmp/q")).is_err());
    }
}
