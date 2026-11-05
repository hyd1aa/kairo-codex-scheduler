//! The only model-launch site: one process, no retries, bounded evidence and lifetime.
use crate::{management::account_command, runtime::Runtime, Store};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde_json::Value;
use std::os::fd::AsRawFd;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn command(store: &Store, r: &Runtime) -> Result<Command> {
    r.validate()?;
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let code = fs::read(&r.codex).context("offline_mock_required")?;
        if !code.starts_with(b"#!")
            || !code
                .windows(b"KAIRO_SYNTHETIC_CODEX".len())
                .any(|w| w == b"KAIRO_SYNTHETIC_CODEX")
        {
            bail!("offline_mock_required / 离线模式只允许模拟 CLI");
        }
    }
    let mut c = account_command(store, &r.codex)?;
    c.env_clear()
        .env("HOME", &r.home)
        .env("CODEX_HOME", &r.codex_home)
        .env(
            "PATH",
            format!(
                "{}:/usr/local/bin:/usr/bin:/bin",
                r.codex
                    .parent()
                    .unwrap_or(std::path::Path::new("/usr/bin"))
                    .display()
            ),
        )
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::null());
    Ok(c)
}
pub fn logged_in(store: &Store, r: &Runtime) -> Result<bool> {
    Ok(
        crate::process::output(command(store, r)?.args(["login", "status"]), 10, 65536)?
            .status
            .success(),
    )
}
pub fn safe_reason(text: &str) -> String {
    let s = text.to_lowercase();
    for (words, label) in [
        (&["call_in_progress"][..], "call_in_progress"),
        (
            &[
                "not_logged_in",
                "401",
                "unauthorized",
                "authentication",
                "oauth",
            ][..],
            "login_required",
        ),
        (&["weekly"][..], "weekly_limit"),
        (
            &["429", "rate limit", "usage limit", "quota"][..],
            "server_limit",
        ),
        (&["timeout", "timed out"][..], "timeout"),
        (&["network", "connect", "transport"][..], "network_error"),
        (
            &["unsupported", "cli_safety", "help_failed"][..],
            "cli_unsupported",
        ),
        (&["offline_mock"][..], "offline_mock_required"),
        (&["permission denied"][..], "permission_denied"),
        (&["no such file"][..], "codex_missing"),
    ] {
        if words.iter().any(|w| s.contains(w)) {
            return label.into();
        }
    }
    "cli_failed".into()
}
pub struct Outcome {
    pub success: bool,
    pub reason: String,
    pub exit_code: Option<i32>,
}
pub struct Running {
    child: Child,
    _watch: crate::guardian::ParentWatch,
    _lease: File,
    work: PathBuf,
    stdout: Option<thread::JoinHandle<Vec<u8>>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
}
fn reader(input: impl Read + Send + 'static, limit: u64) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = input.take(limit + 1).read_to_end(&mut bytes);
        bytes
    })
}
pub fn start(store: &Store, r: &Runtime) -> Result<Running> {
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(store.root.join("call.lock"))?;
    crate::preserve_owner(&lease, &store.root)?;
    crate::single_owner(&lease)?;
    lease.try_lock_exclusive().context("call_in_progress")?;
    // Keep the invocation lock in the child/guardian until every managed process exits.
    if unsafe { libc::fcntl(lease.as_raw_fd(), libc::F_SETFD, 0) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // No credential parsing, copying or server quota query in this path.
    if !logged_in(store, r)? {
        bail!("not_logged_in");
    }
    let help =
        crate::process::output(command(store, r)?.args(["exec", "--help"]), 10, 1024 * 1024)?;
    let text = String::from_utf8_lossy(&help.stdout);
    if !help.status.success()
        || [
            "--ignore-user-config",
            "--ignore-rules",
            "--sandbox",
            "--ephemeral",
            "--json",
            "--skip-git-repo-check",
        ]
        .iter()
        .any(|f| !text.contains(f))
    {
        bail!("cli_safety_flags_unsupported");
    }
    let work = store.root.join(format!("call-work-{}", std::process::id()));
    if crate::present(&work)? {
        // Crash leftovers are never reused as instructions or project configuration.
        if fs::symlink_metadata(&work)?.file_type().is_symlink() {
            bail!("workdir_unsafe");
        }
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir(&work)?;
    fs::set_permissions(&work, fs::Permissions::from_mode(0o700))?;
    if unsafe { libc::geteuid() } == 0 {
        let owner = fs::metadata(&store.root)?;
        let name = std::ffi::CString::new(work.as_os_str().as_encoded_bytes())?;
        if unsafe { libc::chown(name.as_ptr(), owner.uid(), owner.gid()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let mut cmd = command(store, r)?;
    cmd.current_dir(&work).process_group(0).args([
        "exec",
        "--ignore-user-config",
        "--ignore-rules",
        "--sandbox",
        "read-only",
        "--skip-git-repo-check",
        "--ephemeral",
        "--json",
        "--color",
        "never",
        "-c",
        "approval_policy=\"never\"",
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "features.shell_tool=false",
        "-c",
        "features.view_image=false",
        "-c",
        "model_providers.openai.request_max_retries=0",
        "-c",
        "model_providers.openai.stream_max_retries=0",
        "请仅回复：Kairo ping。不要调用工具，不要修改文件。",
    ]);
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_dir_all(&work);
            return Err(e.into());
        }
    };
    let watch = match crate::guardian::ParentWatch::with_lock(child.id(), lease.as_raw_fd()) {
        Ok(w) => w,
        Err(e) => {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(e);
        }
    };
    let stdout = reader(child.stdout.take().unwrap(), 1024 * 1024);
    let stderr = reader(child.stderr.take().unwrap(), 65536);
    Ok(Running {
        child,
        _watch: watch,
        _lease: lease,
        work,
        stdout: Some(stdout),
        stderr: Some(stderr),
    })
}
impl Running {
    pub fn finish(mut self) -> Outcome {
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut timed_out = false;
        let code = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) if Instant::now() < deadline && !crate::runtime::stopping() => {
                    thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    timed_out = true;
                    break None;
                }
            }
        };
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
        let stdout = self.stdout.take().unwrap().join().unwrap_or_default();
        let stderr = self.stderr.take().unwrap().join().unwrap_or_default();
        let mut message = false;
        let mut complete = false;
        let mut invalid = stdout.len() > 1024 * 1024 || stderr.len() > 65536;
        let mut error = String::from_utf8_lossy(&stderr).to_string();
        for line in stdout.split(|b| *b == b'\n').filter(|s| !s.is_empty()) {
            match serde_json::from_slice::<Value>(line) {
                Ok(v) => match v["type"].as_str() {
                    Some("item.completed") if v["item"]["type"] == "agent_message" => {
                        message = v["item"]["text"]
                            .as_str()
                            .is_some_and(|s| !s.trim().is_empty());
                    }
                    Some("turn.completed") => {
                        complete = v["usage"]["output_tokens"].as_u64().is_some_and(|n| n > 0)
                    }
                    Some("turn.failed" | "error") => {
                        invalid = true;
                        error.push_str(&v.to_string());
                    }
                    Some("item.started" | "item.completed")
                        if !matches!(
                            v["item"]["type"].as_str(),
                            Some("agent_message" | "reasoning")
                        ) =>
                    {
                        invalid = true
                    }
                    _ => {}
                },
                Err(_) => invalid = true,
            }
        }
        let success = code == Some(0) && message && complete && !invalid && !timed_out;
        Outcome {
            success,
            reason: if success {
                "completed".into()
            } else if timed_out {
                "timeout_or_stopped".into()
            } else if code == Some(0) && !invalid {
                "completion_evidence_missing".into()
            } else {
                safe_reason(&error)
            },
            exit_code: code,
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
        if let Some(t) = self.stdout.take() {
            let _ = t.join();
        }
        if let Some(t) = self.stderr.take() {
            let _ = t.join();
        }
        let _ = fs::remove_dir_all(&self.work);
    }
}
