//! Interactive presentation only: command output and scheduler gates stay unchanged.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[derive(Debug)]
pub struct EndOfInput;
impl std::fmt::Display for EndOfInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "end of input")
    }
}
impl std::error::Error for EndOfInput {}
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}
fn width() -> usize {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    // TIOCGWINSZ only reads terminal dimensions; it never changes terminal modes.
    let columns =
        if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) } == 0 {
            Some(unsafe { size.assume_init() }.ws_col as usize).filter(|n| *n > 0)
        } else {
            None
        };
    columns
        .or_else(|| std::env::var("COLUMNS").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(72)
        .clamp(20, 100)
}
fn line(s: &str) {
    println!("{}", clean(s));
}
fn page(title: &str, main: bool) {
    // ED2 clears only the visible screen. Never ED3, alternate screen or raw mode.
    if io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && std::env::var("TERM").as_deref() != Ok("dumb")
    {
        print!("\x1b[2J\x1b[H");
    }
    if main {
        println!("{}", color("[ K A I R O ]", 34));
        println!("Codex Window Scheduler");
        println!(
            "v{} | {}",
            env!("CARGO_PKG_VERSION"),
            timezone()
                .map(|t| t.to_string())
                .unwrap_or_else(|_| "?".into())
        );
    } else {
        println!("{}", color(&format!("Kairo / {title}"), 34));
    }
    println!("{}", "─".repeat(width().min(48)));
}
thread_local! { static VERIFIED: std::cell::RefCell<Option<(String,i64)>> = const { std::cell::RefCell::new(None) }; }
thread_local! { static TITLE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
pub fn action_page(_en: bool) {
    TITLE.with(|t| page(&t.borrow(), false));
}
fn pause(en: bool) -> Result<()> {
    input(t(
        en,
        "按 Enter 返回当前菜单...",
        "Press Enter to return to this menu...",
    ))?;
    Ok(())
}
fn report(result: Result<()>, en: bool) -> Result<()> {
    if let Err(e) = result {
        if e.is::<EndOfInput>() {
            return Err(e);
        }
        let (_, message) = i18n::error(&e, en);
        println!("{}", color(&format!("✗ {message}"), 31));
        // Include the actual cause, without dumping credentials or internal objects.
        if let Some(cause) = e.chain().find_map(|e| e.downcast_ref::<std::io::Error>()) {
            line(&cause.to_string());
        }
    }
    pause(en)
}
const ZH: &[&str] = &[
    "查看运行状态",
    "查询 Codex 额度",
    "管理激活时间",
    "服务管理",
    "查看运行日志",
    "OAuth 登录与账号管理",
    "系统诊断与修复",
    "预热参数设置",
    "检查更新 / 升级程序",
    "中文 / English 切换",
    "帮助与项目说明",
    "卸载 Kairo（保留状态）",
];
const EN: &[&str] = &[
    "Status",
    "Codex quota",
    "Activation schedule",
    "Service management",
    "Logs",
    "OAuth login and accounts",
    "Diagnostics and repair",
    "Prewarm settings",
    "Check updates / upgrade",
    "中文 / English",
    "Help and project information",
    "Uninstall Kairo (preserve state)",
];
fn local_bytes(cli: &Cli, name: &str) -> io::Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cli.data_dir.join(name))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Expected a regular state file"));
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(io::Error::other("State file too large"));
    }
    Ok(bytes)
}
fn redact(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase();
                if key.contains("token")
                    || key.contains("password")
                    || key.contains("secret")
                    || key.contains("api_key")
                    || key == "authorization"
                {
                    *value = json!("[redacted]");
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(array) => {
            for v in array {
                redact(v);
            }
        }
        _ => {}
    }
}
fn file_state(cli: &Cli, name: &str, en: bool) -> Option<Value> {
    match local_bytes(cli, name) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(mut v) => {
                redact(&mut v);
                Some(v)
            }
            Err(_) => {
                println!(
                    "! {name}: {}",
                    t(
                        en,
                        "损坏或格式无效；请运行 doctor，不要删除请求历史",
                        "Corrupt or invalid format; run doctor; retain request history"
                    )
                );
                None
            }
        },
        Err(e) => {
            println!(
                "! {name}: {}",
                if e.kind() == io::ErrorKind::NotFound {
                    t(en, "状态文件不存在", "State file missing")
                } else {
                    t(
                        en,
                        "读取失败；检查权限和磁盘",
                        "Read failed; check permissions and disk",
                    )
                }
            );
            None
        }
    }
}
fn service_summary(en: bool) {
    match mg::service("status") {
        Ok(v) => {
            let running = v["status"]["active_state"] == "active";
            println!(
                "{}: {}",
                t(en, "服务状态", "Service"),
                color(
                    t(
                        en,
                        if running {
                            "● 运行中"
                        } else {
                            "● 已停止"
                        },
                        if running {
                            "● Running"
                        } else {
                            "● Stopped"
                        }
                    ),
                    if running { 32 } else { 33 }
                )
            );
        }
        Err(_) => println!(
            "{}",
            color(
                t(
                    en,
                    "! 无法查询 systemd 服务状态",
                    "! Cannot query systemd service"
                ),
                33
            )
        ),
    }
}
fn summary(cli: &Cli, en: bool, detailed: bool) -> Result<()> {
    service_summary(en);
    let store = configured(cli)?;
    let d = fs::read(cli.data_dir.join("deployment.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<rt::Deployment>(&b).ok());
    let state = store.state();
    let auth = rt::runtime(&store)
        .ok()
        .and_then(|r| kairo_platform::identity(&r.codex_home.join("auth.json")).ok());
    let bound = auth
        .as_ref()
        .zip(state.as_ref().ok())
        .is_some_and(|(id, s)| id.fingerprint == s.account);
    let authorized = d.as_ref().is_some_and(|d| {
        d.schema == 1
            && d.enabled
            && d.approved_at.is_some()
            && bound
            && state.as_ref().is_ok_and(|s| s.account == d.account)
    }) && mg::validate(&store).is_ok();
    // --observe in the unit overrides deployment approval. Status alone can be stale.
    let unit = std::process::Command::new("systemctl")
        .args([
            "show",
            "kairo.service",
            "--property=ExecStart",
            "--property=Environment",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let observed = unit
        .as_ref()
        .is_none_or(|o| String::from_utf8_lossy(&o.stdout).contains("--observe"));
    let live = authorized && !observed;
    println!(
        "{}: {}",
        t(en, "调度模式", "Scheduling"),
        color(
            t(
                en,
                if live {
                    "正式运行（仍受安全门禁约束）"
                } else {
                    "只读观察"
                },
                if live {
                    "Production (safety gates apply)"
                } else {
                    "Read-only observation"
                }
            ),
            if live { 32 } else { 33 }
        )
    );
    println!(
        "{}: {}",
        t(en, "自动预热", "Automatic warmup"),
        t(
            en,
            if live { "已启用" } else { "尚未启用" },
            if live { "Enabled" } else { "Not enabled" }
        )
    );
    println!(
        "OAuth: {}",
        t(
            en,
            if auth.is_some() {
                "已发现；服务端校验请选 06 → 02"
            } else {
                "未登录或凭据无法读取"
            },
            if auth.is_some() {
                "Discovered; verify server identity via 06 → 02"
            } else {
                "Not logged in or credentials unreadable"
            }
        )
    );
    let verified = VERIFIED.with(|v| {
        v.borrow()
            .as_ref()
            .filter(|(id, at)| {
                auth.as_ref().is_some_and(|a| a.fingerprint == *id)
                    && chrono::Utc::now().timestamp() - at <= 300
            })
            .map(|(_, at)| *at)
    });
    println!(
        "{}: {}",
        t(en, "服务端身份", "Server identity"),
        verified
            .and_then(|at| local_time(at, timezone().ok()?))
            .map(|at| format!("{} ({at})", t(en, "最近校验成功", "Recently verified")))
            .unwrap_or_else(|| t(en, "待校验（06 → 02）", "Not verified (06 → 02)").into())
    );
    println!(
        "{}: {}",
        t(en, "本地账号绑定", "Local account binding"),
        t(
            en,
            if bound {
                "已绑定"
            } else {
                "未绑定或不匹配"
            },
            if bound {
                "Bound"
            } else {
                "Unbound or mismatched"
            }
        )
    );
    println!(
        "{}: {}",
        t(en, "部署授权", "Deployment approval"),
        t(
            en,
            if authorized {
                "已取得"
            } else {
                "未取得或状态无效"
            },
            if authorized {
                "Approved"
            } else {
                "Missing or invalid"
            }
        )
    );
    let status = fs::read(cli.data_dir.join("status.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    println!(
        "{}: {}",
        t(en, "下次检查", "Next check"),
        status
            .as_ref()
            .and_then(|v| v["next_check"].as_i64())
            .filter(|at| *at > chrono::Utc::now().timestamp())
            .and_then(|at| local_time(at, timezone().ok()?))
            .unwrap_or_else(|| t(
                en,
                "等待守护进程记录；计划请选 03 → 05",
                "Awaiting daemon record; planned times: 03 → 05"
            )
            .into())
    );
    if detailed {
        println!(
            "v{} | {}",
            env!("CARGO_PKG_VERSION"),
            cli.data_dir.display()
        );
        for name in [
            "deployment.json",
            "state.json",
            "cursor.json",
            "status.json",
        ] {
            file_state(cli, name, en);
        }
        println!("{}",t(en,"缺失部署/历史状态不等于 OAuth 失效。请先 06 → 02 校验、07 诊断，并核对历史；不要自动 FRESH 或删除保护记录。","Missing deployment/history does not imply invalid OAuth. Verify via 06 → 02, diagnose via 07 and review history; never automatically FRESH-bind or delete guards."));
        if store.root.join("state.json").exists() && state.is_err() {
            println!("{}",color(t(en,"✗ 调度状态无效或读取失败；请运行 doctor 并保留请求历史","✗ Invalid or unreadable scheduler state; run doctor and retain request history"),31));
        }
        if let Ok(c) = store.config() {
            println!(
                "{}: {}",
                t(en, "激活时间", "Activation times"),
                c.anchors.join(", ")
            );
        }
        if let Some(v) = status {
            println!(
                "{}: {}",
                t(en, "最近调度结果", "Latest result"),
                v["event"]
                    .as_str()
                    .map(|s| i18n::value(s, en))
                    .unwrap_or_else(
                        || t(en, "无调度结果记录", "No scheduler result recorded").into()
                    )
            );
        }
        if let Ok(s) = state {
            let v = serde_json::to_value(s)?;
            quota(
                &json!({"five_hour":v["observations"].as_array().and_then(|a|a.last()).map(|v|v["quota"].clone()).unwrap_or(Value::Null),"weekly":v["weekly"]}),
                en,
            );
        }
    }
    Ok(())
}
fn quota(v: &Value, en: bool) {
    for (key, zh, english) in [
        ("five_hour", "五小时额度", "Five-hour quota"),
        ("weekly", "每周额度", "Weekly quota"),
    ] {
        println!("{}", color(t(en, zh, english), 34));
        if let Some(used) = v[key]["used"].as_f64().filter(|n| n.is_finite()) {
            let used = used.clamp(0., 100.);
            let count = (used / 10.).round() as usize;
            println!(
                "[{}{}] {:.1}% {} / {:.1}% {}",
                "#".repeat(count),
                "-".repeat(10 - count),
                used,
                t(en, "已用", "used"),
                100. - used,
                t(en, "剩余", "remaining")
            );
            let reset = v[key]["reset_at"]
                .as_i64()
                .and_then(|at| local_time(at, timezone().ok()?))
                .unwrap_or_else(|| "?".into());
            println!("{}: {reset}", t(en, "重置时间", "Resets"));
        } else {
            println!(
                "{}",
                t(
                    en,
                    "额度未知；选择 02 查询实时额度",
                    "Quota unknown; use 02 for a live query"
                )
            );
        }
    }
}
pub fn render(v: &Value, en: bool) {
    if v.get("tag_name").is_some() {
        println!(
            "{}: v{}",
            t(en, "当前版本", "Current version"),
            env!("CARGO_PKG_VERSION")
        );
        println!(
            "{}: {}",
            t(en, "最新稳定版", "Latest stable"),
            clean(v["tag_name"].as_str().unwrap_or("?"))
        );
        println!(
            "{}: {}",
            t(en, "有可用更新", "Update available"),
            v["update_available"]
        );
        for l in v["body"].as_str().unwrap_or("").lines().take(10) {
            line(&l.chars().take(width()).collect::<String>());
        }
        line(v["html_url"].as_str().unwrap_or(""));
        return;
    }
    if v.get("five_hour").is_some() {
        if v["identity_server_verified"] == true {
            if let Some(id) = v["account_fingerprint"].as_str() {
                VERIFIED.with(|cache| {
                    *cache.borrow_mut() = Some((id.into(), chrono::Utc::now().timestamp()))
                });
            }
            println!("{}",color(t(en,"✓ 服务端身份验证成功（不代表已绑定或生产授权）","✓ Server identity verified (binding and production approval remain separate)"),32));
        }
        if v["server_denied"] == true {
            println!(
                "{}",
                color(
                    t(
                        en,
                        "! 服务端拒绝；保持保护，不发送预热请求",
                        "! Server denial; protection remains active, no warmup requests"
                    ),
                    33
                )
            );
        }
        quota(v, en);
        println!(
            "{}: {}",
            t(en, "最近查询时间", "Queried at"),
            chrono::Local::now().to_rfc3339()
        );
        return;
    }
    if v.get("codex_safety_flags").is_some() {
        for key in [
            "systemd",
            "codex_safety_flags",
            "oauth_present",
            "state_valid",
        ] {
            let ok = v[key] == true;
            println!(
                "{} {}",
                color(if ok { "✓" } else { "✗" }, if ok { 32 } else { 31 }),
                i18n::label(key, en)
            );
            if !ok {
                println!("  {}",t(en,"检查安装、认证路径和文件权限；保留历史后再修复","Check installation, auth paths and permissions; retain history before repair"));
            }
        }
        return;
    }
    if v.get("idle_window_prewarm").is_some() {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("state");
        }
        i18n::render(&v, en, 0);
        return;
    }
    i18n::render(v, en, 0);
}
fn execute(cli: &Cli, cmd: Cmd, en: bool) -> Result<()> {
    if let Cmd::Service { ref action } = cmd {
        mg::service(action)?;
        let v = mg::service("status")?;
        let state = v["status"]["active_state"].as_str().unwrap_or("unknown");
        if (action == "start" || action == "restart") && state != "active" {
            bail!("service_not_active / 命令已返回，但服务尚未运行，请查看日志");
        }
        if action == "stop" && state != "inactive" && state != "failed" {
            bail!("service_not_stopped / 服务尚未完全停止，请查看日志");
        }
        println!(
            "{}",
            color(
                match action.as_str() {
                    "start" => t(en, "✓ Kairo 服务启动成功", "✓ Kairo service started"),
                    "stop" => t(en, "✓ Kairo 服务已停止", "✓ Kairo service stopped"),
                    "restart" => t(en, "✓ Kairo 服务重启成功", "✓ Kairo service restarted"),
                    _ => t(en, "✓ 服务状态已查询", "✓ Service state queried"),
                },
                32
            )
        );
        println!(
            "{}: {}",
            t(en, "当前服务状态", "Current service state"),
            i18n::value(state, en)
        );
        println!(
            "{}",
            color(
                t(
                    en,
                    "启动服务不等于取得自动预热授权",
                    "Starting the service does not authorize automatic warmup"
                ),
                33
            )
        );
        return Ok(());
    }
    let diagnostics = matches!(cmd, Cmd::Doctor { .. });
    run(cli, cmd)?;
    if diagnostics {
        for name in ["deployment.json", "state.json", "cursor.json"] {
            file_state(cli, name, en);
        }
        println!("{}",t(en,"缺失部署/历史时继续观察；先核查历史再决定绑定或迁移，不要自动 FRESH。","Missing deployment/history stays in observation; review history before binding or migration; never automatically FRESH-bind."));
    }
    Ok(())
}
pub fn latest() -> Result<Value> {
    if std::env::var_os("KAIRO_OFFLINE").is_some() {
        bail!("offline_mode / 离线测试模式禁止网络请求");
    }
    let out = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--max-time",
            "15",
            "--max-filesize",
            "262144",
            "https://api.github.com/repos/hyd1aa/kairo-codex-scheduler/releases/latest",
        ])
        .output()?;
    if !out.status.success() {
        bail!("release_query_failed / GitHub 最新版本查询失败，请检查网络");
    }
    let mut v: Value = serde_json::from_slice(&out.stdout)?;
    let tag = v["tag_name"]
        .as_str()
        .context("release_invalid / 发布信息无效")?;
    let version = |s: &str| -> Option<Vec<u64>> {
        s.trim_start_matches('v')
            .split('.')
            .map(str::parse)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()
    };
    v["update_available"] = json!(version(tag)
        .zip(version(env!("CARGO_PKG_VERSION")))
        .is_some_and(|(a, b)| a > b));
    Ok(v)
}
fn logs(_cli: &Cli, en: bool) -> Result<()> {
    let mut offset = 0usize;
    let mut errors = false;
    let mut details = false;
    loop {
        action_page(en);
        let result = (|| -> Result<()> {
            let mut command = std::process::Command::new("journalctl");
            command.args([
                "--unit=kairo.service",
                "--no-pager",
                "--output=json",
                "--reverse",
                "--lines",
                "200",
            ]);
            if errors {
                command.arg("--priority=warning");
            }
            let o = command.output()?;
            if !o.status.success() {
                bail!("logs_failed / 无法读取日志；请检查日志权限");
            }
            let rows: Vec<Value> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(serde_json::from_str)
                .collect::<std::result::Result<_, _>>()?;
            offset = offset.min(rows.len().saturating_sub(1) / 20 * 20);
            println!(
                "{} {} / {}",
                t(en, "页", "Page"),
                offset / 20 + 1,
                rows.len().div_ceil(20).max(1)
            );
            if rows.is_empty() {
                println!(
                    "{}",
                    t(en, "没有匹配的日志记录", "No matching journal records")
                );
            }
            for row in rows.iter().skip(offset).take(20) {
                let msg = row["MESSAGE"].as_str().unwrap_or("");
                let parsed = serde_json::from_str::<Value>(msg).ok();
                let short = parsed
                    .as_ref()
                    .and_then(|v| {
                        v["error"]
                            .as_str()
                            .or_else(|| v["event"].as_str())
                            .or_else(|| v["message"].as_str())
                    })
                    .unwrap_or_else(|| {
                        if parsed.is_some() {
                            t(
                                en,
                                "结构化日志；选择详细信息查看",
                                "Structured record; toggle details to inspect",
                            )
                        } else {
                            msg
                        }
                    });
                let priority = row["PRIORITY"]
                    .as_str()
                    .unwrap_or("6")
                    .parse::<u8>()
                    .unwrap_or(6);
                let priority = if parsed.as_ref().is_some_and(|v| !v["error"].is_null()) {
                    3
                } else {
                    priority
                };
                let text = if details {
                    if let Some(mut v) = parsed.clone() {
                        redact(&mut v);
                        clean(&v.to_string())
                    } else {
                        clean(msg)
                    }
                } else {
                    clean(&i18n::value(short, en))
                };
                let stamp = row["__REALTIME_TIMESTAMP"]
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .and_then(chrono::DateTime::from_timestamp_micros)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                println!(
                    "{}",
                    color(
                        &format!(
                            "{stamp} {}",
                            text.chars()
                                .take(if details {
                                    width() * 2
                                } else {
                                    width().saturating_sub(13)
                                })
                                .collect::<String>()
                        ),
                        if priority <= 3 {
                            31
                        } else if priority <= 4 {
                            33
                        } else {
                            90
                        }
                    )
                );
            }
            Ok(())
        })();
        if let Err(e) = result {
            let (_, m) = i18n::error(&e, en);
            println!("{}", color(&m, 31));
        }
        match numbered(
            en,
            &[
                "刷新",
                "下一页",
                "上一页",
                "错误/警告日志切换",
                "详细信息切换",
            ],
            &[
                "Refresh",
                "Next page",
                "Previous page",
                "Toggle errors/warnings",
                "Toggle details",
            ],
            t(en, "返回", "Back"),
        )?
        .trim_start_matches('0')
        {
            "" => return Ok(()),
            "1" => offset = 0,
            "2" => offset = (offset + 20).min(180),
            "3" => offset = offset.saturating_sub(20),
            "4" => {
                errors = !errors;
                offset = 0;
            }
            "5" => details = !details,
            _ => report(
                Err(anyhow::anyhow!("menu_choice_invalid / 菜单选项无效")),
                en,
            )?,
        }
    }
}
fn section(cli: &mut Cli, en: bool, n: usize) -> Result<()> {
    let title = if en { EN[n - 1] } else { ZH[n - 1] };
    TITLE.with(|t| *t.borrow_mut() = title.into());
    if n == 5 {
        return logs(cli, en);
    }
    loop {
        page(title, false);
        match n {
            1 => {
                if let Err(e) = summary(cli, en, true) {
                    let (_, m) = i18n::error(&e, en);
                    println!("{}", color(&m, 31));
                }
                match submenu(
                    en,
                    &["刷新摘要", "查看详细状态（JSON）"],
                    &["Refresh summary", "Detailed status (JSON)"],
                )?
                .trim_start_matches('0')
                {
                    "" => return Ok(()),
                    "1" => continue,
                    "2" => {
                        // Read only local records; status CLI remains independently available.
                        let mut v = serde_json::Map::new();
                        for name in [
                            "config.json",
                            "runtime.json",
                            "accounts.json",
                            "deployment.json",
                            "state.json",
                            "cursor.json",
                            "status.json",
                        ] {
                            if let Some(data) = file_state(cli, name, en) {
                                v.insert(name.into(), data);
                            }
                        }
                        println!("{}", serde_json::to_string_pretty(&v)?);
                        pause(en)?;
                    }
                    _ => unreachable!(),
                }
            }
            2 => {
                match submenu(
                    en,
                    &["查询实时额度（只读）"],
                    &["Query live quota (read only)"],
                )?
                .trim_start_matches('0')
                {
                    "" => return Ok(()),
                    _ => report(execute(cli, Cmd::Quota { fixture: None }, en), en)?,
                }
            }
            11 => {
                match submenu(en,&["基础使用","首次授权与安全说明","项目与版本"],&["Basic usage","First authorization and safety","Project and version"])?.trim_start_matches('0') {
                    ""=>return Ok(()),"1"=>println!("{}",t(en,"数字 + Enter 进入页面；00 返回上一级。操作后 Enter 留在当前菜单。支持 kairo status / quota / --help 及 --json。","Number + Enter opens a page; 00 returns one level. Enter after actions stays in this menu. Commands: kairo status / quota / --help and --json.")),
                    "2"=>{run(cli,Cmd::HelpInfo)?; println!("{}",t(en,"登录、服务端校验、本地绑定、生产授权是四个独立步骤。缺失旧状态时先核查历史，不要自动 FRESH。","OAuth discovery, server verification, local binding and production approval are separate steps. Review history before any fresh binding."));},
                    "3"=>println!("Kairo v{}\nhttps://github.com/hyd1aa/kairo-codex-scheduler",env!("CARGO_PKG_VERSION")),_=>unreachable!()
                }
                pause(en)?;
            }
            _ => {
                if n == 4 {
                    service_summary(en);
                    let out = std::process::Command::new("systemctl")
                        .args(["is-enabled", "kairo.service"])
                        .output();
                    println!(
                        "{}: {}",
                        t(en, "开机自启", "Boot startup"),
                        out.ok()
                            .map(|o| clean(&String::from_utf8_lossy(&o.stdout)))
                            .unwrap_or_else(|| "?".into())
                    );
                }
                if n == 6 {
                    summary(cli, en, false)?;
                }
                if n == 8 {
                    let s = configured(cli)?;
                    let c = s.config()?;
                    println!(
                        "{}: {} | {}: {}s",
                        t(en, "空闲预热", "Idle warmup"),
                        c.idle_window_prewarm,
                        t(en, "缓冲", "Buffer"),
                        c.buffer_seconds
                    );
                }
                if n == 9 {
                    println!(
                        "{}: v{}",
                        t(en, "当前版本", "Current version"),
                        env!("CARGO_PKG_VERSION")
                    );
                }
                let command = choose(en, &format!("{n:02}"));
                match command {
                    Ok(None) => return Ok(()),
                    Ok(Some(cmd)) => report(execute(cli, cmd, en), en)?,
                    Err(e) => report(Err(e), en)?,
                }
            }
        }
    }
}
pub fn menu(cli: &mut Cli) -> Result<()> {
    if !io::stdin().is_terminal() {
        bail!("subcommand_required / 非交互终端请使用子命令");
    }
    ACTIVE.store(true, Ordering::Relaxed);
    let result = (|| -> Result<()> {
        loop {
            let c = {
                let s = configured(cli)?;
                s.config().unwrap_or_default()
            };
            let en = c.language == "en-US";
            cli.lang = Some(c.language);
            page("", true);
            if let Err(e) = summary(cli, en, false) {
                let (_, m) = i18n::error(&e, en);
                println!("{}", color(&m, 33));
            }
            let choice = numbered(en, ZH, EN, t(en, "退出 Kairo", "Exit Kairo"))?;
            let n = choice.parse::<usize>().unwrap_or(usize::MAX);
            if n == 0 {
                return Ok(());
            }
            if n == 10 {
                run(
                    cli,
                    Cmd::Lang {
                        language: if en { "zh-CN" } else { "en-US" }.into(),
                    },
                )?;
                continue;
            }
            if (1..=12).contains(&n) {
                if n == 12 {
                    TITLE.with(|t| *t.borrow_mut() = if en { EN[11] } else { ZH[11] }.into());
                    action_page(en);
                    report(
                        choose(en, "12").and_then(|c| c.map_or(Ok(()), |c| execute(cli, c, en))),
                        en,
                    )?;
                } else {
                    section(cli, en, n)?;
                }
            } else {
                report(
                    Err(anyhow::anyhow!("menu_choice_invalid / 菜单选项无效")),
                    en,
                )?;
            }
        }
    })();
    ACTIVE.store(false, Ordering::Relaxed);
    match result {
        Err(e) if e.is::<EndOfInput>() => Ok(()),
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_redaction_covers_nested_credentials_without_hiding_scheduler_state() {
        let mut value = json!({"event":"observation_only","nested":[{"refresh_token":"synthetic-sensitive-value","access_token":"synthetic-access"}],"Authorization":"synthetic-auth","account":"synthetic-account"});
        redact(&mut value);
        assert_eq!(value["event"], "observation_only");
        assert_eq!(value["account"], "synthetic-account");
        assert!(!value.to_string().contains("synthetic-sensitive-value"));
        assert!(!value.to_string().contains("synthetic-access"));
        assert!(!value.to_string().contains("synthetic-auth"));
    }
    #[test]
    fn external_text_cannot_inject_terminal_controls() {
        assert_eq!(clean("event\x1b[3J\r\n"), "event[3J");
    }
}
