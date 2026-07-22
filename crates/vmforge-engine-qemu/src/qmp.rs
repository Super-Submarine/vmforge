//! Minimal QMP client over a unix socket.
//!
//! Implements the QMP handshake (greeting -> `qmp_capabilities`) and
//! synchronous command execution, skipping asynchronous events
//! (https://www.qemu.org/docs/master/interop/qmp-spec.html).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use vmforge_core::HvError;

pub struct QmpClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    version: (u64, u64),
}

impl QmpClient {
    /// Connect to a QMP unix socket, retrying until `timeout` while QEMU
    /// starts up, then perform the capabilities handshake.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Self, HvError> {
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect(socket) {
                Ok(s) => break s,
                Err(e) if Instant::now() < deadline => {
                    let _ = e;
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    return Err(HvError::Engine(format!(
                        "QMP socket {} not available: {e}",
                        socket.display()
                    )))
                }
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(HvError::Io)?;
        let writer = stream.try_clone().map_err(HvError::Io)?;
        let mut client = Self {
            reader: BufReader::new(stream),
            writer,
            version: (0, 0),
        };
        // Greeting, then capabilities negotiation.
        let greeting = client.read_message()?;
        if greeting.get("QMP").is_none() {
            return Err(HvError::Engine(format!(
                "unexpected QMP greeting: {greeting}"
            )));
        }
        let qemu = &greeting["QMP"]["version"]["qemu"];
        client.version = (
            qemu["major"].as_u64().unwrap_or(0),
            qemu["minor"].as_u64().unwrap_or(0),
        );
        client.execute("qmp_capabilities", None)?;
        Ok(client)
    }

    /// QEMU `(major, minor)` version reported in the QMP greeting.
    pub fn qemu_version(&self) -> (u64, u64) {
        self.version
    }

    /// Execute a QMP command and return its `return` payload.
    pub fn execute(&mut self, command: &str, arguments: Option<Value>) -> Result<Value, HvError> {
        let mut msg = json!({ "execute": command });
        if let Some(args) = arguments {
            msg["arguments"] = args;
        }
        let line = format!("{msg}\n");
        self.writer
            .write_all(line.as_bytes())
            .map_err(HvError::Io)?;
        loop {
            let reply = self.read_message()?;
            if let Some(ret) = reply.get("return") {
                return Ok(ret.clone());
            }
            if let Some(err) = reply.get("error") {
                return Err(HvError::Engine(format!(
                    "QMP command '{command}' failed: {err}"
                )));
            }
            // Skip asynchronous events.
        }
    }

    fn read_message(&mut self) -> Result<Value, HvError> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).map_err(HvError::Io)?;
        if n == 0 {
            return Err(HvError::Engine("QMP connection closed by QEMU".into()));
        }
        serde_json::from_str(&line)
            .map_err(|e| HvError::Engine(format!("invalid QMP message '{line}': {e}")))
    }
}
