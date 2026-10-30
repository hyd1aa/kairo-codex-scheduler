//! Interactive presentation only: command output and scheduler gates stay unchanged.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static DETAILS: AtomicBool = AtomicBool::new(false);
pub fn details() -> bool {
    DETAILS.load(Ordering::Relaxed)
}
pub fn detail_output(cli: &Cli, mut value: Value) -> Result<()> {
    // Keep raw lifecycle/history accessible only in the explicitly requested detail page.
    for (key, name) in [
        ("saved_state", "state.json"),
        ("saved_cursor", "cursor.json"),
    ] {
        if let Ok(bytes) = local_bytes(cli, name) {
            if let Ok(record) = serde_json::from_slice::<Value>(&bytes) {
                value[key] = record;
            }
        }
    }
    redact(&mut value);
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
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
fn summary(cli: &Cli, en: bool, detailed: bool, compact: bool) -> Result<()> {
    service_summary(en);
    let store = configured(cli)?;
    let state = store.state();
    let auth = rt::runtime(&store)
        .ok()
        .and_then(|r| kairo_platform::identity(&r.codex_home.join("auth.json")).ok());
    let bound = auth
        .as_ref()
        .zip(state.as_ref().ok())
        .is_some_and(|(id, s)| id.fingerprint == s.account);
    let health = kairo_platform::recovery::inspect(&store, &rt::guard_root())?;
    let authorized = health["production_authorized"] == true;
    let service_mode = kairo_platform::activation::status(&store.root);
    let live = authorized && service_mode["daemon_live_confirmed"] == true;
    let requested = store
        .read::<rt::Deployment>("deployment.json")
        .is_ok_and(|d| d.enabled);
    let uncertain = requested && !live && service_mode["observation_override"] != true;
    if compact {
        let intent = store
            .read::<kairo_platform::onboarding::Intent>("onboarding.json")
            .ok();
        let auto = store
            .read::<Value>("onboarding-status.json")
            .unwrap_or_default();
        let (zh, english, code) = if live {
            ("自动预热运行中", "Automatic warmup running", 32)
        } else if auth.is_none() {
            (
                "需要登录 Codex（03 账号管理）",
                "Log in to Codex (03 Account)",
                33,
            )
        } else if intent.as_ref().is_some_and(|i| !i.requested) {
            ("已暂停", "Paused", 33)
        } else if auto["status"] == "exclusive_account_confirmation_required"
            || auto["status"] == "account_changed_confirmation_required"
        {
            (
                "需要一次确认（04 启动预热）",
                "One confirmation required (04 Start)",
                33,
            )
        } else if health["classification"] == "recovery_waiting" {
            ("等待安全恢复", "Waiting for safe recovery", 33)
        } else if uncertain {
            (
                "运行状态待确认；可能执行请求",
                "Execution unconfirmed; requests possible",
                33,
            )
        } else {
            ("尚未启用（04 启动预热）", "Not enabled (04 Start)", 33)
        };
        println!(
            "{}: {}",
            t(en, "状态", "Status"),
            color(t(en, zh, english), code)
        );
        println!(
            "{}: {}",
            t(en, "账号", "Account"),
            t(
                en,
                if auth.is_some() {
                    "已发现 Codex 登录"
                } else {
                    "未登录"
                },
                if auth.is_some() {
                    "Codex login discovered"
                } else {
                    "Not logged in"
                }
            )
        );
        if let Some(deadline) = health["effective_not_before"].as_i64() {
            println!(
                "{}: {}",
                t(en, "预计复查", "Estimated recheck"),
                local_time(deadline, timezone()?).unwrap_or_else(|| deadline.to_string())
            );
            if intent
                .as_ref()
                .is_some_and(|i| i.requested && i.exclusive_account)
            {
                println!(
                    "{}",
                    t(
                        en,
                        "恢复方式：自动复查并启用；无需人工操作",
                        "Recovery: automatic recheck and enable; no manual action needed"
                    )
                );
            }
        }
        if let Some(reason) = auto["reason"].as_str() {
            println!("{}: {}", t(en, "提示", "Notice"), clean(reason));
        }
        let cache = store
            .read::<Value>("quota-cache.json")
            .ok()
            .filter(|v| auth.as_ref().is_some_and(|a| v["account"] == a.fingerprint));
        if let Some(v) = cache.filter(|v| {
            v["at"].as_i64().is_some_and(|at| {
                chrono::Utc::now().timestamp() - at <= 300 && at <= chrono::Utc::now().timestamp()
            })
        }) {
            if let Some(used) = v["five_hour"]["used"].as_f64() {
                println!(
                    "{}: {:.1}%",
                    t(en, "五小时额度剩余", "Five-hour remaining"),
                    (100.0 - used).clamp(0.0, 100.0)
                );
            }
        } else {
            println!(
                "{}",
                t(
                    en,
                    "五小时额度：待查询（01）",
                    "Five-hour quota: query via01"
                )
            );
        }
        let tz = timezone()?;
        let next = next_checks(&store, &store.config()?, 48)?;
        if let Some(at) = next["checks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v["at"].as_i64())
        {
            println!(
                "{}: {}",
                t(en, "下次检查", "Next check"),
                local_time(at, tz).unwrap_or_default()
            );
        }
        return Ok(());
    }

    println!(
        "{}: {}",
        t(en, "调度模式", "Scheduling"),
        color(
            t(
                en,
                if live {
                    "正式运行（仍受安全门禁约束）"
                } else if uncertain {
                    "已授权但运行未确认；可能执行请求"
                } else {
                    "只读观察"
                },
                if live {
                    "Production (safety gates apply)"
                } else if uncertain {
                    "Authorized but unconfirmed; requests may execute"
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
            if live {
                "已启用"
            } else if uncertain {
                "未确认（不等于安全暂停）"
            } else {
                "尚未启用"
            },
            if live {
                "Enabled"
            } else if uncertain {
                "Unconfirmed (not a safe pause)"
            } else {
                "Not enabled"
            }
        )
    );
    if compact {
        let account = if auth.is_none() {
            t(
                en,
                "未登录或 OAuth 无法读取",
                "Not logged in / OAuth unreadable",
            )
        } else if !bound {
            t(en, "OAuth 已发现，未绑定", "OAuth discovered, unbound")
        } else {
            t(
                en,
                "本地已绑定；服务端校验见 03",
                "Locally bound; verify server via 03",
            )
        };
        println!("{}: {account}", t(en, "账号状态", "Account"));
    } else {
        println!(
            "OAuth: {}",
            t(
                en,
                if auth.is_some() {
                    "已发现；服务端校验请选 03 → 02"
                } else {
                    "未登录或凭据无法读取"
                },
                if auth.is_some() {
                    "Discovered; verify server identity via 03 → 02"
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
                .unwrap_or_else(|| t(en, "待校验（03 → 02）", "Not verified (03 → 02)").into())
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
    }
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
    if !compact {
        render_recovery(&health, en);
    }
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
        println!("{}",t(en,"缺失部署/历史状态不等于 OAuth 失效。旧账号请进入 06 → 09 恢复向导，不要 FRESH 或删除保护记录。","Missing deployment/history does not imply invalid OAuth. Existing accounts: use recovery wizard 06 → 09; never FRESH-bind or delete guards."));
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
                    31
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
                if key == "state_valid" {
                    t(
                        en,
                        "调度状态完整就绪（不代表生产授权）",
                        "Complete scheduler readiness (not production approval)",
                    )
                    .into()
                } else {
                    i18n::label(key, en)
                }
            );
            if !ok {
                println!("  {}",t(en,"检查安装、认证路径和文件权限；保留历史后再修复","Check installation, auth paths and permissions; retain history before repair"));
            }
        }
        println!(
            "{}: {}",
            t(
                en,
                "本地已有配置/记录可校验（缺失不算就绪）",
                "Existing local records validate (absence is not readiness)"
            ),
            v["existing_records_valid"]
        );
        render_recovery(&v["recovery_status"], en);
        return;
    }
    if v.get("idle_window_prewarm").is_some() {
        let mut settings = v.clone();
        if let Some(o) = settings.as_object_mut() {
            o.remove("state");
        }
        i18n::render(&settings, en, 0);
        if v["state"].is_object() {
            let state = &v["state"];
            println!(
                "{}",
                color(
                    t(
                        en,
                        "窗口生命周期（最近保存状态）",
                        "Window lifecycle (last saved state)"
                    ),
                    34
                )
            );
            let at = |key: &str| {
                state[key]
                    .as_i64()
                    .filter(|at| *at > 0)
                    .and_then(|at| local_time(at, timezone().ok()?))
            };
            i18n::render(
                &json!({"positive_until":at("positive_until"),"pending":at("pending"),"recent_reason":state["attempts"].as_array().and_then(|a|a.last()).map(|a|a["outcome"].clone())}),
                en,
                0,
            );
            let denied = state["server_denied"] == true;
            println!(
                "{}: {}",
                i18n::label("server_denied", en),
                color(
                    t(
                        en,
                        if denied {
                            "已拒绝（自动请求受保护）"
                        } else {
                            "未拒绝"
                        },
                        if denied {
                            "Denied (automatic requests blocked)"
                        } else {
                            "Not denied"
                        }
                    ),
                    if denied { 31 } else { 32 }
                )
            );
            quota(
                &json!({"weekly":state["weekly"],"five_hour":state["observations"].as_array().and_then(|a|a.last()).map(|o|o["quota"].clone())}),
                en,
            );
        } else {
            println!("{}",color(t(en,"! 调度历史缺失、无效或无法读取；请在 01 查看原因，保留保护记录","! Scheduler history is missing, invalid or unreadable; inspect 01 and retain guards"),33));
        }
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
            if state == "active" {
                t(en, "运行中", "Running").into()
            } else {
                i18n::value(state, en)
            }
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
        println!(
            "{}",
            t(
                en,
                "doctor 报告不等于生产授权；首次启用与旧账号恢复请进入 06 → 09",
                "A doctor report is not production approval; first use and recovery: 06 -> 09"
            )
        );
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
                if let Err(e) = summary(cli, en, true, false) {
                    let (_, m) = i18n::error(&e, en);
                    println!("{}", color(&m, 31));
                }
                match submenu(
                    en,
                    &["刷新摘要", "查看详细状态（JSON）", "查询实时额度（只读）"],
                    &[
                        "Refresh summary",
                        "Detailed status (JSON)",
                        "Query live quota (read only)",
                    ],
                )?
                .trim_start_matches('0')
                {
                    "" => return Ok(()),
                    "1" => continue,
                    "3" => report(execute(cli, Cmd::Quota { fixture: None }, en), en)?,
                    "2" => {
                        DETAILS.store(true, Ordering::Relaxed);
                        let result = run(cli, Cmd::Status);
                        DETAILS.store(false, Ordering::Relaxed);
                        report(result, en)?;
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
                    summary(cli, en, false, false)?;
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
                    Ok(Some(Cmd::Recover {
                        action: RecoveryAction::Wizard,
                    })) => recovery_wizard(cli)?,
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
            if let Err(e) = summary(cli, en, false, true) {
                let (_, m) = i18n::error(&e, en);
                println!("{}", color(&m, 33));
            }
            let choice = numbered(
                en,
                &[
                    "查看状态与额度",
                    "设置预热时间",
                    "Codex 账号管理",
                    "启动 / 暂停预热",
                    "检查更新",
                    "高级设置",
                ],
                &[
                    "Status and quota",
                    "Warmup schedule",
                    "Codex account",
                    "Start / pause warmup",
                    "Check updates",
                    "Advanced settings",
                ],
                t(en, "退出 Kairo", "Exit Kairo"),
            )?;
            let n = choice.parse::<usize>().unwrap_or(usize::MAX);
            if n == 0 {
                return Ok(());
            }
            match n {
                1 => section(cli, en, 1)?,
                2 => section(cli, en, 3)?,
                3 => section(cli, en, 6)?,
                4 => warmup_menu(cli, en)?,
                5 => section(cli, en, 9)?,
                6 => advanced(cli, en)?,
                _ => report(
                    Err(anyhow::anyhow!("menu_choice_invalid / 菜单选项无效")),
                    en,
                )?,
            }
        }
    })();
    ACTIVE.store(false, Ordering::Relaxed);
    match result {
        Err(e) if e.is::<EndOfInput>() => Ok(()),
        r => r,
    }
}

pub fn render_recovery(v: &Value, en: bool) {
    if let Some(class) = v["classification"].as_str() {
        let message=match class {
            "oauth_missing"=>t(en,"未发现有效 OAuth；请先人工登录并进行服务端校验","No valid local OAuth; log in and verify the server identity"),
            "data_corrupt"=>t(en,"记录损坏或读取失败；本地恢复会保留证据，全局门禁损坏必须人工核查备份","Corrupt or unreadable records; local recovery preserves evidence; corrupt global guards require backup review"),
            "recovery_waiting"=>t(en,"已有账号恢复中：历史仍未知，等待结束后必须再次校验及明确授权","Existing-account recovery: history remains unknown; recheck and explicitly approve after waiting"),
            "not_authorized"=>t(en,"调度状态就绪，但尚未生产授权；请在向导中明确启用","Scheduler records ready, production not authorized; explicitly enable in the wizard"),
            "authorized"=>t(en,"已取得生产授权；是否真正运行请检查服务与新的 live 状态","Production authorized; verify the service and a fresh live status to confirm actual operation"),
            _=>t(en,"调度历史缺失或不完整；不能凭额度判断新旧账号。旧账号请恢复；仅从未运行的账号可声明 FRESH","Scheduler history missing/incomplete; quota cannot prove an account is new. Recover existing accounts; only never-used accounts may declare FRESH"),
        };
        println!("{}", color(message, 33));
        if let Some(files) = v["files"].as_object() {
            for (file, value) in files {
                if value != "valid" {
                    println!(
                        "! {file}: {}",
                        t(
                            en,
                            if value == "missing" {
                                "文件缺失"
                            } else {
                                "无效或无法读取"
                            },
                            if value == "missing" {
                                "Missing"
                            } else {
                                "Invalid or unreadable"
                            }
                        )
                    );
                }
            }
        }
        if v["account_guard_valid"] != true {
            println!(
                "{}",
                t(
                    en,
                    "! 账号保护记录未就绪；请勿清空或当作 FRESH",
                    "! Account guard not ready; never clear it or assume FRESH"
                )
            );
        }
    }
    let recovery = &v["recovery"];
    if recovery["approved"] == true {
        println!("{}",color(t(en,"✓ 恢复基线已持久化；旧历史仍未知，实际授权和服务模式以 status 为准","✓ Recovery baseline persisted; old history remains unknown; use status to confirm approval and actual service mode"),32));
    } else if recovery.is_object() {
        let now = chrono::Utc::now().timestamp();
        let at = v["effective_not_before"]
            .as_i64()
            .or_else(|| recovery["not_before"].as_i64());
        println!(
            "{}: {}",
            t(
                en,
                "未知历史保守等待截止",
                "Unknown-history quarantine until"
            ),
            at.and_then(|at| local_time(at, timezone().ok()?))
                .unwrap_or_else(|| "?".into())
        );
        println!("{}",t(en,"服务重启保留等待；系统重启保留已确认进度，关机时间不计入等待","Service restarts preserve waiting; reboots retain credited progress, excluding powered-off time"));
        println!(
            "{}: {}",
            t(
                en,
                "剩余秒数（活动可能延长）",
                "Remaining seconds (activity can extend)"
            ),
            at.unwrap_or(now).saturating_sub(now).max(0)
        );
        println!("{}",t(en,"旧历史仍未知。已选择自动启用的账号由后台复查并启用；否则请在主菜单04选择启动。","Old history stays unknown. Opted-in accounts recheck and enable in the background; otherwise choose Start in main menu04."));
    }
    if v.get("five_hour").is_some() {
        quota(v, en);
    } else if v.get("ready_for_explicit_approval").is_some() && recovery.is_object() {
        quota(
            &json!({"five_hour":recovery["five"],"weekly":recovery["weekly"]}),
            en,
        );
        println!(
            "{}: {}",
            t(en, "最近只读校验", "Latest read-only verification"),
            recovery["checked_at"]
                .as_i64()
                .and_then(|at| local_time(at, timezone().ok()?))
                .unwrap_or_else(|| "?".into())
        );
    }
    if v["automatic_warmup_enabled"] == true && v["daemon_live_confirmed"] == true {
        println!("{}",color(t(en,"✓ 已确认正式运行，自动预热已启用；仍受额度、窗口及全局防重保护，不强制测试模型请求","✓ Production daemon confirmed; automatic warmup enabled with quota/window/global guards; no forced model test"),32));
    } else if v.get("automatic_warmup_enabled").is_some() {
        println!("{}",color(t(en,"自动预热尚未启用；准备、复查及等待不会授权模型请求","Automatic warmup is not enabled; preparation, rechecks and waiting do not authorize model requests"),33));
    }
}
pub fn recovery_wizard(cli: &Cli) -> Result<()> {
    if !io::stdin().is_terminal() || cli.json {
        bail!("interactive_wizard_required / 向导需要交互终端；脚本请使用 recover status、begin、check 和 enable");
    }
    let en = cli.lang.as_deref() == Some("en-US");
    let previous_active = ACTIVE.swap(true, Ordering::Relaxed);
    let previous = TITLE.with(|t| t.replace(titled(en)));
    let result = (|| -> Result<()> {
        loop {
            action_page(en);
            let state = {
                let store = configured(cli)?;
                kairo_platform::recovery::inspect(&store, &rt::guard_root())?
            };
            render_recovery(&state, en);
            let choice = submenu(
                en,
                &[
                    "OAuth 人工登录",
                    "服务端校验身份与额度",
                    "首次新账号绑定（FRESH，仅从未运行）",
                    "旧账号历史丢失：准备保守恢复",
                    "复查恢复等待及最新额度",
                    "明确授权并验证正式运行",
                    "查看真实运行状态",
                ],
                &[
                    "Human OAuth login",
                    "Verify server identity and quota",
                    "First account binding (FRESH, never used only)",
                    "Existing account: begin unknown-history recovery",
                    "Recheck recovery wait and latest quota",
                    "Explicitly authorize and verify production",
                    "Show actual runtime status",
                ],
            )?;
            let cmd = match choice.trim_start_matches('0') {
                "" => return Ok(()),
                "1" => Cmd::Auth {
                    action: "login".into(),
                    name: None,
                    home: None,
                    acknowledge_fresh: false,
                },
                "2" => Cmd::Auth {
                    action: "verify".into(),
                    name: None,
                    home: None,
                    acknowledge_fresh: false,
                },
                "3" => {
                    let ack=input(t(en,"仅确认从未运行过本账号，才输入 FRESH；旧账号禁止使用 > ","Type FRESH only if this account has never run Kairo; existing accounts must not use it > "))?=="FRESH";
                    Cmd::Auth {
                        action: "bind".into(),
                        name: None,
                        home: None,
                        acknowledge_fresh: ack,
                    }
                }
                "4" => {
                    let ack=input(t(en,"先停止其他设备/调度器的自动与手动调用。确认旧历史未知，输入 RECOVER > ","Stop automatic/manual requests on other devices/schedulers. Acknowledge unknown history: type RECOVER > "))?=="RECOVER";
                    Cmd::Recover {
                        action: RecoveryAction::Begin {
                            acknowledge_unknown_history: ack,
                        },
                    }
                }
                "5" => Cmd::Recover {
                    action: RecoveryAction::Check,
                },
                "6" => {
                    let ack=input(t(en,"等待结束并校验通过后，将切换真实服务，未来可能发送预热请求。输入 ENABLE 明确授权 > ","After waiting/verification, switches the real service and permits future warmup requests. Type ENABLE to approve > "))?=="ENABLE";
                    Cmd::Enable {
                        approve_production_cutover: ack,
                    }
                }
                "7" => {
                    report(summary(cli, en, true, false), en)?;
                    continue;
                }
                _ => unreachable!(),
            };
            report(run(cli, cmd), en)?;
        }
    })();
    TITLE.with(|t| *t.borrow_mut() = previous);
    ACTIVE.store(previous_active, Ordering::Relaxed);
    result
}
fn titled(en: bool) -> String {
    t(
        en,
        "首次启用与已有账号恢复",
        "First use and existing-account recovery",
    )
    .into()
}

fn warmup_menu(cli: &Cli, en: bool) -> Result<()> {
    loop {
        TITLE.with(|t| *t.borrow_mut() = t_string(en, "启动 / 暂停预热", "Start / pause warmup"));
        page(&t_string(en, "自动预热", "Automatic warmup"), false);
        let choice = submenu(
            en,
            &["启动：自动初始化及安全恢复", "暂停预热"],
            &[
                "Start: automatic initialization and safe recovery",
                "Pause warmup",
            ],
        )?;
        match choice.trim_start_matches('0') {
            "" => return Ok(()),
            "1" => {
                println!("{}",t(en,"继续表示授权安全条件满足后自动预热。无法检查其他机器：请确认同账号的其他调度器已停止，恢复等待期间避免手动请求。","Continuing authorizes automatic warmup once safe. Other machines cannot be checked: stop other schedulers for this account and avoid manual requests during recovery."));
                let confirmed = {
                    let s = configured(cli)?;
                    let account = rt::runtime(&s)
                        .and_then(|r| kairo_platform::identity(&r.codex_home.join("auth.json")))
                        .ok();
                    s.read::<kairo_platform::onboarding::Intent>("onboarding.json")
                        .is_ok_and(|i| {
                            i.exclusive_account
                                && account.as_ref().is_some_and(|a| {
                                    i.account.as_deref() == Some(a.fingerprint.as_str())
                                })
                        })
                };
                if confirmed
                    || input(t(
                        en,
                        "确认此账号由本机独占调度？[yes/N]: ",
                        "Confirm exclusive scheduling? [yes/N]: ",
                    ))?
                    .eq_ignore_ascii_case("yes")
                {
                    report(
                        run(
                            cli,
                            Cmd::Onboard {
                                request: true,
                                confirm_exclusive_account: true,
                            },
                        ),
                        en,
                    )?;
                }
            }
            "2" => report(run(cli, Cmd::Pause), en)?,
            _ => {}
        }
    }
}
fn t_string(en: bool, zh: &str, english: &str) -> String {
    t(en, zh, english).into()
}
fn advanced(cli: &mut Cli, en: bool) -> Result<()> {
    loop {
        let title = t(en, "高级设置", "Advanced settings");
        TITLE.with(|t| *t.borrow_mut() = title.into());
        page(title, false);
        let choice = submenu(
            en,
            &[
                "运行日志",
                "系统诊断",
                "安全恢复（维护）",
                "预热参数",
                "服务管理",
                "中文 / English",
                "帮助",
                "卸载 Kairo",
            ],
            &[
                "Logs",
                "Diagnostics",
                "Recovery (maintenance)",
                "Warmup parameters",
                "Service management",
                "中文 / English",
                "Help",
                "Uninstall Kairo",
            ],
        )?;
        match choice.trim_start_matches('0') {
            "" => return Ok(()),
            "1" => section(cli, en, 5)?,
            "2" => section(cli, en, 7)?,
            "3" => recovery_wizard(cli)?,
            "4" => section(cli, en, 8)?,
            "5" => section(cli, en, 4)?,
            "6" => {
                run(
                    cli,
                    Cmd::Lang {
                        language: if en { "zh-CN" } else { "en-US" }.into(),
                    },
                )?;
                return Ok(());
            }
            "7" => section(cli, en, 11)?,
            "8" => report(
                choose(en, "12").and_then(|c| c.map_or(Ok(()), |c| execute(cli, c, en))),
                en,
            )?,
            _ => {}
        }
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
