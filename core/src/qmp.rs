//! Minimal QMP (QEMU Machine Protocol) client over a unix socket.
//!
//! Protocol: on connect QEMU sends a greeting; the client must negotiate
//! capabilities with `qmp_capabilities` before issuing commands. Commands
//! are single-line JSON objects; responses carry either `return` or `error`.
//! Asynchronous events (lines with an `event` key) are skipped while waiting
//! for a command response.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{Error, Result};

pub struct QmpClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl QmpClient {
    /// Connect to the QMP socket and negotiate capabilities.
    pub fn connect(socket: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        let writer = stream.try_clone()?;
        let mut client = QmpClient {
            reader: BufReader::new(stream),
            writer,
        };
        let greeting = client.read_line()?;
        if greeting.get("QMP").is_none() {
            return Err(Error::Qmp(format!("unexpected greeting: {greeting}")));
        }
        client.execute("qmp_capabilities", None)?;
        Ok(client)
    }

    /// Execute a QMP command and return its `return` payload.
    pub fn execute(&mut self, command: &str, arguments: Option<Value>) -> Result<Value> {
        let mut msg = json!({ "execute": command });
        if let Some(args) = arguments {
            msg["arguments"] = args;
        }
        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;

        loop {
            let resp = self.read_line()?;
            if resp.get("event").is_some() {
                continue; // async event, not our response
            }
            if let Some(err) = resp.get("error") {
                return Err(Error::QmpCommand {
                    command: command.to_string(),
                    class: err["class"].as_str().unwrap_or("Unknown").to_string(),
                    desc: err["desc"].as_str().unwrap_or_default().to_string(),
                });
            }
            if let Some(ret) = resp.get("return") {
                return Ok(ret.clone());
            }
        }
    }

    /// Run a legacy HMP (human monitor) command via QMP. Used for
    /// savevm/loadvm/delvm which have no stable QMP equivalent usable
    /// synchronously across QEMU versions.
    pub fn hmp(&mut self, command_line: &str) -> Result<String> {
        let ret = self.execute(
            "human-monitor-command",
            Some(json!({ "command-line": command_line })),
        )?;
        let out = ret.as_str().unwrap_or_default().to_string();
        // HMP reports errors as plain text on stdout; surface them.
        let lowered = out.to_lowercase();
        if lowered.contains("error") || lowered.contains("failed") {
            return Err(Error::Qmp(format!("HMP '{command_line}': {}", out.trim())));
        }
        Ok(out)
    }

    /// `query-status` — returns the run state string (e.g. "running", "paused").
    pub fn query_status(&mut self) -> Result<String> {
        let ret = self.execute("query-status", None)?;
        Ok(ret["status"].as_str().unwrap_or("unknown").to_string())
    }

    /// Graceful ACPI powerdown request.
    pub fn system_powerdown(&mut self) -> Result<()> {
        self.execute("system_powerdown", None)?;
        Ok(())
    }

    /// Immediately terminate QEMU.
    pub fn quit(&mut self) -> Result<()> {
        // QEMU may close the socket before replying; ignore io errors.
        match self.execute("quit", None) {
            Ok(_) => Ok(()),
            Err(Error::Io(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn read_line(&mut self) -> Result<Value> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            return Err(Error::Qmp("connection closed by QEMU".into()));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    /// Fake QMP server covering greeting, capabilities, a command, an
    /// interleaved event, and an error response.
    #[test]
    fn qmp_handshake_command_event_and_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("qmp.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut w = stream;
            w.write_all(b"{\"QMP\": {\"version\": {}, \"capabilities\": []}}\n")
                .unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap(); // qmp_capabilities
            w.write_all(b"{\"return\": {}}\n").unwrap();

            line.clear();
            r.read_line(&mut line).unwrap(); // query-status
            assert!(line.contains("query-status"));
            w.write_all(b"{\"event\": \"POWERDOWN\", \"timestamp\": {}}\n")
                .unwrap();
            w.write_all(b"{\"return\": {\"status\": \"running\", \"running\": true}}\n")
                .unwrap();

            line.clear();
            r.read_line(&mut line).unwrap(); // bogus command
            w.write_all(b"{\"error\": {\"class\": \"CommandNotFound\", \"desc\": \"nope\"}}\n")
                .unwrap();
        });

        let mut client = QmpClient::connect(&sock).unwrap();
        assert_eq!(client.query_status().unwrap(), "running");
        let err = client.execute("bogus-command", None).unwrap_err();
        match err {
            Error::QmpCommand { class, .. } => assert_eq!(class, "CommandNotFound"),
            other => panic!("unexpected error: {other}"),
        }
        server.join().unwrap();
    }
}
