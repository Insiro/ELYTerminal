//! Programs usage runs off the main thread: bounded by time, a secret only ever on stdin.

use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail};

/// A token, kept out of logs: its Debug says nothing of it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

/// What a program wrote and how it ended.
#[derive(Debug)]
pub struct Output {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Kills and reaps its child whatever way the caller leaves; on Unix its whole group, where it leads one.
pub struct Reaped {
    pub child: Child,
    /// Whether the child leads its own group, and the group goes with it.
    pub group: bool,
}

impl Drop for Reaped {
    fn drop(&mut self) {
        // Unreaped, its id still names its group.
        #[cfg(unix)]
        if self.group
            && let Ok(pid) = libc::pid_t::try_from(self.child.id())
        {
            // SAFETY: a plain signal to the group the unreaped child leads.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs `command`, writing `input` to its stdin, killed past `limit`.
pub fn run(mut command: Command, input: &[u8], limit: Duration) -> anyhow::Result<Output> {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        crate::git::CREATE_NO_WINDOW,
    );
    let name = command.get_program().to_string_lossy().into_owned();
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("usage: {name} did not start"))?;
    let mut child = Reaped {
        child,
        group: false,
    };
    let mut stdin = child.child.stdin.take().context("usage: no stdin")?;
    let stdout = drain(child.child.stdout.take());
    let stderr = drain(child.child.stderr.take());
    // A program that reads none of it still ends; its pipe error is no failure.
    let _ = stdin.write_all(input);
    drop(stdin);
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.child.try_wait().context("usage: wait")? {
            break status;
        }
        if Instant::now() >= deadline {
            bail!("usage: {name} took past {}s", limit.as_secs());
        }
        thread::sleep(Duration::from_millis(20));
    };
    Ok(Output {
        ok: status.success(),
        code: status.code(),
        stdout: stdout.join(),
        stderr: stderr.join(),
    })
}

/// Reads a pipe to its end on a thread of its own, keeping 4 MiB at most.
pub fn drain(pipe: Option<impl Read + Send + 'static>) -> Drained {
    let (sent, read) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(4 << 20).read_to_end(&mut kept);
        }
        let _ = sent.send(kept);
    });
    Drained(read)
}

/// A pipe being read.
pub struct Drained(mpsc::Receiver<Vec<u8>>);

impl Drained {
    /// What it held, waiting a second at most for a grandchild that holds it open.
    pub fn join(self) -> Vec<u8> {
        self.0
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_default()
    }
}

/// An HTTP answer: its status, its `Retry-After` in seconds and its body.
#[derive(Debug, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: Vec<u8>,
}

/// The mark curl's `--write-out` ends its output with, before the status.
const MARK: &str = "\n\u{1e}elyterminal-http:";

/// GETs `url` through curl, `headers` handed over on stdin as curl's config, within 10 s.
pub fn get(url: &str, headers: &[(&str, &str)]) -> anyhow::Result<Answer> {
    let mut config = String::new();
    for (name, value) in headers {
        anyhow::ensure!(
            !value.contains(['"', '\\', '\n', '\r']),
            "usage: the {name} header holds a character curl's config would read"
        );
        config.push_str(&format!("header = \"{name}: {value}\"\n"));
    }
    let mut command = Command::new("curl");
    command
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            "--config",
            "-",
        ])
        .args([
            "--write-out",
            &format!("{MARK}%{{http_code}} %header{{retry-after}}"),
        ])
        .arg(url);
    let output = run(command, config.as_bytes(), Duration::from_secs(15))?;
    anyhow::ensure!(
        output.ok,
        "usage: curl failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    answer(output.stdout)
}

/// Splits curl's output into the body and what `--write-out` added.
pub fn answer(mut out: Vec<u8>) -> anyhow::Result<Answer> {
    let mark = MARK.as_bytes();
    let at = out
        .windows(mark.len())
        .rposition(|window| window == mark)
        .context("usage: curl's output has no status")?;
    let tail = String::from_utf8_lossy(&out[at + mark.len()..]).into_owned();
    out.truncate(at);
    let mut parts = tail.split_whitespace();
    let status = parts
        .next()
        .and_then(|code| code.parse().ok())
        .context("usage: curl named no status")?;
    let retry_after = parts.next().and_then(|secs| secs.parse().ok());
    Ok(Answer {
        status,
        retry_after,
        body: out,
    })
}
