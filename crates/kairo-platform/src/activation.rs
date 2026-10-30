//! Explicit administrator cutover for installer-managed systemd installations.
//! Filesystem/service effects are behind a testable adapter; no model request here.
use crate::{runtime as rt, Store};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const OBSERVER:&str="[Service]\nExecStart=\nExecStart=/opt/kairo/current/bin/kairo --data-dir /var/lib/kairo daemon --observe\n";
const BASE_COMMAND: &str = "/opt/kairo/current/bin/kairo --data-dir /var/lib/kairo daemon";
trait System {
    fn show(&mut self, key: &str) -> Result<String>;
    fn command(&mut self, action: &str) -> Result<()>;
    fn wait_live(&mut self, root: &Path, since: i64) -> Result<()>;
}
struct Real;
impl System for Real {
    fn show(&mut self, key: &str) -> Result<String> {
        let o = crate::process::output(
            std::process::Command::new("/usr/bin/systemctl").args([
                "show",
                "kairo.service",
                "--value",
                "--property",
                key,
            ]),
            10,
            65536,
        )?;
        if !o.status.success() {
            bail!("systemd_query_failed / 无法查询 Kairo systemd 状态");
        }
        Ok(String::from_utf8(o.stdout)?.trim().into())
    }
    fn command(&mut self, action: &str) -> Result<()> {
        let mut c = std::process::Command::new("/usr/bin/systemctl");
        c.arg(action);
        if action != "daemon-reload" {
            c.arg("kairo.service");
        }
        let o = crate::process::output(&mut c, 45, 65536)?;
        if !o.status.success() {
            bail!("systemd_cutover_failed / systemd 切换失败；自动请求授权已回退，请检查服务日志");
        }
        Ok(())
    }
    fn wait_live(&mut self, root: &Path, since: i64) -> Result<()> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(45);
        loop {
            let pid = self.show("MainPID")?.parse::<u32>().unwrap_or(0);
            let live = crate::private_bytes(&root.join("status.json"), 1024 * 1024)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .is_some_and(|v| live_record(&v, pid, since, chrono::Utc::now().timestamp()));
            if live
                && self.show("ActiveState")? == "active"
                && self.show("MainPID")?.parse::<u32>().unwrap_or(0) > 0
                && !self.show("ExecStart")?.contains("--observe")
            {
                return Ok(());
            }
            if std::time::Instant::now() >= until {
                bail!("live_status_unconfirmed / 服务尚未报告新的正式运行状态；授权已回退，查看日志并重试 enable");
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}
struct Plan {
    mode: PathBuf,
    had_mode: bool,
    was_active: bool,
    owner: u32,
}
fn safe_root_file(path: &Path, owner: u32) -> Result<String> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != owner
        || meta.nlink() != 1
        || meta.mode() & 0o022 != 0
    {
        bail!("managed_service_unsafe / 服务文件类型、权限或所有权不安全，拒绝自动修改");
    }
    let f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    f.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("unit_too_large / 服务文件过大");
    }
    Ok(String::from_utf8(bytes)?)
}
fn plan<S: System>(base: &Path, sys: &mut S, owner: u32) -> Result<Plan> {
    let unit = base.join("etc/systemd/system/kairo.service");
    let expected = include_str!("../../../packaging/kairo.service")
        .replace("@USER@", "kairo")
        .replace("@GROUP@", "kairo")
        .replace("@HOME@", "/home/kairo")
        .replace("@PREFIX@", "/opt/kairo");
    if safe_root_file(&unit, owner)? != expected {
        bail!("managed_service_changed / 服务不是标准安装器模板；保留自定义 unit，请人工核对切换");
    }
    if sys.show("FragmentPath")? != "/etc/systemd/system/kairo.service" {
        bail!("managed_unit_mismatch / systemd 使用了其他服务文件，拒绝接管");
    }
    let mode = base.join("etc/systemd/system/kairo.service.d/20-mode.conf");
    let had_mode = crate::recovery::present(&mode)?;
    if had_mode && safe_root_file(&mode, owner)? != OBSERVER {
        bail!("observation_override_changed / 观察模式覆盖文件已被修改；保留原件，禁止自动删除");
    }
    let directory = mode.parent().unwrap();
    if let Ok(m) = fs::symlink_metadata(directory) {
        if !m.is_dir() || m.file_type().is_symlink() || m.uid() != owner || m.mode() & 0o022 != 0 {
            bail!("managed_service_unsafe / 服务覆盖目录不安全");
        }
        for entry in fs::read_dir(directory)? {
            if entry?.path() != mode {
                bail!("custom_service_override / 存在自定义服务覆盖；请人工核对，向导不会删除它们");
            }
        }
    }
    for path in sys.show("DropInPaths")?.split_whitespace() {
        if path != "/etc/systemd/system/kairo.service.d/20-mode.conf" {
            bail!("custom_service_override / 存在自定义或运行时覆盖，禁止自动接管");
        }
    }
    let exec = sys.show("ExecStart")?;
    if !exec.contains(BASE_COMMAND) {
        bail!("managed_exec_mismatch / 服务启动命令与 Kairo 数据目录不匹配");
    }
    Ok(Plan {
        mode,
        had_mode,
        was_active: sys.show("ActiveState")? == "active",
        owner,
    })
}
fn restore_mode(plan: &Plan) -> Result<()> {
    if plan.had_mode && !crate::recovery::present(&plan.mode)? {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&plan.mode)?;
        f.write_all(OBSERVER.as_bytes())?;
        f.sync_all()?;
        fs::File::open(plan.mode.parent().unwrap())?.sync_all()?;
    }
    Ok(())
}
fn cutover<S: System, F: FnOnce() -> Result<()>, P: FnOnce() -> Result<()>>(
    base: &Path,
    root: &Path,
    sys: &mut S,
    owner: u32,
    authorize: F,
    pause: P,
) -> Result<Value> {
    let p = plan(base, sys, owner)?;
    // No Store lock is held while stopping/restarting; daemon can finish and release it.
    sys.command("stop")?;
    if !["inactive", "failed"].contains(&sys.show("ActiveState")?.as_str())
        || sys.show("MainPID")? != "0"
    {
        bail!("service_not_stopped / 服务或子进程尚未停止，未执行生产授权");
    }
    let since = chrono::Utc::now().timestamp();
    let result = (|| -> Result<()> {
        authorize()?;
        if p.had_mode {
            if safe_root_file(&p.mode, p.owner)? != OBSERVER {
                bail!("observation_override_changed / 观察配置在切换期间变化，拒绝删除");
            }
            fs::remove_file(&p.mode)?;
            fs::File::open(p.mode.parent().unwrap())?.sync_all()?;
        }
        sys.command("daemon-reload")?;
        let exec = sys.show("ExecStart")?;
        if exec.contains("--observe") || !exec.contains(BASE_COMMAND) {
            bail!("service_still_observing / 仍有观察模式或未知启动命令；授权将回退");
        }
        sys.command("enable")?;
        sys.command("start")?;
        sys.wait_live(root, since)?;
        Ok(())
    })();
    if let Err(e) = result {
        let stopped = sys.command("stop");
        let paused = pause();
        let restored = restore_mode(&p);
        let reload = sys.command("daemon-reload");
        if stopped.is_err() || paused.is_err() || restored.is_err() || reload.is_err() {
            return Err(e.context("cutover_rollback_incomplete / 切换失败且回退未完全完成；请检查 kairo.service 和 deployment 授权状态，禁止继续自动尝试"));
        }
        if p.was_active {
            sys.command("start")
                .context("observer_restart_failed / 授权已关闭，但观察服务未能重启；请检查日志")?;
        }
        return Err(e);
    }
    Ok(
        json!({"production_authorized":true,"service_active":true,"daemon_live_confirmed":true,"automatic_warmup_enabled":true,"forced_model_requests":0}),
    )
}
pub fn enable(root: &Path, ack: bool) -> Result<Value> {
    if !ack {
        bail!("cutover_approval_required / 需要明确生产切换批准；之后守护进程可能在满足安全门禁时发送预热请求");
    }
    if unsafe { libc::geteuid() } != 0 || fs::canonicalize(root)? != Path::new("/var/lib/kairo") {
        bail!("managed_root_required / 自动切换仅支持 root 和标准 /var/lib/kairo 安装；开发目录不会修改真实服务");
    }
    {
        let store = Store::open(root)?;
        let health = crate::recovery::inspect(&store, &rt::guard_root())?;
        if health["production_authorized"] == true && status(root)["daemon_live_confirmed"] == true
        {
            return Ok(
                json!({"already_enabled":true,"production_authorized":true,"service_active":true,"daemon_live_confirmed":true,"automatic_warmup_enabled":true,"forced_model_requests":0}),
            );
        }
        if let Some(deadline) = health["recovery"]["not_before"].as_i64() {
            if chrono::Utc::now().timestamp() < deadline {
                bail!(
                    "recovery_wait_required / 保守等待尚未结束；服务未修改，请运行 recover check"
                );
            }
            if health["recovery"]["approved"] == true && health["scheduler_state_ready"] != true {
                bail!("account_recovery_required / 之前恢复后的状态再次丢失或损坏；请重新运行 recover begin，服务未修改");
            }
        } else if health["scheduler_state_ready"] != true {
            bail!(
                "account_recovery_required / 缺少完整绑定或历史；请使用 recover wizard，服务未修改"
            );
        }
    }
    cutover(
        Path::new("/"),
        root,
        &mut Real,
        0,
        || {
            let s = Store::open(root)?;
            let id =
                rt::runtime(&s).and_then(|r| crate::identity(&r.codex_home.join("auth.json")))?;
            let recovery_path = rt::guard_root().join(&id.fingerprint).join("recovery.json");
            if crate::recovery::present(&recovery_path)? {
                let record: crate::recovery::Recovery =
                    serde_json::from_slice(&crate::private_bytes(&recovery_path, 1024 * 1024)?)?;
                record.validate(&id.fingerprint)?;
                if !record.approved {
                    crate::recovery::approve(&s)?;
                }
            }
            rt::activate(&s, &id.fingerprint, true)
        },
        || {
            let s = Store::open(root)?;
            if crate::recovery::present(&s.root.join("deployment.json"))? {
                rt::pause(&s)?;
            }
            Ok(())
        },
    )
}

fn live_record(v: &Value, pid: u32, since: i64, now: i64) -> bool {
    pid > 0
        && v["daemon_pid"].as_u64() == Some(pid as u64)
        && v["execution"] == "live"
        && v["error"].is_null()
        && v["event"] != "blocked"
        && v["at"]
            .as_i64()
            .is_some_and(|at| at >= since && at <= now && now - at <= 120)
}
/// Read-only evidence; authorization and running mode are intentionally independent.
pub fn status(root: &Path) -> Value {
    if fs::canonicalize(root).ok().as_deref() != Some(Path::new("/var/lib/kairo")) {
        return json!({"managed_service":false,"daemon_live_confirmed":false});
    }
    let read = |name: &str| {
        crate::private_bytes(&root.join(name), 1024 * 1024)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    };
    let d = read("deployment.json");
    let s = read("status.json");
    let mut sys = Real;
    let active = sys.show("ActiveState").ok().as_deref() == Some("active");
    let exec = sys.show("ExecStart").unwrap_or_default();
    let managed =
        sys.show("FragmentPath").ok().as_deref() == Some("/etc/systemd/system/kairo.service");
    let observing = exec.contains("--observe");
    let now = chrono::Utc::now().timestamp();
    let pid = sys
        .show("MainPID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let live = active
        && managed
        && exec.contains(BASE_COMMAND)
        && !observing
        && d.as_ref().is_some_and(|d| d["enabled"] == true)
        && s.as_ref().zip(d.as_ref()).is_some_and(|(s, d)| {
            d["approved_at"]
                .as_i64()
                .is_some_and(|at| live_record(s, pid, at, now))
        });
    json!({"managed_service":managed,"service_active":active,"observation_override":observing,"daemon_live_confirmed":live})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fake {
        mode: PathBuf,
        active: bool,
        fail_start: bool,
        still_observe: bool,
        confirm: bool,
        actions: Vec<String>,
    }
    impl System for Fake {
        fn show(&mut self, key: &str) -> Result<String> {
            Ok(match key {
                "FragmentPath" => "/etc/systemd/system/kairo.service".into(),
                "DropInPaths" => {
                    if self.mode.exists() {
                        "/etc/systemd/system/kairo.service.d/20-mode.conf".into()
                    } else {
                        String::new()
                    }
                }
                "ExecStart" => format!(
                    "{BASE_COMMAND}{}",
                    if self.mode.exists() || self.still_observe {
                        " --observe"
                    } else {
                        ""
                    }
                ),
                "ActiveState" => if self.active { "active" } else { "inactive" }.into(),
                "MainPID" => if self.active { "42" } else { "0" }.into(),
                _ => String::new(),
            })
        }
        fn command(&mut self, action: &str) -> Result<()> {
            self.actions.push(action.into());
            if action == "stop" {
                self.active = false;
            }
            if action == "start" {
                if self.fail_start && !self.mode.exists() {
                    bail!("mock start failure");
                }
                self.active = true;
            }
            Ok(())
        }
        fn wait_live(&mut self, _: &Path, _: i64) -> Result<()> {
            if !self.confirm {
                bail!("mock stale observer status");
            }
            Ok(())
        }
    }
    fn fixture() -> (PathBuf, Fake) {
        let base = std::env::temp_dir().join(format!(
            "kairo-cutover-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = base.join("etc/systemd/system/kairo.service.d");
        fs::create_dir_all(&dir).unwrap();
        let expected = include_str!("../../../packaging/kairo.service")
            .replace("@USER@", "kairo")
            .replace("@GROUP@", "kairo")
            .replace("@HOME@", "/home/kairo")
            .replace("@PREFIX@", "/opt/kairo");
        fs::write(base.join("etc/systemd/system/kairo.service"), expected).unwrap();
        let mode = dir.join("20-mode.conf");
        fs::write(&mode, OBSERVER).unwrap();
        (
            base,
            Fake {
                mode,
                active: true,
                fail_start: false,
                still_observe: false,
                confirm: true,
                actions: Vec::new(),
            },
        )
    }
    #[test]
    fn old_pid_or_observer_status_cannot_confirm_a_new_production_daemon() {
        let mut v = json!({"daemon_pid":41,"at":100,"execution":"live","event":"waiting"});
        assert!(!live_record(&v, 42, 100, 100));
        v["daemon_pid"] = json!(42);
        assert!(live_record(&v, 42, 100, 100));
        v["execution"] = json!("observation_only");
        assert!(!live_record(&v, 42, 100, 100));
    }
    #[test]
    fn managed_cutover_requires_live_confirmation_and_rolls_back_on_failure() {
        for failure in 0..4 {
            let (base, mut sys) = fixture();
            let authorized = std::cell::Cell::new(false);
            let paused = std::cell::Cell::new(false);
            sys.fail_start = failure == 1;
            sys.still_observe = failure == 2;
            sys.confirm = failure != 3;
            let result = cutover(
                &base,
                &base.join("local"),
                &mut sys,
                unsafe { libc::geteuid() },
                || {
                    authorized.set(true);
                    Ok(())
                },
                || {
                    paused.set(true);
                    Ok(())
                },
            );
            assert!(authorized.get());
            if failure == 0 {
                assert_eq!(result.unwrap()["daemon_live_confirmed"], true);
                assert!(!sys.mode.exists());
                assert!(!paused.get());
            } else {
                assert!(result.is_err());
                assert!(paused.get());
                assert_eq!(fs::read_to_string(&sys.mode).unwrap(), OBSERVER);
                assert!(sys.active);
            }
            fs::remove_dir_all(base).unwrap();
        }
    }
    #[test]
    fn custom_override_is_preserved_and_no_service_action_runs() {
        let (base, mut sys) = fixture();
        fs::write(
            sys.mode.parent().unwrap().join("custom.conf"),
            "synthetic custom override",
        )
        .unwrap();
        let result = cutover(
            &base,
            &base.join("local"),
            &mut sys,
            unsafe { libc::geteuid() },
            || panic!("must not authorize"),
            || Ok(()),
        );
        assert!(result.is_err());
        assert!(sys.actions.is_empty());
        assert_eq!(fs::read_to_string(&sys.mode).unwrap(), OBSERVER);
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn waiting_or_failed_authorization_restores_observation_without_removing_override() {
        let (base, mut sys) = fixture();
        let paused = std::cell::Cell::new(false);
        assert!(cutover(
            &base,
            &base.join("local"),
            &mut sys,
            unsafe { libc::geteuid() },
            || bail!("waiting"),
            || {
                paused.set(true);
                Ok(())
            }
        )
        .is_err());
        assert!(paused.get());
        assert!(sys.active);
        assert_eq!(fs::read_to_string(&sys.mode).unwrap(), OBSERVER);
        fs::remove_dir_all(base).unwrap();
    }
}
