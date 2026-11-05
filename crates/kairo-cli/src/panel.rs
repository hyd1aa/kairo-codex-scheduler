//! Lightweight bilingual presentation of local scheduled-call evidence.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}
#[derive(Debug)]
pub struct EndOfInput;
impl std::fmt::Display for EndOfInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "end of input")
    }
}
impl std::error::Error for EndOfInput {}
fn page(title: &str) {
    if io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && std::env::var("TERM").as_deref() != Ok("dumb")
    {
        print!("\x1b[2J\x1b[H");
    }
    println!("{}", color(title, 34));
}
fn choices(en: bool, zh: &[&str], english: &[&str], home: bool) -> Result<usize> {
    for (n, text) in if en { english } else { zh }.iter().enumerate() {
        println!("{}. {}", n + 1, text);
    }
    println!(
        "\n0. {}",
        t(
            en,
            if home { "退出" } else { "返回" },
            if home { "Exit" } else { "Back" }
        )
    );
    loop {
        let s = input(&if en {
            format!("Select [0-{}]: ", zh.len())
        } else {
            format!("请选择 [0-{}]：", zh.len())
        })?;
        if s.len() == 1 {
            if let Ok(n) = s.parse::<usize>() {
                if n <= zh.len() {
                    return Ok(n);
                }
            }
        }
        println!(
            "{}",
            color(
                t(en, "选项无效，请重新输入", "Invalid selection; try again"),
                31
            )
        );
    }
}
fn report(cli: &Cli, cmd: Cmd, en: bool) -> Result<()> {
    page(t(en, "KAIRO / 结果", "KAIRO / Result"));
    if let Err(error) = run(cli, cmd) {
        if error.is::<EndOfInput>() {
            return Err(error);
        }
        let (_, message) = i18n::error(&error, en);
        println!("{}", color(&message, 31));
    }
    input(t(en, "按 Enter 返回当前菜单…", "Press Enter to return…"))?;
    Ok(())
}
fn human(at: i64, en: bool) -> String {
    let Ok(tz) = timezone() else {
        return "—".into();
    };
    let Some(date) = chrono::DateTime::from_timestamp(at, 0) else {
        return "—".into();
    };
    let date = date.with_timezone(&tz);
    let today = chrono::Utc::now().with_timezone(&tz).date_naive();
    let label = match date.date_naive().signed_duration_since(today).num_days() {
        0 => t(en, "今天", "Today").into(),
        1 => t(en, "明天", "Tomorrow").into(),
        -1 => t(en, "昨天", "Yesterday").into(),
        _ => date.format("%m-%d").to_string(),
    };
    format!("{} {}", label, date.format("%H:%M:%S"))
}
fn reason(code: &str, en: bool) -> &'static str {
    let (zh, english) = match code {
        "completed" => ("✓ 成功", "✓ Success"),
        "legacy_history" => ("旧版调用记录", "Legacy call record"),
        "started" | "reserved" => ("结果未知，不重复调用", "Result unknown; no retry"),
        "login_required" => ("需要登录 Codex", "Codex login required"),
        "weekly_limit" => ("官方服务端每周额度限制", "Official weekly limit"),
        "server_limit" => ("官方服务端额度或限流拒绝", "Official quota/rate limit"),
        "network_error" => ("网络请求失败", "Network request failed"),
        "timeout_or_stopped" | "timeout" => ("调用超时或已停止", "Call timed out or stopped"),
        "completion_evidence_missing" => {
            ("CLI 未返回有效完成证据", "CLI completion evidence missing")
        }
        "call_in_progress" => (
            "上一调用仍在运行，已跳过",
            "Previous call still running; skipped",
        ),
        "paused" => ("用户已暂停，未调用", "Paused by user; no call"),
        "missed_not_replayed" => ("错过计划时间，不补发", "Missed slot; no catch-up"),
        "codex_missing" => ("找不到 Codex CLI", "Codex CLI not found"),
        "cli_unsupported" => (
            "Codex CLI 不支持所需只读参数",
            "Codex CLI lacks required read-only flags",
        ),
        "offline_mock_required" => ("离线测试仅允许模拟 CLI", "Offline tests require a mock CLI"),
        "permission_denied" => ("权限不足", "Permission denied"),
        "observation_only" => ("只读检查，未调用", "Read-only check; no call"),
        _ => ("CLI 调用失败", "CLI call failed"),
    };
    if en {
        english
    } else {
        zh
    }
}
fn call_line(call: &Value, en: bool) {
    let result = call["result"].as_str().unwrap_or("unknown");
    let label = if result == "legacy_success" {
        t(en, "旧版请求成功", "Legacy request succeeded")
    } else {
        reason(call["reason"].as_str().unwrap_or(""), en)
    };
    let stamp = call["started_at"]
        .as_i64()
        .or(call["due"].as_i64())
        .map(|at| human(at, en))
        .unwrap_or("—".into());
    println!(
        "{} · {}{}",
        stamp,
        color(
            label,
            if result == "success" {
                32
            } else if result == "failed" {
                31
            } else {
                33
            }
        ),
        call["exit_code"]
            .as_i64()
            .map(|n| format!(" (exit={n})"))
            .unwrap_or_default()
    );
}
fn quota_cache(cache: &Value, en: bool) {
    for (key, zh, english) in [
        ("five_hour", "五小时额度", "Five-hour quota"),
        ("weekly", "每周额度", "Weekly quota"),
    ] {
        let remaining = cache[key]["used"]
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
            .map(|used| format!("{:.1}%", 100. - used))
            .unwrap_or_else(|| "—".into());
        println!(
            "{}: {}{}",
            t(en, zh, english),
            t(en, "剩余 ", "Remaining "),
            remaining
        );
    }
    let stamp = cache["at"]
        .as_i64()
        .map(|at| {
            format!(
                "{}{}",
                human(at, en),
                t(
                    en,
                    if chrono::Utc::now().timestamp() - at > 300 {
                        "（旧缓存，非实时）"
                    } else {
                        "（缓存，非实时）"
                    },
                    if chrono::Utc::now().timestamp() - at > 300 {
                        " (stale cache, not live)"
                    } else {
                        " (cached, not live)"
                    }
                )
            )
        })
        .unwrap_or_else(|| t(en, "尚未查询", "Not queried").into());
    println!(
        "{}",
        color(
            &format!("{}: {}", t(en, "额度更新", "Quota updated"), stamp),
            90
        )
    );
    println!(
        "{}",
        color(
            t(
                en,
                "额度仅供参考，不保证请求成功或开启新窗口。",
                "Quota is informational; requests and new windows are not guaranteed."
            ),
            90
        )
    );
}
fn home(cli: &Cli, en: bool) -> Result<()> {
    page("[ KAIRO ]");
    let store = configured(cli)?;
    let view = status(&store)?;
    println!(
        "{}",
        color(
            &format!(
                "v{} | {}",
                env!("CARGO_PKG_VERSION"),
                view["timezone"].as_str().unwrap_or("?")
            ),
            90
        )
    );
    println!();
    let (label, tone) = if view["paused"] == true {
        (t(en, "● 已暂停", "● Paused"), 33)
    } else if view["oauth_present"] != true {
        (t(en, "需要登录 Codex", "Codex login required"), 33)
    } else {
        (t(en, "● 定时调用已开启", "● Scheduled calls enabled"), 32)
    };
    println!("{}: {}", t(en, "状态", "Status"), color(label, tone));
    print!("{}: ", t(en, "上次调用", "Last call"));
    if view["last_call"].is_object() {
        call_line(&view["last_call"], en);
    } else {
        println!("—");
    }
    let next = view["next_call"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| human(d.timestamp(), en))
        .unwrap_or_else(|| "—".into());
    println!("{}: {}", t(en, "下次调用", "Next call"), next);
    println!(
        "{}",
        color(
            &format!(
                "{}: {}s",
                t(en, "计划时间后缓冲", "Delay after scheduled time"),
                view["config"]["buffer_seconds"]
            ),
            90
        )
    );
    println!();
    quota_cache(&view["quota_cache"], en);
    println!("\n{}\n", "─".repeat(24));
    Ok(())
}
pub fn render(value: &Value, en: bool) {
    if let Some(calls) = value["calls"].as_array() {
        for call in calls.iter().rev().take(20) {
            call_line(call, en);
        }
        if calls.is_empty() {
            println!("{}", t(en, "暂无调用记录", "No call history"));
        }
    } else if value["five_hour"].is_object() {
        quota_cache(value, en);
    } else if let Some(help) = value["help"].as_str() {
        println!("{help}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}
fn schedule_menu(cli: &Cli, en: bool) -> Result<()> {
    loop {
        page(t(en, "KAIRO / 调用时间", "KAIRO / Call times"));
        let c = configured(cli)?.config()?;
        println!(
            "{}: {}\n{}: {}s\n",
            t(en, "当前", "Current"),
            c.anchors.join("、"),
            t(en, "实际调用延后", "Calls start after"),
            c.buffer_seconds
        );
        let cmd = match choices(
            en,
            &[
                "添加时间",
                "修改时间",
                "删除时间",
                "恢复默认",
                "下次实际调用",
                "未来24小时预览",
            ],
            &[
                "Add time",
                "Edit time",
                "Remove time",
                "Restore defaults",
                "Next actual call",
                "Preview next 24 hours",
            ],
            false,
        )? {
            0 => return Ok(()),
            1 => Schedule::Add {
                time: input("HH:MM > ")?,
            },
            2 => Schedule::Edit {
                old: input(t(en, "原时间 > ", "Old time > "))?,
                new: input(t(en, "新时间 > ", "New time > "))?,
            },
            3 => Schedule::Remove {
                time: input("HH:MM > ")?,
            },
            4 => Schedule::Reset,
            5 => Schedule::Next,
            6 => Schedule::Preview,
            _ => unreachable!(),
        };
        report(cli, Cmd::Schedule { action: cmd }, en)?;
    }
}
fn records(cli: &Cli, en: bool) -> Result<()> {
    loop {
        page(t(en, "KAIRO / 调用记录与额度", "KAIRO / Calls and quota"));
        {
            let store = configured(cli)?;
            render(&acceptance(&store)?, en);
            quota_cache(&status(&store)?["quota_cache"], en);
        }
        match choices(
            en,
            &["查询额度（可选，只读）", "查看详细调用记录（JSON）"],
            &[
                "Query quota (optional, read only)",
                "Detailed call records (JSON)",
            ],
            false,
        )? {
            0 => return Ok(()),
            1 => report(cli, Cmd::Quota { fixture: None }, en)?,
            2 => {
                page("KAIRO / JSON");
                let store = configured(cli)?;
                println!("{}", serde_json::to_string_pretty(&acceptance(&store)?)?);
                input(t(en, "按 Enter 返回…", "Press Enter to return…"))?;
            }
            _ => unreachable!(),
        }
    }
}
fn account_menu(cli: &Cli, en: bool) -> Result<()> {
    loop {
        page(t(
            en,
            "KAIRO / Codex 登录与账号",
            "KAIRO / Codex login and account",
        ));
        let action = match choices(
            en,
            &[
                "登录 Codex（官方 OAuth）",
                "查看登录状态",
                "查看账号配置",
                "新增账号配置",
                "切换账号配置",
                "删除账号配置",
            ],
            &[
                "Log in to Codex (official OAuth)",
                "Login status",
                "List profiles",
                "Add profile",
                "Select profile",
                "Remove profile",
            ],
            false,
        )? {
            0 => return Ok(()),
            1 => "login",
            2 => "verify",
            3 => "list",
            4 => "add",
            5 => "select",
            6 => "remove",
            _ => unreachable!(),
        };
        let name = if ["add", "select", "remove"].contains(&action) {
            Some(input(t(en, "账号名称 > ", "Profile name > "))?)
        } else {
            None
        };
        let home = if action == "add" {
            Some(PathBuf::from(input(t(
                en,
                "现有认证目录 > ",
                "Existing auth directory > ",
            ))?))
        } else {
            None
        };
        report(
            cli,
            Cmd::Auth {
                action: action.into(),
                name,
                home,
                acknowledge_fresh: false,
            },
            en,
        )?;
    }
}
fn update_menu(cli: &Cli, en: bool) -> Result<()> {
    loop {
        page(t(en, "KAIRO / 更新", "KAIRO / Updates"));
        let action = match choices(
            en,
            &[
                "查询最新稳定版",
                "下载并自动升级",
                "检查本地发布清单",
                "校验并升级本地程序",
                "回滚程序",
                "恢复中断的升级事务",
            ],
            &[
                "Latest stable release",
                "Download and upgrade",
                "Check local manifest",
                "Verify and apply local update",
                "Roll back binary",
                "Recover interrupted update",
            ],
            false,
        )? {
            0 => return Ok(()),
            1 => Update::Latest,
            2 => Update::Online,
            3 => Update::Check {
                manifest: input("Manifest > ")?.into(),
            },
            4 => Update::Apply {
                binary: input("Binary > ")?.into(),
                sha256: input("SHA256 > ")?,
                prefix: input("Prefix > ")?.into(),
            },
            5 => Update::Rollback {
                prefix: input("Prefix > ")?.into(),
            },
            6 => Update::Recover {
                prefix: input("Prefix > ")?.into(),
            },
            _ => unreachable!(),
        };
        report(cli, Cmd::Update { action }, en)?;
    }
}
fn advanced(cli: &Cli, en: bool) -> Result<()> {
    loop {
        page(t(
            en,
            "KAIRO / 更新与高级设置",
            "KAIRO / Updates and settings",
        ));
        match choices(
            en,
            &[
                "检查更新",
                "运行日志",
                "系统诊断",
                "调用缓冲秒数",
                "服务管理",
                "详细状态（JSON）",
                "帮助",
            ],
            &[
                "Check updates",
                "Logs",
                "Diagnostics",
                "Call delay in seconds",
                "Service management",
                "Detailed status (JSON)",
                "Help",
            ],
            false,
        )? {
            0 => return Ok(()),
            1 => update_menu(cli, en)?,
            2 => report(cli, Cmd::Logs { lines: 20 }, en)?,
            3 => report(cli, Cmd::Doctor { repair: false }, en)?,
            4 => {
                let n = input("0..300 > ")?.parse::<i64>();
                match n {
                    Ok(buffer) => report(
                        cli,
                        Cmd::Prewarm {
                            idle: None,
                            buffer: Some(buffer),
                            reset: false,
                        },
                        en,
                    )?,
                    Err(_) => {
                        println!("{}", t(en, "秒数无效", "Invalid seconds"));
                        input(t(en, "按 Enter 返回…", "Press Enter to return…"))?;
                    }
                }
            }
            5 => {
                page(t(en, "KAIRO / 服务管理", "KAIRO / Services"));
                let n = choices(
                    en,
                    &["查看状态", "启动服务", "停止服务", "重启服务"],
                    &["Status", "Start service", "Stop service", "Restart service"],
                    false,
                )?;
                if n > 0 {
                    report(
                        cli,
                        Cmd::Service {
                            action: ["status", "start", "stop", "restart"][n - 1].into(),
                        },
                        en,
                    )?;
                }
            }
            6 => {
                page("KAIRO / JSON");
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status(&configured(cli)?)?)?
                );
                input(t(en, "按 Enter 返回…", "Press Enter to return…"))?;
            }
            7 => report(cli, Cmd::HelpInfo, en)?,
            _ => unreachable!(),
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
            let language = configured(cli)?.config()?.language;
            let en = language == "en-US";
            cli.lang = Some(language);
            home(cli, en)?;
            match choices(
                en,
                &[
                    "查看调用记录与额度",
                    "设置调用时间",
                    "启动 / 暂停",
                    "Codex 登录与账号",
                    "更新 / 高级设置",
                    "中文 / English",
                    "卸载 Kairo",
                ],
                &[
                    "Call history and quota",
                    "Call schedule",
                    "Start / pause",
                    "Codex login and account",
                    "Updates / advanced settings",
                    "中文 / English",
                    "Uninstall Kairo",
                ],
                true,
            )? {
                0 => return Ok(()),
                1 => records(cli, en)?,
                2 => schedule_menu(cli, en)?,
                3 => loop {
                    page(t(en, "KAIRO / 启动与暂停", "KAIRO / Start and pause"));
                    let n = choices(en, &["启动", "暂停"], &["Start", "Pause"], false)?;
                    if n == 0 {
                        break;
                    }
                    report(
                        cli,
                        if n == 1 {
                            Cmd::Enable {
                                approve_production_cutover: false,
                            }
                        } else {
                            Cmd::Pause
                        },
                        en,
                    )?;
                },
                4 => account_menu(cli, en)?,
                5 => advanced(cli, en)?,
                6 => run(
                    cli,
                    Cmd::Lang {
                        language: if en { "zh-CN" } else { "en-US" }.into(),
                    },
                )?,
                7 => {
                    page(t(en, "KAIRO / 卸载", "KAIRO / Uninstall"));
                    println!("{}",t(en,"将移除 Kairo 程序和服务。\n默认保留 OAuth、用户配置及状态数据。\n不会删除已有的 Codex CLI。","Removes Kairo programs and services.\nPreserves OAuth, user configuration and state data.\nKeeps your existing Codex CLI."));
                    if input(t(
                        en,
                        "输入 uninstall 确认；Enter 取消 > ",
                        "Type uninstall to confirm; Enter cancels > ",
                    ))? == "uninstall"
                    {
                        report(cli, Cmd::SystemUninstall { yes: true }, en)?;
                    }
                }
                _ => unreachable!(),
            }
        }
    })();
    ACTIVE.store(false, Ordering::Relaxed);
    match result {
        Err(e) if e.is::<EndOfInput>() => Ok(()),
        result => result,
    }
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
