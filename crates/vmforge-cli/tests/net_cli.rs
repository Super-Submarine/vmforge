//! Integration tests for the experimental `vmforge net` commands.
//! These exercise the CLI binary directly and need no QEMU or KVM.

use std::process::{Command, Output};

fn vmforge(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vmforge"))
        .args(args)
        .output()
        .expect("failed to run vmforge binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn net_args_with_ssh_forward() {
    let out = vmforge(&["net", "args", "--forward", "2222:22"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).trim(),
        "-netdev user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22 -device virtio-net-pci,netdev=net0"
    );
}

#[test]
fn net_args_repeatable_forwards_json() {
    let out = vmforge(&[
        "net",
        "args",
        "--forward",
        "2222:22",
        "--forward",
        "udp:5353:53",
        "--json",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let doc: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    let args: Vec<&str> = doc["qemu_args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        args,
        vec![
            "-netdev",
            "user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=udp:127.0.0.1:5353-:53",
            "-device",
            "virtio-net-pci,netdev=net0",
        ]
    );
    assert_eq!(doc["nic"]["port_forwards"][0]["host_port"], 2222);
}

#[test]
fn net_is_marked_experimental() {
    let out = vmforge(&["net", "args", "--forward", "2222:22"]);
    assert!(stderr(&out).contains("EXPERIMENTAL"));
}

#[test]
fn net_args_rejects_bad_forward_spec() {
    for spec in ["2222", "icmp:1:2", "0:22", "2222:70000"] {
        let out = vmforge(&["net", "args", "--forward", spec]);
        assert!(!out.status.success(), "should reject {spec:?}");
        assert!(stderr(&out).contains("invalid forward spec"));
    }
}

#[test]
fn net_args_rejects_unknown_option() {
    let out = vmforge(&["net", "args", "--bogus"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn ssh_command_from_forward() {
    let out = vmforge(&["net", "ssh-command", "--forward", "2222:22"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "ssh -p 2222 root@127.0.0.1");
}

#[test]
fn ssh_command_explicit_port_and_user() {
    let out = vmforge(&[
        "net",
        "ssh-command",
        "--host-port",
        "2200",
        "--user",
        "alice",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "ssh -p 2200 alice@127.0.0.1");
}

#[test]
fn ssh_command_without_ssh_forward_fails() {
    let out = vmforge(&["net", "ssh-command", "--forward", "8080:80"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no SSH forward"));
}
