//! User-mode (SLIRP) networking: QEMU argv generation and runtime
//! host->guest port forwarding, per `docs/interface-contracts.md` §2.
//!
//! The Python `vmforge_net` v0 package is the reference implementation and
//! test oracle: [`UserNetBackend::qemu_args`] must produce argv identical to
//! `vmforge_net.natgen.build_qemu_args` for the same configuration (N1).
//! Runtime forward add/remove goes through the engine-owned QMP connection
//! via `human-monitor-command` wrapping HMP `hostfwd_add`/`hostfwd_remove`
//! (N2).

use serde::{Deserialize, Serialize};

use crate::HvError;

/// Transport protocol for a port forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    pub fn as_str(&self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

/// A host->guest port forwarding rule (QEMU `hostfwd`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForward {
    pub proto: Proto,
    /// Host bind address; `None` binds 127.0.0.1 (loopback only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_ip: Option<String>,
    pub host_port: u16,
    pub guest_port: u16,
}

impl PortForward {
    /// Render as a QEMU hostfwd rule: `proto:hostip:hostport-:guestport`.
    pub fn to_hostfwd(&self) -> String {
        format!(
            "{}:{}:{}-:{}",
            self.proto.as_str(),
            self.host_ip.as_deref().unwrap_or("127.0.0.1"),
            self.host_port,
            self.guest_port
        )
    }

    /// Parse a CLI forward spec. Accepted forms:
    ///
    /// - `HOSTPORT:GUESTPORT`              (tcp, bind 127.0.0.1)
    /// - `PROTO:HOSTPORT:GUESTPORT`        (bind 127.0.0.1)
    /// - `PROTO:HOSTIP:HOSTPORT:GUESTPORT` (explicit bind address)
    pub fn parse(spec: &str) -> Result<Self, HvError> {
        let bad = || {
            HvError::Engine(format!(
                "invalid forward spec '{spec}' (expected [tcp|udp:][HOSTIP:]HOSTPORT:GUESTPORT)"
            ))
        };
        let parts: Vec<&str> = spec.split(':').collect();
        let (proto, host_ip, host_port, guest_port) = match parts.as_slice() {
            [hp, gp] => (Proto::Tcp, None, *hp, *gp),
            [proto, hp, gp] => (Self::parse_proto(proto).ok_or_else(bad)?, None, *hp, *gp),
            [proto, ip, hp, gp] => (
                Self::parse_proto(proto).ok_or_else(bad)?,
                Some((*ip).to_string()),
                *hp,
                *gp,
            ),
            _ => return Err(bad()),
        };
        if let Some(ip) = &host_ip {
            if ip.parse::<std::net::IpAddr>().is_err() {
                return Err(bad());
            }
        }
        let parse_port = |s: &str| s.parse::<u16>().ok().filter(|p| *p != 0);
        Ok(PortForward {
            proto,
            host_ip,
            host_port: parse_port(host_port).ok_or_else(bad)?,
            guest_port: parse_port(guest_port).ok_or_else(bad)?,
        })
    }

    fn parse_proto(s: &str) -> Option<Proto> {
        match s {
            "tcp" => Some(Proto::Tcp),
            "udp" => Some(Proto::Udp),
            _ => None,
        }
    }
}

/// Networking mode for one NIC. M1: user-mode NAT only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    Nat,
}

/// Configuration of one virtual NIC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NicConfig {
    /// netdev id, unique per VM: `net<n>`.
    pub id: String,
    pub mode: NetMode,
    /// QEMU NIC model; default `virtio-net-pci`.
    pub model: String,
    /// `None` = engine assigns a 52:54:00:xx:xx:xx address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
}

impl NicConfig {
    pub fn nat(id: impl Into<String>) -> Self {
        NicConfig {
            id: id.into(),
            mode: NetMode::Nat,
            model: "virtio-net-pci".to_string(),
            mac: None,
            port_forwards: Vec::new(),
        }
    }
}

/// Minimal QMP connection surface the networking backend needs. The engine
/// owns the real socket (invariant N2); tests supply a mock.
pub trait QmpConn {
    /// Execute `human-monitor-command` with `command-line` and return the
    /// HMP text output (empty string on silent success).
    fn human_monitor_command(&mut self, command_line: &str) -> Result<String, HvError>;
}

/// Networking backend: pure argv generation plus runtime forward management.
pub trait NetworkBackend: Send + Sync {
    /// Pure function: QEMU argv fragment for one NIC.
    fn qemu_args(&self, nic: &NicConfig) -> Result<Vec<String>, HvError>;
    /// Add a port forward on a live VM through the engine's QMP connection.
    fn hostfwd_add(
        &self,
        qmp: &mut dyn QmpConn,
        nic_id: &str,
        fwd: &PortForward,
    ) -> Result<(), HvError>;
    /// Remove a port forward on a live VM.
    fn hostfwd_remove(
        &self,
        qmp: &mut dyn QmpConn,
        nic_id: &str,
        fwd: &PortForward,
    ) -> Result<(), HvError>;
}

/// User-mode NAT (SLIRP) backend.
#[derive(Debug, Default)]
pub struct UserNetBackend;

impl UserNetBackend {
    pub fn new() -> Self {
        UserNetBackend
    }

    fn validate(nic: &NicConfig) -> Result<(), HvError> {
        let mut chars = nic.id.chars();
        let head_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        let tail_ok = chars.all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        if !(head_ok && tail_ok) {
            return Err(HvError::Engine(format!("invalid netdev id '{}'", nic.id)));
        }
        if let Some(mac) = &nic.mac {
            let octets: Vec<&str> = mac.split(':').collect();
            let ok = octets.len() == 6
                && octets
                    .iter()
                    .all(|o| o.len() == 2 && o.chars().all(|c| c.is_ascii_hexdigit()));
            if !ok {
                return Err(HvError::Engine(format!("invalid MAC address '{mac}'")));
            }
        }
        Ok(())
    }
}

impl NetworkBackend for UserNetBackend {
    fn qemu_args(&self, nic: &NicConfig) -> Result<Vec<String>, HvError> {
        Self::validate(nic)?;
        let mut netdev = format!("user,id={}", nic.id);
        for fwd in &nic.port_forwards {
            netdev.push_str(&format!(",hostfwd={}", fwd.to_hostfwd()));
        }
        let mut device = format!("{},netdev={}", nic.model, nic.id);
        if let Some(mac) = &nic.mac {
            device.push_str(&format!(",mac={mac}"));
        }
        Ok(vec![
            "-netdev".to_string(),
            netdev,
            "-device".to_string(),
            device,
        ])
    }

    fn hostfwd_add(
        &self,
        qmp: &mut dyn QmpConn,
        nic_id: &str,
        fwd: &PortForward,
    ) -> Result<(), HvError> {
        let cmd = format!("hostfwd_add {} {}", nic_id, fwd.to_hostfwd());
        let out = qmp.human_monitor_command(&cmd)?;
        if out.trim().is_empty() {
            Ok(())
        } else {
            // HMP reports errors (e.g. host-port conflicts) as text output (N3).
            Err(HvError::Engine(format!(
                "hostfwd_add failed: {}",
                out.trim()
            )))
        }
    }

    fn hostfwd_remove(
        &self,
        qmp: &mut dyn QmpConn,
        nic_id: &str,
        fwd: &PortForward,
    ) -> Result<(), HvError> {
        let cmd = format!(
            "hostfwd_remove {} {}:{}:{}",
            nic_id,
            fwd.proto.as_str(),
            fwd.host_ip.as_deref().unwrap_or("127.0.0.1"),
            fwd.host_port
        );
        let out = qmp.human_monitor_command(&cmd)?;
        if out.trim().is_empty() {
            Ok(())
        } else {
            Err(HvError::Engine(format!(
                "hostfwd_remove failed: {}",
                out.trim()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockQmp {
        sent: Vec<String>,
        reply: String,
        fail: bool,
    }

    impl MockQmp {
        fn ok() -> Self {
            MockQmp {
                sent: Vec::new(),
                reply: String::new(),
                fail: false,
            }
        }
    }

    impl QmpConn for MockQmp {
        fn human_monitor_command(&mut self, command_line: &str) -> Result<String, HvError> {
            if self.fail {
                return Err(HvError::Engine("qmp connection lost".into()));
            }
            self.sent.push(command_line.to_string());
            Ok(self.reply.clone())
        }
    }

    fn fwd(host_port: u16, guest_port: u16) -> PortForward {
        PortForward {
            proto: Proto::Tcp,
            host_ip: None,
            host_port,
            guest_port,
        }
    }

    #[test]
    fn qemu_args_plain_nat() {
        let nic = NicConfig::nat("net0");
        let args = UserNetBackend::new().qemu_args(&nic).unwrap();
        assert_eq!(
            args,
            vec![
                "-netdev",
                "user,id=net0",
                "-device",
                "virtio-net-pci,netdev=net0"
            ]
        );
    }

    #[test]
    fn qemu_args_with_forwards_and_mac() {
        // Conformance vector matching the Python natgen reference output.
        let mut nic = NicConfig::nat("net0");
        nic.mac = Some("52:54:00:12:34:56".to_string());
        nic.port_forwards = vec![
            fwd(2222, 22),
            PortForward {
                proto: Proto::Udp,
                host_ip: Some("0.0.0.0".to_string()),
                host_port: 5353,
                guest_port: 53,
            },
        ];
        let args = UserNetBackend::new().qemu_args(&nic).unwrap();
        assert_eq!(
            args,
            vec![
                "-netdev",
                "user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=udp:0.0.0.0:5353-:53",
                "-device",
                "virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56",
            ]
        );
    }

    #[test]
    fn qemu_args_is_deterministic() {
        let mut nic = NicConfig::nat("net1");
        nic.port_forwards = vec![fwd(8080, 80), fwd(2222, 22)];
        let backend = UserNetBackend::new();
        assert_eq!(
            backend.qemu_args(&nic).unwrap(),
            backend.qemu_args(&nic).unwrap()
        );
    }

    #[test]
    fn qemu_args_rejects_bad_id_and_mac() {
        let mut nic = NicConfig::nat("0bad");
        assert!(UserNetBackend::new().qemu_args(&nic).is_err());
        nic.id = "net0".into();
        nic.mac = Some("not-a-mac".into());
        assert!(UserNetBackend::new().qemu_args(&nic).is_err());
    }

    #[test]
    fn parse_short_form() {
        let f = PortForward::parse("2222:22").unwrap();
        assert_eq!(f, fwd(2222, 22));
        assert_eq!(f.to_hostfwd(), "tcp:127.0.0.1:2222-:22");
    }

    #[test]
    fn parse_proto_form() {
        let f = PortForward::parse("udp:5353:53").unwrap();
        assert_eq!(f.proto, Proto::Udp);
        assert_eq!(f.to_hostfwd(), "udp:127.0.0.1:5353-:53");
    }

    #[test]
    fn parse_full_form_with_host_ip() {
        let f = PortForward::parse("tcp:0.0.0.0:2222:22").unwrap();
        assert_eq!(f.host_ip.as_deref(), Some("0.0.0.0"));
        assert_eq!(f.to_hostfwd(), "tcp:0.0.0.0:2222-:22");
    }

    #[test]
    fn parse_rejects_bad_specs() {
        for spec in [
            "",
            "22",
            "tcp",
            "abc:22",
            "2222:0",
            "0:22",
            "2222:99999",
            "icmp:1:2",
            "tcp:not.an.ip:2222:22",
            "a:b:c:d:e",
        ] {
            assert!(PortForward::parse(spec).is_err(), "should reject {spec:?}");
        }
    }

    #[test]
    fn hostfwd_add_sends_hmp_command() {
        let mut qmp = MockQmp::ok();
        UserNetBackend::new()
            .hostfwd_add(&mut qmp, "net0", &fwd(2222, 22))
            .unwrap();
        assert_eq!(qmp.sent, vec!["hostfwd_add net0 tcp:127.0.0.1:2222-:22"]);
    }

    #[test]
    fn hostfwd_add_port_conflict_is_error() {
        let mut qmp = MockQmp::ok();
        qmp.reply = "Could not set up host forwarding rule 'tcp:127.0.0.1:2222-:22'".into();
        let err = UserNetBackend::new()
            .hostfwd_add(&mut qmp, "net0", &fwd(2222, 22))
            .unwrap_err();
        assert!(err.to_string().contains("host forwarding rule"));
    }

    #[test]
    fn hostfwd_remove_sends_hmp_command() {
        let mut qmp = MockQmp::ok();
        UserNetBackend::new()
            .hostfwd_remove(&mut qmp, "net0", &fwd(2222, 22))
            .unwrap();
        assert_eq!(qmp.sent, vec!["hostfwd_remove net0 tcp:127.0.0.1:2222"]);
    }

    #[test]
    fn hostfwd_qmp_failure_propagates() {
        let mut qmp = MockQmp::ok();
        qmp.fail = true;
        assert!(UserNetBackend::new()
            .hostfwd_add(&mut qmp, "net0", &fwd(2222, 22))
            .is_err());
    }

    #[test]
    fn port_forward_json_roundtrip() {
        let f = PortForward {
            proto: Proto::Tcp,
            host_ip: None,
            host_port: 2222,
            guest_port: 22,
        };
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"proto":"tcp","host_port":2222,"guest_port":22}"#);
        let back: PortForward = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }
}
