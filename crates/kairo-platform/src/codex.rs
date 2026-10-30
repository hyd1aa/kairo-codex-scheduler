//! Sole real-model execution site, guarded by local and account reservations.
use crate::Store;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountJournal {
    pub schema: u32,
    pub account: String,
    pub last_attempt: i64,
    pub anchors: std::collections::BTreeSet<String>,
}
pub struct AccountGate {
    store: Store,
    journal: AccountJournal,
    reserved_at: Option<i64>,
    recovery: Option<crate::recovery::Recovery>,
}
impl AccountGate {
    /// Gate must have been explicitly provisioned during an approved migration.
    /// Missing, corrupt, or mismatched journals are never recreated automatically.
    pub fn open_existing(root: &Path, account: &str) -> Result<Self> {
        if account.len() != 64 || !account.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("Invalid account fingerprint / 账号指纹无效")
        }
        let store = Store::open(&root.join(account))?;
        let journal: AccountJournal = store
            .read("account.json")
            .context("Account gate missing; migration required / 账号门禁缺失，需要迁移")?;
        if journal.schema != 1 || journal.account != account {
            bail!("Account gate mismatch / 账号门禁不匹配")
        }
        let recovery = if crate::recovery::present(&store.root.join("recovery.json"))? {
            let r: crate::recovery::Recovery = store.read("recovery.json")?;
            r.validate(account)?;
            if !r.approved {
                bail!("recovery_approval_required / 历史未知恢复尚未获准；请完成等待和恢复向导");
            }
            Some(r)
        } else {
            None
        };
        Ok(Self {
            store,
            journal,
            reserved_at: None,
            recovery,
        })
    }
    pub fn not_before(&self, buffer: i64) -> i64 {
        self.journal
            .last_attempt
            .saturating_add(18000 + buffer)
            .max(self.recovery.as_ref().map_or(0, |r| r.not_before))
    }
    pub fn check_local(&self, root: &Path) -> Result<()> {
        let owner = std::fs::canonicalize(root)?;
        let local_path = root.join("recovery.json");
        let local = if crate::recovery::present(&local_path)? {
            let bytes = crate::private_bytes(&local_path, 1024 * 1024)?;
            let r: crate::recovery::Recovery = serde_json::from_slice(&bytes)?;
            r.validate(&self.journal.account)?;
            Some(r)
        } else {
            None
        };
        match (self.recovery.as_ref(),local) {
            (None,None)=>Ok(()),
            (Some(r),Some(local)) if r.owner==owner && local.owner==owner && local.started_at==r.started_at && local.approved=>Ok(()),
            _=>bail!("recovery_owner_mismatch / 全局恢复授权与本地目录或记录不匹配；请运行 recover check，禁止执行"),
        }
    }
    pub fn reserve_budget(
        &mut self,
        anchor: &str,
        now: i64,
        buffer: i64,
        budget_until: i64,
    ) -> Result<()> {
        self.reserve(anchor, now, buffer)?;
        self.reserved_at = Some(now);
        self.journal.last_attempt = self.journal.last_attempt.max(budget_until);
        self.store.write("account.json", &self.journal)
    }
    pub fn finish(&mut self, now: i64, ok: bool) -> Result<()> {
        if self.reserved_at.is_some_and(|start| now >= start) {
            self.journal.last_attempt = if ok {
                now
            } else {
                self.journal.last_attempt.max(now)
            };
            self.store.write("account.json", &self.journal)?;
        }
        Ok(())
    }
    pub fn reserve(&mut self, anchor: &str, now: i64, buffer: i64) -> Result<()> {
        if !(0..=300).contains(&buffer) || self.journal.last_attempt < 0 {
            bail!("Invalid gate parameters / 门禁参数无效")
        }
        if now < self.not_before(buffer) || self.journal.anchors.contains(anchor) {
            bail!("Account duplicate guard / 账号级重复请求保护")
        }
        self.journal.last_attempt = now;
        self.journal.anchors.insert(anchor.into());
        self.store.write("account.json", &self.journal)
    }
}
pub struct CodexExecutor {
    binary: PathBuf,
    home: PathBuf,
    workdir: PathBuf,
    timeout: Duration,
    codex_home: Option<PathBuf>,
    cutoff: Option<i64>,
}
impl CodexExecutor {
    pub fn new(binary: PathBuf, home: PathBuf, workdir: PathBuf) -> Result<Self> {
        if !binary.is_absolute() || !home.is_absolute() || !workdir.is_absolute() {
            bail!("Absolute paths required / 需要绝对路径")
        }
        if std::fs::read_dir(&workdir)?.next().is_some() {
            bail!("Workdir must be empty / 工作目录必须为空")
        }
        Ok(Self {
            binary,
            home,
            workdir,
            timeout: Duration::from_secs(90),
            codex_home: None,
            cutoff: None,
        })
    }
    pub fn with_cutoff(mut self, at: i64) -> Self {
        self.cutoff = Some(at);
        self
    }
    pub fn with_codex_home(mut self, path: PathBuf) -> Self {
        self.codex_home = Some(path);
        self
    }
    fn command(&self) -> Command {
        let mut c = Command::new(&self.binary);
        c.process_group(0)
            .env_clear()
            .env("HOME", &self.home)
            .env(
                "PATH",
                format!(
                    "{}:/usr/local/bin:/usr/bin:/bin",
                    self.binary
                        .parent()
                        .unwrap_or(Path::new("/usr/bin"))
                        .display()
                ),
            )
            .env("LANG", "C.UTF-8")
            .current_dir(&self.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = &self.codex_home {
            c.env("CODEX_HOME", path);
        }
        c
    }
    /// This is the sole real-model invocation site. Account reservation is durable first.
    /// Local state reservation must precede this call; failure remains an attempted anchor.
    pub fn execute(
        &self,
        gate: &mut AccountGate,
        anchor: &str,
        now: i64,
        buffer: i64,
        local: &Store,
    ) -> Result<bool> {
        gate.check_local(&local.root)?;
        let s = local.state()?;
        let a = s
            .attempts
            .last()
            .context("Local reservation missing / 缺少本地调用前记录")?;
        if s.account != gate.journal.account
            || a.at != now
            || a.anchor != anchor
            || a.outcome != "attempt_persisted_unknown"
        {
            bail!("Local gate mismatch / 本地门禁不匹配")
        }
        let help =
            crate::process::output(self.command().args(["exec", "--help"]), 10, 1024 * 1024)?;
        if !help.status.success() {
            bail!("cli_help_failed / CLI 安全参数检查失败")
        }
        let help = String::from_utf8_lossy(&help.stdout);
        for flag in [
            "--ignore-user-config",
            "--sandbox",
            "--skip-git-repo-check",
            "--ephemeral",
        ] {
            if !help.contains(flag) {
                bail!("cli_safety_flag_missing / CLI 缺少必要安全参数")
            }
        }
        if let Some(home) = &self.codex_home {
            let current = crate::identity(&home.join("auth.json"))?;
            if current.fingerprint != gate.journal.account {
                bail!("account_changed_before_exec / 执行前账号发生变化，禁止请求")
            }
        }
        let spawn_at = if self.cutoff.is_some() {
            chrono::Utc::now().timestamp()
        } else {
            now
        };
        if spawn_at < now || self.cutoff.is_some_and(|cutoff| spawn_at >= cutoff) {
            bail!("daily_cutoff / 已到当日截止时间或时钟回拨，禁止请求")
        }
        let budget = self.timeout.as_secs() as i64;
        gate.reserve_budget(anchor, now, buffer, spawn_at.saturating_add(budget))?;
        let mut child = self
            .command()
            .args([
                "exec",
                "--ignore-user-config",
                "--sandbox",
                "read-only",
                "--skip-git-repo-check",
                "--ephemeral",
                "Reply OK.",
            ])
            .spawn()?;
        let _watch = match crate::guardian::ParentWatch::new(child.id()) {
            Ok(w) => w,
            Err(e) => {
                terminate_group(&mut child)?;
                return Err(e);
            }
        };
        let timeout = if let Some(cutoff) = self.cutoff {
            self.timeout.min(Duration::from_secs(
                cutoff.saturating_sub(spawn_at).max(0) as u64
            ))
        } else {
            self.timeout
        };
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                gate.finish(
                    if self.cutoff.is_some() {
                        chrono::Utc::now().timestamp()
                    } else {
                        now
                    },
                    status.success(),
                )?;
                return Ok(status.success());
            }
            if Instant::now() > deadline || crate::runtime::stopping() {
                terminate_group(&mut child)?;
                gate.finish(
                    if self.cutoff.is_some() {
                        chrono::Utc::now().timestamp()
                    } else {
                        now
                    },
                    false,
                )?;
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}
fn terminate_group(child: &mut Child) -> Result<()> {
    // Command::process_group(0) creates a dedicated group whose id is the child pid.
    // Killing the group covers normal descendant processes before reaping the leader.
    let result = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
        return Err(std::io::Error::last_os_error().into());
    }
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairo_core::{reserve, Quota, State};
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn mock_cli_obeys_both_durable_gates() {
        let root = std::env::temp_dir().join(format!("kairo-codex-{}", std::process::id()));
        fs::create_dir_all(root.join("work")).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        let binary = root.join("mock-codex");
        fs::write(&binary,"#!/bin/sh\nif [ \"$2\" = --help ]; then echo '--ignore-user-config --sandbox --skip-git-repo-check --ephemeral'; exit 0; fi\n[ -z \"$OPENAI_API_KEY\" ] || exit 12\n[ -z \"$OPENAI_BASE_URL\" ] || exit 13\nprintf invoked >> \"$HOME/calls\"\nexit 0\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let account = "a".repeat(64);
        let guardroot = root.join("guards");
        {
            let store = Store::open(&guardroot.join(&account)).unwrap();
            store
                .write(
                    "account.json",
                    &AccountJournal {
                        schema: 1,
                        account: account.clone(),
                        last_attempt: 0,
                        anchors: Default::default(),
                    },
                )
                .unwrap();
        }
        let local = Store::open(&root.join("local")).unwrap();
        let n = 100000;
        let mut s = State {
            schema: 1,
            account: account.clone(),
            ..Default::default()
        };
        reserve(
            &mut s,
            "2026-10-19/11:00",
            n,
            Quota {
                duration: 18000,
                used: 0.,
                reset_at: n + 18000,
                remaining: 18000,
            },
        );
        local.write("state.json", &s).unwrap();
        let exec = CodexExecutor::new(binary, root.join("home"), root.join("work")).unwrap();
        let mut gate = AccountGate::open_existing(&guardroot, &account).unwrap();
        assert!(exec
            .execute(&mut gate, "2026-10-19/11:00", n, 30, &local)
            .unwrap());
        assert!(exec
            .execute(&mut gate, "2026-10-19/11:00", n, 30, &local)
            .is_err());
        assert_eq!(
            fs::read_to_string(root.join("home/calls")).unwrap(),
            "invoked"
        );
        let expired = CodexExecutor::new(
            root.join("mock-codex"),
            root.join("home"),
            root.join("work"),
        )
        .unwrap()
        .with_cutoff(chrono::Utc::now().timestamp() - 1);
        assert!(expired
            .execute(&mut gate, "2026-10-19/11:00", n, 30, &local)
            .unwrap_err()
            .to_string()
            .contains("daily_cutoff"));
        let auth = root.join("auth");
        fs::create_dir(&auth).unwrap();
        let path = auth.join("auth.json");
        fs::write(
            &path,
            r#"{"tokens":{"account_id":"changed-account","access_token":"synthetic"}}"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let changed = CodexExecutor::new(
            root.join("mock-codex"),
            root.join("home"),
            root.join("work"),
        )
        .unwrap()
        .with_codex_home(auth);
        assert!(changed
            .execute(&mut gate, "2026-10-19/11:00", n, 30, &local)
            .unwrap_err()
            .to_string()
            .contains("account_changed_before_exec"));
        assert_eq!(
            fs::read_to_string(root.join("home/calls")).unwrap(),
            "invoked"
        );
        drop(gate);
        drop(local);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_account_state_blocks() {
        let root = std::env::temp_dir().join(format!("kairo-gate-missing-{}", std::process::id()));
        assert!(AccountGate::open_existing(&root, &"b".repeat(64)).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
