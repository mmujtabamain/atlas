//! Starting and stopping a private `Xvfb` server.
//!
//! The display number is chosen by Xvfb itself (`-displayfd`): it takes the
//! lock atomically and reports the number on the given descriptor once the
//! server accepts connections. Picking a number ourselves from the lock files
//! races when several captures start at the same time (parallel tests did).

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use log::{debug, info, warn};

pub struct Xvfb {
    child: Child,
    display: String,
    number: u32,
}

/// Parses what Xvfb writes to `-displayfd`: the bare display number and a newline.
pub fn parse_display_number(line: &str) -> Result<u32> {
    line.trim()
        .parse()
        .map_err(|_| anyhow!("Xvfb reported an unexpected display number: {line:?}"))
}

impl Xvfb {
    /// Spawns `Xvfb -displayfd 1 -screen 0 WxHx24` and waits until it reports
    /// its display number (which it only does once it accepts connections).
    pub fn start(width: u16, height: u16) -> Result<Xvfb> {
        info!("starting Xvfb ({width}x{height}x24)");
        let mut child = Command::new("Xvfb")
            // Let Xvfb pick a free display and print it on stdout (fd 1).
            .args(["-displayfd", "1"])
            .args(["-screen", "0", &format!("{width}x{height}x24")])
            // No TCP, no reset between clients (keeps atoms/cursors stable), 96 dpi
            // so gpui's scale factor is 1.0 and the PNG matches logical pixels.
            .args(["-nolisten", "tcp", "-noreset", "-dpi", "96", "-ac"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning Xvfb (is the `xvfb` package installed?)")?;

        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let read = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(read.map(|_| line));
        });

        let started = Instant::now();
        let number = match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(line)) if !line.trim().is_empty() => parse_display_number(&line)?,
            Ok(Ok(_)) | Ok(Err(_)) => {
                let stderr = drain_stderr(&mut child);
                let _ = child.kill();
                bail!("Xvfb exited before reporting a display: {}", stderr.trim());
            }
            Err(_) => {
                let _ = child.kill();
                bail!("Xvfb did not report a display within 15s");
            }
        };
        let display = format!(":{number}");
        debug!("Xvfb is up on {display} (pid {}) after {:?}", child.id(), started.elapsed());
        Ok(Xvfb { child, display, number })
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    /// Detaches the server so it outlives this process (`--keep-running`).
    pub fn leak(mut self) {
        info!("leaving Xvfb running on {} (pid {})", self.display, self.child.id());
        // Replace the child with a dummy so Drop does not kill it.
        let dummy = Command::new("true").spawn().expect("spawning `true`");
        let mut real = std::mem::replace(&mut self.child, dummy);
        std::mem::forget(real.stderr.take());
        std::mem::forget(real);
    }
}

fn drain_stderr(child: &mut Child) -> String {
    let mut text = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut text);
    }
    text
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        // SIGTERM lets Xvfb remove its lock file and socket; SIGKILL would not.
        let pid = self.child.id().to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            warn!("Xvfb {} ignored SIGTERM, killing it", self.display);
            let _ = self.child.kill();
            let _ = self.child.wait();
            for stale in [
                format!("/tmp/.X{}-lock", self.number),
                format!("/tmp/.X11-unix/X{}", self.number),
            ] {
                let _ = std::fs::remove_file(stale);
            }
        }
        debug!("Xvfb {} stopped", self.display);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_displayfd_line() {
        assert_eq!(parse_display_number("90\n").unwrap(), 90);
        assert_eq!(parse_display_number("  7 ").unwrap(), 7);
        assert!(parse_display_number("").is_err());
        assert!(parse_display_number("nope").is_err());
    }
}
