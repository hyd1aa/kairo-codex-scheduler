mod i18n;
mod panel;
use anyhow::{bail, Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use i18n::{color, t};
use kairo_core::{plan, Config, Cursor, State};
use kairo_platform::{
    confirmation, management as mg, runtime as rt, timezone, update as up, Store,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "kairo",
    version,
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true,
    about = "Codex five-hour window scheduler / Codex 五小时窗口智能调度管理器"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Interface language: zh-CN or en-US / 界面语言：zh-CN 或 en-US"
    )]
    lang: Option<String>,
    #[arg(
        long,
        global = true,
        help = "Machine-readable JSON without ANSI / 无 ANSI 的机器可解析 JSON"
    )]
    json: bool,
    #[arg(
        long,
        global = true,
        env = "KAIRO_DATA_DIR",
        default_value_os_t = default_data(),
        help = "Private Kairo data directory / Kairo 私有数据目录"
    )]
    data_dir: PathBuf,
    #[arg(short='h',long,global=true,action=clap::ArgAction::Help,help="Show help / 显示帮助")]
    help: Option<bool>,
    #[arg(short='V',long,action=clap::ArgAction::Version,help="Show version / 显示版本")]
    version: Option<bool>,
    #[command(subcommand)]
    command: Option<Cmd>,
}
#[derive(Subcommand)]
enum Cmd {
    #[command(
        about = "Initialize configuration without starting services / 初始化配置，不启动服务"
    )]
    Init,
    #[command(about = "Show runtime status / 查看运行状态")]
    Status,
    #[command(
        about = "Show natural-dispatch and delayed window confirmation / 查看自然调度和延迟窗口确认"
    )]
    Acceptance,
    #[command(
        name = "runtime",
        about = "Read or configure executable and OAuth paths / 查看或设置程序及认证路径"
    )]
    RuntimeSettings {
        #[arg(long, help = "Unix user home / 系统用户主目录")]
        home: Option<PathBuf>,
        #[arg(
            long,
            help = "Existing Codex authentication directory / 现有 Codex 认证目录"
        )]
        codex_home: Option<PathBuf>,
        #[arg(
            long,
            help = "Absolute Codex executable path / Codex 可执行文件绝对路径"
        )]
        codex: Option<PathBuf>,
    },
    #[command(
        about = "Read five-hour and weekly quota without model requests / 查询五小时及每周额度，不调用模型"
    )]
    Quota {
        #[arg(long, help = "Synthetic quota fixture / 合成额度测试文件")]
        fixture: Option<PathBuf>,
    },
    #[command(about = "Manage daily activation times / 管理每日激活时间")]
    Schedule {
        #[command(subcommand)]
        action: Schedule,
    },
    #[command(about = "Persist interface language / 保存界面语言")]
    Lang {
        #[arg(help = "zh-CN or en-US / zh-CN 或 en-US")]
        language: String,
    },
    #[command(about = "Configure idle prewarm and safety buffer / 设置空闲预热及安全缓冲")]
    Prewarm {
        #[arg(
            long,
            help = "Enable or disable idle-window prewarm / 开启或关闭空闲窗口预热"
        )]
        idle: Option<bool>,
        #[arg(
            long,
            help = "Reset buffer in seconds: 0..300 / 重置后缓冲秒数：0..300"
        )]
        buffer: Option<i64>,
        #[arg(long, help = "Restore defaults / 恢复默认参数")]
        reset: bool,
    },
    #[command(
        about = "Diagnose installation and safely repair configuration / 诊断安装并安全修复配置"
    )]
    Doctor {
        #[arg(
            long,
            help = "Repair configuration defaults; never reset attempt history / 修复默认配置，不重置调用历史"
        )]
        repair: bool,
    },
    #[command(about = "Control only kairo.service / 仅控制 kairo.service")]
    Service {
        #[arg(
            help = "status, start, stop, restart, enable, disable / status、start、stop、restart、enable、disable"
        )]
        action: String,
    },
    #[command(about = "Read Kairo logs / 查看 Kairo 日志")]
    Logs {
        #[arg(
            long,
            default_value = "100",
            help = "Maximum log records: 1..10000 / 最大日志数量：1..10000"
        )]
        lines: u32,
    },
    #[command(
        about = "OAuth login, verification and account profiles / OAuth 登录、校验和账号配置"
    )]
    Auth {
        #[arg(
            default_value = "status",
            help = "status, verify, login, list, add, select, remove, bind / status、verify、login、list、add、select、remove、bind"
        )]
        action: String,
        #[arg(help = "Account profile name / 账号配置名称")]
        name: Option<String>,
        #[arg(
            long,
            help = "Existing Codex authentication directory; no token copying / 现有 Codex 认证目录，不复制令牌"
        )]
        home: Option<PathBuf>,
        #[arg(
            long,
            help = "Acknowledge a fresh account with no recent requests / 确认首次绑定且没有近期调用"
        )]
        acknowledge_fresh: bool,
    },
    #[command(about = "Check, install, recover or roll back updates / 检查、安装、恢复或回滚更新")]
    Update {
        #[command(subcommand)]
        action: Update,
    },
    #[command(
        about = "Install a verified binary without starting services / 安装校验后的二进制，不启动服务"
    )]
    Install {
        #[arg(long, help = "Source binary / 源二进制程序")]
        binary: Option<PathBuf>,
        #[arg(long, help = "Independently trusted SHA256 / 独立可信的 SHA256 校验值")]
        sha256: String,
        #[arg(long, help = "Installation prefix / 安装目录")]
        prefix: PathBuf,
    },
    #[command(about = "Remove binaries and preserve user state / 卸载程序并保留用户状态")]
    Uninstall {
        #[arg(long, help = "Installation prefix / 安装目录")]
        prefix: PathBuf,
    },
    #[command(about = "Show project help / 显示项目说明")]
    HelpInfo,
    #[command(
        about = "Remove the managed system installation, preserving state / 卸载系统安装并保留状态"
    )]
    SystemUninstall {
        #[arg(long, required = true)]
        yes: bool,
    },
    #[command(
        about = "Run the scheduler; observation until explicitly activated / 运行调度器，明确批准前仅观察"
    )]
    Daemon {
        #[arg(
            long,
            help = "Structurally read-only observation / 结构性只读观察，禁止模型执行"
        )]
        observe: bool,
        #[arg(long, help = "Perform one iteration / 只执行一轮检查")]
        once: bool,
        #[arg(
            long,
            help = "Offline synthetic timeline; never invokes Codex / 离线合成时间线，绝不调用 Codex"
        )]
        fixture: Option<PathBuf>,
        #[arg(
            long,
            default_value = "0",
            help = "Offline tick delay in milliseconds / 离线测试每步间隔毫秒数"
        )]
        poll_ms: u64,
        #[arg(
            long,
            help = "Keep offline daemon alive until signal / 离线时间线结束后等待退出信号"
        )]
        hold: bool,
    },
    #[command(about = "Preview a synthetic scheduling decision / 预览合成调度决策")]
    Simulate {
        #[arg(help = "Synthetic fixture / 合成测试文件")]
        fixture: PathBuf,
    },
    #[command(about = "Validate state without requests / 无请求校验持久化状态")]
    Validate,
    #[command(
        about = "Atomically migrate compatible state or import a normalized transfer / 原子迁移兼容状态或导入标准化迁移包"
    )]
    Migrate {
        #[arg(
            long,
            help = "Normalized transfer bundle; never raw OAuth / 标准化状态迁移包，不接受 OAuth"
        )]
        import: Option<PathBuf>,
        #[arg(
            long,
            conflicts_with = "import",
            help = "Python 1.3 JSON state file / Python 1.3 JSON 状态文件"
        )]
        legacy: Option<PathBuf>,
        #[arg(
            long,
            help = "Explicit original account fingerprint / 明确的原账号指纹"
        )]
        legacy_account: Option<String>,
        #[arg(
            long,
            help = "Original scheduler timezone, e.g. Asia/Shanghai / 原调度器时区，例如 Asia/Shanghai"
        )]
        legacy_timezone: Option<String>,
        #[arg(
            long,
            help = "Convert and validate without writing state or networking / 只转换校验，不写状态、不联网"
        )]
        dry_run: bool,

        #[arg(
            long,
            help = "Expected verified account fingerprint / 预期的已验证账号指纹"
        )]
        account: Option<String>,
    },
    #[command(
        about = "Enable live scheduling after separate production approval / 单独生产批准后启用真实调度"
    )]
    Activate {
        #[arg(long, help = "Verified account fingerprint / 已验证账号指纹")]
        account: String,
        #[arg(
            long,
            help = "Explicit acknowledgment of approved production cutover / 明确确认已批准生产切换"
        )]
        approve_production_cutover: bool,
    },
    #[command(about = "Pause Kairo model requests / 暂停 Kairo 模型请求")]
    Pause,
}
#[derive(Subcommand)]
enum Schedule {
    #[command(about = "List activation times / 查看激活时间")]
    List,
    #[command(about = "Add HH:MM / 新增 HH:MM 时间")]
    Add { time: String },
    #[command(about = "Edit an existing time / 修改已有时间")]
    Edit { old: String, new: String },
    #[command(about = "Remove a time / 删除时间")]
    Remove { time: String },
    #[command(about = "Show the next actual check / 查看下次实际检查时间")]
    Next,
    #[command(about = "Preview the next 24 hours / 预览未来24小时")]
    Preview,
    #[command(about = "Restore four default times / 恢复四个默认时间")]
    Reset,
}
#[derive(Subcommand)]
enum Update {
    #[command(about = "Check latest stable release without updating / 查询最新稳定版，不执行升级")]
    Latest,
    #[command(
        about = "Update from GitHub HTTPS with SHA256 verification / 从 GitHub HTTPS 校验 SHA256 并更新"
    )]
    Online,
    #[command(about = "Read a local release manifest / 读取本地发布清单")]
    Check { manifest: PathBuf },
    #[command(about = "Atomically apply a verified local binary / 原子安装已校验的本地程序")]
    Apply {
        binary: PathBuf,
        #[arg(long)]
        sha256: String,
        #[arg(long)]
        prefix: PathBuf,
    },
    #[command(
        about = "Fetch and apply a checksum-authenticated release manifest / 按独立可信清单校验值下载并升级"
    )]
    Fetch {
        manifest: PathBuf,
        #[arg(long)]
        manifest_sha256: String,
        #[arg(long)]
        prefix: PathBuf,
    },
    #[command(
        about = "Roll back the binary while retaining current attempt history / 回退二进制并保留当前调用历史"
    )]
    Rollback {
        #[arg(long)]
        prefix: PathBuf,
    },
    #[command(about = "Recover an interrupted update transaction / 恢复中断的升级事务")]
    Recover {
        #[arg(long)]
        prefix: PathBuf,
    },
}
fn emit(cli: &Cli, v: Value) -> Result<()> {
    if panel::details() {
        return panel::detail_output(cli, v);
    } else if panel::active() {
        panel::render(&v, cli.lang.as_deref() == Some("en-US"));
    } else if cli.json {
        println!("{}", serde_json::to_string_pretty(&v)?)
    } else {
        i18n::render(&v, cli.lang.as_deref() == Some("en-US"), 0)
    }
    Ok(())
}
fn configured(cli: &Cli) -> Result<Store> {
    if cli.data_dir.join(".upgrade-lock").exists() {
        bail!("update_in_progress / 程序升级中，请稍后重试或运行 update recover")
    };
    let store = Store::open(&cli.data_dir)?;
    if cli.data_dir.join(".upgrade-lock").exists() {
        bail!("update_in_progress / 程序升级中，请稍后重试或运行 update recover")
    };
    Ok(store)
}
fn next_checks(store: &Store, c: &Config, hours: i64) -> Result<Value> {
    let tz = timezone()?;
    let now = chrono::Utc::now();
    let mut times = plan(c, tz, now, hours)?;
    if let Ok(cur) = store.read::<Cursor>("cursor.json") {
        if cur.timezone == tz.to_string() && cur.config_signature == kairo_core::config_signature(c)
        {
            times.retain(|due| {
                let key = (*due - chrono::Duration::seconds(c.buffer_seconds))
                    .with_timezone(&tz)
                    .format("%Y-%m-%d/%H:%M")
                    .to_string();
                !cur.finished.contains(&key)
            });
            if let Some(due) = cur.due {
                if due > now.timestamp()
                    && due < cur.cutoff
                    && due <= now.timestamp() + hours * 3600
                {
                    if let Some(d) = chrono::DateTime::from_timestamp(due, 0) {
                        times.push(d)
                    }
                }
            }
        }
    }
    times.sort();
    times.dedup();
    Ok(
        json!({"timezone":tz.to_string(),"checks":times.iter().map(|d|d.with_timezone(&tz).to_rfc3339()).collect::<Vec<_>>()}),
    )
}
fn local_time(at: i64, tz: chrono_tz::Tz) -> Option<String> {
    chrono::DateTime::from_timestamp(at, 0).map(|d| d.with_timezone(&tz).to_rfc3339())
}
fn status_quota(store: &Store, tz: chrono_tz::Tz) -> Value {
    match rt::verify(store) {
        Ok(mut v) => {
            for key in ["five_hour", "weekly"] {
                if let Some(at) = v[key]["reset_at"].as_i64() {
                    v[key]["reset_at_local"] = json!(local_time(at, tz));
                }
            }
            v
        }
        Err(_) => json!({"error":"quota_or_identity_unavailable"}),
    }
}
fn possible_prewarm(
    next: &Value,
    quota: &Value,
    config: &Config,
    cutoff: i64,
    last_attempt: Option<i64>,
    tz: chrono_tz::Tz,
) -> Option<String> {
    if quota["server_denied"] != false || quota["weekly"]["used"].as_f64()? >= 100.0 {
        return None;
    }
    let mut at = chrono::DateTime::parse_from_rfc3339(next.as_str()?)
        .ok()?
        .timestamp();
    let used = quota["five_hour"]["used"].as_f64()?;
    if used > 0.0 {
        at = at.max(
            quota["five_hour"]["reset_at"]
                .as_i64()?
                .checked_add(config.buffer_seconds)?,
        );
    }
    if let Some(last) = last_attempt {
        at = at.max(last.checked_add(kairo_core::WINDOW + config.buffer_seconds)?);
    }
    if cutoff > 0 && at >= cutoff {
        let start = chrono::DateTime::from_timestamp(at.checked_sub(1)?, 0)?;
        at = plan(config, tz, start, 72)
            .ok()?
            .into_iter()
            .find(|d| d.timestamp() >= at)?
            .timestamp();
    }
    local_time(at, tz)
}
fn run(cli: &Cli, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Daemon {
            observe,
            once,
            fixture,
            poll_ms,
            hold,
        } => {
            return if let Some(f) = fixture {
                kairo_platform::replay::run(&cli.data_dir, &f, poll_ms, hold)
            } else {
                rt::daemon(&cli.data_dir, once, observe)
            }
        }
        Cmd::Install {
            binary,
            sha256,
            prefix,
        } => {
            let prefix = up::safe_prefix(&prefix)?;
            {
                let s = configured(cli)?;
                if !s.root.join("config.json").exists() {
                    let c = Config {
                        language: cli.lang.clone().unwrap_or("zh-CN".into()),
                        ..Default::default()
                    };
                    c.validate()?;
                    s.write("config.json", &c)?;
                }
                // Existing state must be snapshotted before the candidate migrates it.
            }
            up::install(
                &binary.unwrap_or(std::env::current_exe()?),
                &sha256,
                &prefix,
                &cli.data_dir,
            )?;
            return emit(
                cli,
                json!({"installed":true,"prefix":prefix,"execution":"observation_only"}),
            );
        }
        Cmd::SystemUninstall { yes } => {
            if !yes {
                bail!("confirmation_required / 需要确认卸载");
            }
            let status = std::process::Command::new("sh")
                .arg("/opt/kairo/installer/system-uninstall.sh")
                .env("KAIRO_LANG", cli.lang.as_deref().unwrap_or("zh-CN"))
                .status()
                .context("managed_installer_missing / 需要通过在线安装器安装")?;
            if !status.success() {
                bail!("system_uninstall_failed / 系统卸载失败，请检查权限及日志");
            }
            return Ok(());
        }
        Cmd::Uninstall { prefix } => {
            up::uninstall(&prefix, &cli.data_dir)?;
            return emit(cli, json!({"removed":true,"data_dir":cli.data_dir}));
        }
        Cmd::Update { action } => {
            match action {
                Update::Latest => return emit(cli, panel::latest()?),
                Update::Online => {
                    let status = std::process::Command::new("bash")
                        .args(["/opt/kairo/installer/install.sh", "--update"])
                        .env("KAIRO_LANG", cli.lang.as_deref().unwrap_or("zh-CN"))
                        .status()
                        .context("managed_installer_missing / 需要通过在线安装器安装")?;
                    if !status.success() {
                        bail!("online_update_failed / 在线更新失败，未跳过校验，请检查权限及日志");
                    }
                }
                Update::Check { manifest } => {
                    return emit(cli, up::check_manifest(&up::manifest_file(&manifest)?)?)
                }
                Update::Apply {
                    binary,
                    sha256,
                    prefix,
                } => up::install(&binary, &sha256, &prefix, &cli.data_dir)?,
                Update::Fetch {
                    manifest,
                    manifest_sha256,
                    prefix,
                } => up::fetch(&manifest, &manifest_sha256, &prefix, &cli.data_dir)?,
                Update::Rollback { prefix } => up::rollback(&prefix, &cli.data_dir)?,
                Update::Recover { prefix } => up::recover(&prefix, &cli.data_dir)?,
            };
            return emit(cli, json!({"success":true}));
        }
        Cmd::Doctor { repair: true } => {
            let store = configured(cli)?;
            mg::repair_defaults(&store)?;
            return emit(cli, mg::doctor(&store, true)?);
        }
        Cmd::Migrate {
            legacy: Some(ref path),
            ref account,
            ref legacy_account,
            ref legacy_timezone,
            dry_run: true,
            ..
        } => {
            let value: Value = serde_json::from_slice(&fs::read(path)?)?;
            let transfer = kairo_platform::legacy::convert(
                &value,
                legacy_account
                    .as_deref()
                    .context("legacy_account_required / 需要原账号指纹")?,
                account
                    .as_deref()
                    .context("account_required / 需要账号指纹")?,
                legacy_timezone
                    .as_deref()
                    .context("legacy_timezone_required / 需要原调度器时区")?
                    .parse()?,
            )?;
            return emit(cli, serde_json::to_value(transfer)?);
        }
        Cmd::HelpInfo => {
            return emit(
                cli,
                json!({"help":t(cli.lang.as_deref()==Some("en-US"),"Kairo 管理每日激活时间及五小时额度窗口，不能增加额度。默认仅观察；每周额度、服务端拒绝、近期请求和调用前持久化保护不能关闭。启用调度前请完成授权、额度和请求历史安全检查。使用 --help 查看全部命令。","Kairo manages daily activation times and five-hour allowance windows; it cannot increase quota. Observation is the default. Weekly limits, server denial, recent-request guards and durable reservations cannot be disabled. Complete authorization, quota and request-history checks before enabling scheduling. Use --help for commands.")}),
            )
        }
        Cmd::Migrate {
            ref import,
            ref legacy,
            dry_run,
            ..
        } if import.is_none() && legacy.is_none() => {
            let s = Store::open(&cli.data_dir)?;
            if dry_run {
                mg::validate(&s)?;
            } else {
                mg::migrate(&s)?;
            }
            return emit(cli, json!({"migrated":!dry_run,"validated":true}));
        }
        Cmd::Validate => {
            let s = Store::open(&cli.data_dir)?;
            mg::validate(&s)?;
            return emit(cli, json!({"validated":true}));
        }
        _ => {}
    }
    let store = configured(cli)?;
    if matches!(cmd, Cmd::Init) {
        if store.root.join("config.json").exists() {
            bail!("already_initialized / 已初始化")
        };
        let c = Config {
            language: cli.lang.clone().unwrap_or("zh-CN".into()),
            ..Default::default()
        };
        c.validate()?;
        store.write("config.json", &c)?;
        store.write("runtime.json", &rt::Runtime::default())?;
        return emit(
            cli,
            json!({"initialized":true,"execution":"observation_only"}),
        );
    }
    let mut c = store.config()?;
    match cmd {
        Cmd::Init
        | Cmd::SystemUninstall { .. }
        | Cmd::Install { .. }
        | Cmd::Uninstall { .. }
        | Cmd::Update { .. }
        | Cmd::HelpInfo
        | Cmd::Daemon { .. }
        | Cmd::Validate => unreachable!(),
        Cmd::Status => {
            let tz = timezone()?;
            let mut checks = next_checks(&store, &c, 48)?;
            let next = checks["checks"]
                .as_array_mut()
                .and_then(|a| a.first().cloned());
            let state = store.state();
            let acceptance = match state.as_ref() {
                Ok(s) => Some(confirmation::summary(&store, s, tz)?),
                Err(_) => None,
            };
            let mut deployment = store.read::<Value>("deployment.json").ok();
            if let Some(ref mut d) = deployment {
                if let Some(at) = d["approved_at"].as_i64() {
                    d["approved_at_local"] = json!(local_time(at, tz));
                }
            }
            let current_quota = if deployment.as_ref().is_some_and(|v| v["enabled"] == true) {
                Some(status_quota(&store, tz))
            } else {
                None
            };
            let next_possible = current_quota.as_ref().and_then(|quota| {
                possible_prewarm(
                    next.as_ref()?,
                    quota,
                    &c,
                    store
                        .read::<Cursor>("cursor.json")
                        .map_or(0, |cur| cur.cutoff),
                    state
                        .as_ref()
                        .ok()
                        .and_then(|s| s.attempts.last().map(|a| a.at)),
                    tz,
                )
            });
            let mut status_file = store.read::<Value>("status.json").ok();
            if let Some(ref mut v) = status_file {
                if let Some(at) = v["at"].as_i64() {
                    v["at_local"] = json!(local_time(at, tz));
                }
                if let Some(at) = v["next_check"].as_i64() {
                    v["next_check_local"] = json!(local_time(at, tz));
                }
            }
            let recent_reason = status_file
                .as_ref()
                .and_then(|v| v["error"].as_str().or_else(|| v["event"].as_str()));
            emit(
                cli,
                json!({"version":env!("CARGO_PKG_VERSION"),"timezone":tz.to_string(),"config":c,"next_check":next,"next_possible_prewarm":next_possible,"prewarm_conditional":true,"state_valid":state.is_ok(),"status_file":status_file,"recent_reason":recent_reason,"deployment":deployment,"acceptance":acceptance,"current_quota":current_quota}),
            )
        }
        Cmd::Acceptance => {
            let state = store.state()?;
            confirmation::sync(&store, &state)?;
            let tz = timezone()?;
            let mut v = confirmation::summary(&store, &state, tz)?;
            let mut natural = store.read::<Value>("natural-acceptance.json").ok();
            if let Some(ref mut n) = natural {
                for key in ["at", "completed_at"] {
                    if let Some(at) = n[key].as_i64() {
                        n[format!("{key}_local")] = json!(local_time(at, tz));
                    }
                }
                if let Some(at) = n["first_natural_check"]["at"].as_i64() {
                    n["first_natural_check"]["at_local"] = json!(local_time(at, tz));
                }
                if let Some(at) = n["first_natural_check"]["next_check"].as_i64() {
                    n["first_natural_check"]["next_check_local"] = json!(local_time(at, tz));
                }
            }
            v["natural_dispatch"] = json!(natural);
            v["confirmation_details"] = confirmation::details(&store, &state, tz)?;
            v["timezone"] = json!(tz.to_string());
            emit(cli, v)
        }
        Cmd::Schedule { action } => {
            match action {
                Schedule::List => {
                    return emit(
                        cli,
                        json!({"timezone":timezone()?.to_string(),"anchors":c.anchors,"max_anchors":48,"minimum_spacing_minutes":10,"buffer_seconds":c.buffer_seconds}),
                    )
                }
                Schedule::Add { time } => {
                    kairo_core::time(&time)?;
                    c.anchors.push(time)
                }
                Schedule::Edit { old, new } => {
                    kairo_core::time(&new)?;
                    *c.anchors
                        .iter_mut()
                        .find(|s| **s == old)
                        .context("time_missing / 时间不存在")? = new
                }
                Schedule::Remove { time } => {
                    let i = c
                        .anchors
                        .iter()
                        .position(|s| *s == time)
                        .context("time_missing / 时间不存在")?;
                    c.anchors.remove(i);
                }
                Schedule::Reset => c.anchors = Config::default().anchors,
                Schedule::Next | Schedule::Preview => {
                    let mut v = next_checks(
                        &store,
                        &c,
                        if matches!(action, Schedule::Next) {
                            48
                        } else {
                            24
                        },
                    )?;
                    if matches!(action, Schedule::Next) {
                        v["checks"].as_array_mut().unwrap().truncate(1)
                    }
                    return emit(cli, v);
                }
            }
            c.validate()?;
            c.anchors.sort();
            store.write("config.json", &c)?;
            emit(cli, json!({"anchors":c.anchors}))
        }
        Cmd::RuntimeSettings {
            home,
            codex_home,
            codex,
        } => {
            let mut r = rt::runtime(&store)?;
            let changed = home.is_some() || codex_home.is_some() || codex.is_some();
            if let Some(v) = home {
                r.home = v
            }
            if let Some(v) = codex_home {
                r.codex_home = v
            }
            if let Some(v) = codex {
                r.codex = v
            }
            r.validate()?;
            if changed {
                if store.root.join("deployment.json").exists() {
                    rt::pause(&store)?
                }
                store.write("runtime.json", &r)?;
            }
            emit(cli, json!({"runtime":r}))
        }
        Cmd::Lang { language } => {
            c.language = language;
            c.validate()?;
            store.write("config.json", &c)?;
            emit(cli, json!({"language":c.language}))
        }
        Cmd::Prewarm {
            idle,
            buffer,
            reset,
        } => {
            if reset {
                c.idle_window_prewarm = true;
                c.buffer_seconds = 30;
            }
            if let Some(v) = idle {
                c.idle_window_prewarm = v
            }
            if let Some(v) = buffer {
                c.buffer_seconds = v
            }
            c.validate()?;
            if idle.is_some() || buffer.is_some() || reset {
                store.write("config.json", &c)?;
            }
            emit(
                cli,
                json!({"idle_window_prewarm":c.idle_window_prewarm,"buffer_seconds":c.buffer_seconds,"non_optional_guards":["weekly","server_denied","attempt_guard","durable_reservation"],"state":store.state().ok()}),
            )
        }
        Cmd::Quota { fixture } => {
            let r = rt::runtime(&store)?;
            let now = chrono::Utc::now().timestamp();
            let (f, w, d) = if let Some(path) = fixture {
                let v: Value = serde_json::from_slice(&fs::read(path)?)?;
                kairo_platform::parse_usage(&v, now)?
            } else {
                let id = kairo_platform::identity(&r.codex_home.join("auth.json"))?;
                rt::verified_usage(&id, &kairo_platform::quota(&id)?, now)?
            };
            emit(cli, json!({"five_hour":f,"weekly":w,"server_denied":d}))
        }
        Cmd::Auth {
            action,
            name,
            home,
            acknowledge_fresh,
        } => match action.as_str() {
            "login" => {
                if cli.json || !io::stdin().is_terminal() {
                    bail!("interactive_login_required / OAuth 登录需要交互终端，由用户完成人工认证")
                };
                mg::login(&store)?;
                emit(cli, json!({"success":true}))
            }
            "bind" => emit(cli, rt::bind(&store, acknowledge_fresh)?),
            _ => emit(
                cli,
                mg::account_action(&store, &action, name.as_deref(), home.as_deref())?,
            ),
        },
        Cmd::Doctor { repair } => emit(cli, mg::doctor(&store, repair)?),
        Cmd::Service { action } => {
            drop(store);
            emit(cli, mg::service(&action)?)
        }
        Cmd::Logs { lines } => {
            if !(1..=10000).contains(&lines) {
                bail!("log_limit_invalid / 日志数量必须为1到10000")
            };
            drop(store);
            let o = std::process::Command::new("journalctl")
                .args([
                    "--unit=kairo.service",
                    "--no-pager",
                    "--output=json",
                    "--lines",
                ])
                .arg(lines.to_string())
                .output()?;
            if !o.status.success() {
                bail!("logs_failed / 日志查询失败")
            };
            let values = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let tz = timezone().ok();
            let values = values
                .into_iter()
                .map(|v| {
                    let message = v["MESSAGE"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or(v["MESSAGE"].clone());
                    let timestamp = v["__REALTIME_TIMESTAMP"]
                        .as_str()
                        .and_then(|s| s.parse::<i64>().ok())
                        .and_then(chrono::DateTime::from_timestamp_micros)
                        .map(|t| {
                            if let Some(tz) = tz {
                                t.with_timezone(&tz).to_rfc3339()
                            } else {
                                t.to_rfc3339()
                            }
                        });
                    json!({"timestamp":timestamp,"message":message})
                })
                .collect::<Vec<_>>();
            emit(
                cli,
                json!({"timezone":tz.map(|t|t.to_string()).unwrap_or("timezone_unknown".into()),"logs":values}),
            )
        }
        Cmd::Simulate { fixture } => {
            let v: Value = serde_json::from_slice(&fs::read(fixture)?)?;
            let now = v["now"].as_i64().context("clock_missing / 缺少时间戳")?;
            let (f, w, d) = kairo_platform::parse_usage(&v["usage"], now)?;
            let mut s: State = serde_json::from_value(v["state"].clone())?;
            let anchor = v["anchor"].as_str().unwrap_or("synthetic");
            let decision = kairo_core::decide(
                &c,
                &s,
                anchor,
                now,
                v["cutoff"].as_i64().unwrap_or(now + 3600),
                &f,
                &w,
                d,
            );
            let reason = match &decision {
                kairo_core::Decision::Request(r) => json!({"event":r,"eligible":true}),
                kairo_core::Decision::Block(r) => json!({"event":r,"eligible":false}),
                kairo_core::Decision::Defer(at) => {
                    json!({"event":"deferred","next_check":at,"eligible":false})
                }
            };
            if matches!(decision, kairo_core::Decision::Request(_)) {
                kairo_core::reserve(&mut s, anchor, now, f.clone());
                kairo_core::complete(&mut s, true, Some(f));
            }
            emit(cli, json!({"decision":reason,"state":s,"model_requests":0}))
        }
        Cmd::Migrate {
            import,
            legacy,
            account,
            legacy_account,
            legacy_timezone,
            dry_run,
        } => {
            let _lease = rt::daemon_lease(&cli.data_dir)?;
            let account = account.context("account_required / 需要账号指纹")?;
            if let Some(path) = legacy {
                let value: Value = serde_json::from_slice(&fs::read(path)?)?;
                let transfer = kairo_platform::legacy::convert(
                    &value,
                    legacy_account
                        .as_deref()
                        .context("legacy_account_required / 需要原账号指纹")?,
                    &account,
                    legacy_timezone
                        .as_deref()
                        .context("legacy_timezone_required / 需要原调度器时区")?
                        .parse()?,
                )?;
                mg::import_transfer(&store, transfer, &account)?;
            } else if let Some(path) = import {
                if dry_run {
                    let transfer: mg::Transfer = serde_json::from_slice(&fs::read(path)?)?;
                    transfer.state.validate()?;
                    return emit(cli, serde_json::to_value(transfer)?);
                }
                mg::import(&store, &path, &account)?;
            } else {
                bail!("transfer_required / 需要状态迁移包")
            }
            emit(cli, json!({"migrated":true,"execution":"observation_only"}))
        }
        Cmd::Activate {
            account,
            approve_production_cutover,
        } => {
            if let Err(e) = rt::activate(&store, &account, approve_production_cutover) {
                if e.chain().any(|e| {
                    e.downcast_ref::<io::Error>()
                        .is_some_and(|e| e.kind() == io::ErrorKind::NotFound)
                }) && ["deployment.json", "state.json", "cursor.json"]
                    .iter()
                    .any(|name| !store.root.join(name).exists())
                {
                    return Err(e.context("activation_history_missing / 部署授权或调度历史文件缺失；OAuth 校验成功不代表已绑定或已授权。请运行 status、doctor 并人工核对历史；不要自动 FRESH、伪造状态或删除保护记录"));
                }
                return Err(e);
            }
            emit(cli, json!({"enabled":true}))
        }
        Cmd::Pause => {
            rt::pause(&store)?;
            emit(cli, json!({"paused":true}))
        }
    }
}
fn input(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut s = String::new();
    if io::stdin().read_line(&mut s)? == 0 {
        bail!(panel::EndOfInput);
    }
    Ok(s.trim().into())
}
fn submenu(en: bool, zh: &[&str], english: &[&str]) -> Result<String> {
    loop {
        let choice = numbered(en, zh, english, t(en, "返回", "Back"))?;
        let normalized = choice.trim_start_matches('0');
        if normalized.is_empty()
            || normalized
                .parse::<usize>()
                .is_ok_and(|n| n > 0 && n <= zh.len())
        {
            panel::action_page(en);
            return Ok(choice);
        }
        println!(
            "{}",
            color(
                t(
                    en,
                    "菜单选项无效；请重新输入",
                    "Invalid selection; try again"
                ),
                31
            )
        );
    }
}
fn numbered(en: bool, zh: &[&str], english: &[&str], exit: &str) -> Result<String> {
    for (i, s) in (if en { english } else { zh }).iter().enumerate() {
        println!("{:02}. {}", i + 1, s)
    }
    println!("00. {exit}");
    loop {
        let choice = input(t(en, "请输入选项 > ", "Select > "))?;
        if !choice.is_empty() {
            return Ok(choice);
        }
        println!(
            "{}",
            color(
                t(
                    en,
                    "菜单选项无效；请重新输入",
                    "Invalid selection; try again"
                ),
                31
            )
        );
    }
}
fn choose(en: bool, choice: &str) -> Result<Option<Cmd>> {
    Ok(Some(match choice {
        "00" => return Ok(None),
        "01" => Cmd::Status,
        "02" => Cmd::Quota { fixture: None },
        "03" => {
            println!(
                "{}",
                t(
                    en,
                    "最多48个时间，最小间隔10分钟（含跨午夜）",
                    "Up to 48 times; minimum spacing 10 minutes including midnight"
                )
            );
            let a = submenu(
                en,
                &[
                    "查看全部激活时间",
                    "新增时间",
                    "修改时间",
                    "删除时间",
                    "下次实际检查",
                    "未来24小时预览",
                    "恢复默认时间",
                ],
                &[
                    "List all times",
                    "Add time",
                    "Edit time",
                    "Remove time",
                    "Next actual check",
                    "Preview next 24 hours",
                    "Restore default times",
                ],
            )?;
            Cmd::Schedule {
                action: match a.trim_start_matches('0') {
                    "1" => Schedule::List,
                    "2" => Schedule::Add {
                        time: input("HH:MM > ")?,
                    },
                    "3" => Schedule::Edit {
                        old: input(t(en, "原时间 > ", "Old time > "))?,
                        new: input(t(en, "新时间 > ", "New time > "))?,
                    },
                    "4" => Schedule::Remove {
                        time: input("HH:MM > ")?,
                    },
                    "5" => Schedule::Next,
                    "6" => Schedule::Preview,
                    "7" => Schedule::Reset,
                    _ => return Ok(None),
                },
            }
        }
        "04" => {
            let a = submenu(
                en,
                &["查看状态", "启动服务", "停止服务", "重启服务"],
                &["Status", "Start service", "Stop service", "Restart service"],
            )?;
            Cmd::Service {
                action: match a.trim_start_matches('0') {
                    "1" => "status",
                    "2" => "start",
                    "3" => "stop",
                    "4" => "restart",
                    _ => return Ok(None),
                }
                .into(),
            }
        }
        "05" => Cmd::Logs { lines: 20 },
        "06" => {
            let a = submenu(
                en,
                &[
                    "查看本地账号状态",
                    "服务端账号校验",
                    "OAuth 登录（人工认证）",
                    "查看账号配置",
                    "新增账号配置",
                    "切换账号配置",
                    "删除账号配置",
                    "首次账号绑定（需确认无旧服务）",
                ],
                &[
                    "Local account status",
                    "Verify server identity",
                    "OAuth login (human authentication)",
                    "List account profiles",
                    "Add profile",
                    "Select profile",
                    "Remove profile",
                    "Fresh account binding (legacy must be stopped)",
                ],
            )?;
            let action = match a.trim_start_matches('0') {
                "1" => "status",
                "2" => "verify",
                "3" => "login",
                "4" => "list",
                "5" => "add",
                "6" => "select",
                "7" => "remove",
                "8" => "bind",
                _ => return Ok(None),
            };
            let name = if ["add", "select", "remove"].contains(&action) {
                Some(input(t(en, "账号名称 > ", "Profile name > "))?)
            } else {
                None
            };
            let home = if action == "add" {
                Some(PathBuf::from(input(t(
                    en,
                    "现有 Codex 认证目录 > ",
                    "Existing Codex authentication directory > ",
                ))?))
            } else {
                None
            };
            let acknowledge_fresh = if action == "bind" {
                input(t(
                    en,
                    "仅限已批准的首次绑定且无近期请求，输入 FRESH > ",
                    "Approved fresh binding with no recent requests only; enter FRESH > ",
                ))? == "FRESH"
            } else {
                false
            };
            Cmd::Auth {
                action: action.into(),
                name,
                home,
                acknowledge_fresh,
            }
        }
        "07" => {
            let a = submenu(
                en,
                &["系统诊断", "安全修复默认配置（保留调用历史）"],
                &[
                    "Diagnostics",
                    "Repair default configuration (retain attempt history)",
                ],
            )?;
            Cmd::Doctor {
                repair: match a.trim_start_matches('0') {
                    "1" => false,
                    "2" => true,
                    _ => return Ok(None),
                },
            }
        }
        "08" => {
            let a = submenu(
                en,
                &[
                    "查看预热参数",
                    "设置空闲窗口预热",
                    "设置重置后缓冲秒数",
                    "查看每周额度保护和服务端拒绝",
                    "查看窗口生命周期",
                    "无模型请求的未来计划预览",
                    "恢复默认预热参数",
                ],
                &[
                    "Show prewarm parameters",
                    "Set idle-window prewarm",
                    "Set reset safety buffer",
                    "Weekly protection and server denial",
                    "Window lifecycle",
                    "Preview without model requests",
                    "Restore prewarm defaults",
                ],
            )?;
            match a.trim_start_matches('0') {
                "1" | "4" | "5" => Cmd::Prewarm {
                    idle: None,
                    buffer: None,
                    reset: false,
                },
                "2" => Cmd::Prewarm {
                    idle: Some(input("true / false > ")?.parse()?),
                    buffer: None,
                    reset: false,
                },
                "3" => Cmd::Prewarm {
                    idle: None,
                    buffer: Some(input("0..300 > ")?.parse()?),
                    reset: false,
                },
                "6" => Cmd::Schedule {
                    action: Schedule::Preview,
                },
                "7" => Cmd::Prewarm {
                    idle: None,
                    buffer: None,
                    reset: true,
                },
                _ => return Ok(None),
            }
        }
        "09" => {
            let a = submenu(
                en,
                &[
                    "检查发布清单",
                    "校验并升级本地程序",
                    "下载并自动升级",
                    "回滚程序（保留调用历史）",
                    "恢复中断的升级事务",
                    "查询 GitHub 最新稳定版（不升级）",
                ],
                &[
                    "Check release manifest",
                    "Verify and apply local update",
                    "Download and automatically upgrade",
                    "Roll back binary (retain history)",
                    "Recover interrupted update",
                    "Check GitHub latest stable (no upgrade)",
                ],
            )?;
            let action = match a.trim_start_matches('0') {
                "1" => Update::Check {
                    manifest: input(t(en, "清单路径 > ", "Manifest path > "))?.into(),
                },
                "2" => Update::Apply {
                    binary: input(t(en, "二进制路径 > ", "Binary path > "))?.into(),
                    sha256: input("SHA256 > ")?,
                    prefix: input(t(en, "安装目录 > ", "Installation prefix > "))?.into(),
                },
                "3" => Update::Online,
                "4" => Update::Rollback {
                    prefix: input(t(en, "安装目录 > ", "Installation prefix > "))?.into(),
                },
                "6" => Update::Latest,
                "5" => Update::Recover {
                    prefix: input(t(en, "安装目录 > ", "Installation prefix > "))?.into(),
                },
                _ => return Ok(None),
            };
            Cmd::Update { action }
        }
        "10" => Cmd::Lang {
            language: if en { "zh-CN" } else { "en-US" }.into(),
        },
        "11" => Cmd::HelpInfo,
        "12" => {
            println!("{}", t(en, "将停止并禁用 kairo.service，删除安装器管理的 /opt/kairo 程序、unit 和全局 kairo 命令。保留 /var/lib/kairo、账号保护记录、/home/kairo/.codex、Codex CLI 及其他项目。", "Stops/disables kairo.service; removes installer-managed /opt/kairo programs, unit and global kairo command. Retains /var/lib/kairo, account guards, /home/kairo/.codex, Codex CLI and other projects."));
            if input(t(en, "确认继续？输入 yes > ", "Continue? Type yes > "))? != "yes" {
                return Ok(None);
            }
            if input(t(
                en,
                "输入 uninstall 确认卸载（保留状态） > ",
                "Type uninstall to remove (preserves state) > ",
            ))? != "uninstall"
            {
                return Ok(None);
            }
            Cmd::SystemUninstall { yes: true }
        }
        _ => bail!("menu_choice_invalid / 菜单选项无效"),
    }))
}
fn menu(cli: &mut Cli) -> Result<()> {
    panel::menu(cli)
}
fn localize(mut command: clap::Command, en: bool, json: bool) -> clap::Command {
    command = command
        .subcommand_help_heading(t(en, "子命令", "Commands"))
        .disable_help_subcommand(true)
        .color(if json {
            clap::ColorChoice::Never
        } else {
            clap::ColorChoice::Auto
        })
        .help_template(if en {
            "{name} {version}\n{about}\n\nUsage: {usage}\n\n{all-args}"
        } else {
            "{name} {version}\n{about}\n\n用法: {usage}\n\n{all-args}"
        });
    if let Some(a) = command.get_about() {
        let text = i18n::bilingual(&a.to_string(), en);
        command = command.about(text)
    };
    let args = command
        .get_arguments()
        .map(|a| {
            (
                a.get_id().to_string(),
                a.get_help().map(ToString::to_string),
                a.is_positional(),
            )
        })
        .collect::<Vec<_>>();
    for (id, help, positional) in args {
        command = command.mut_arg(id, |a| {
            a.hide_env(true)
                .hide_default_value(true)
                .help(i18n::bilingual(
                    help.as_deref()
                        .unwrap_or(t(en, "命令参数", "Command argument")),
                    en,
                ))
                .help_heading(if positional {
                    t(en, "参数", "Arguments")
                } else {
                    t(en, "选项", "Options")
                })
        });
    }
    let names = command
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect::<Vec<_>>();
    for name in names {
        command = command.mut_subcommand(name, |c| localize(c, en, json));
    }
    command
}
fn default_data() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin) = exe.parent() {
            if bin.file_name().is_some_and(|n| n == "bin") {
                if let Some(prefix) = bin.parent() {
                    let data = prefix.join("var/lib/kairo");
                    if data.join("config.json").exists() {
                        return data;
                    }
                }
            }
        }
    }
    PathBuf::from("/var/lib/kairo")
}
fn prescan() -> (String, bool, PathBuf) {
    let args: Vec<String> = std::env::args().collect();
    let mut lang = None;
    let mut root = std::env::var_os("KAIRO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_data);
    for (i, s) in args.iter().enumerate() {
        if s == "--lang" {
            lang = args.get(i + 1).cloned()
        } else if let Some(v) = s.strip_prefix("--lang=") {
            lang = Some(v.into())
        } else if s == "--data-dir" {
            if let Some(v) = args.get(i + 1) {
                root = v.into()
            }
        } else if let Some(v) = s.strip_prefix("--data-dir=") {
            root = v.into()
        }
    }
    if lang.is_none() {
        lang = fs::read(root.join("config.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<Config>(&v).ok())
            .map(|c| c.language)
    };
    (
        lang.unwrap_or("zh-CN".into()),
        args.iter().any(|s| s == "--json"),
        root,
    )
}
fn main() {
    let (lang, json, _) = prescan();
    let en = lang == "en-US";
    let matches = match localize(Cli::command(), en, json).try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            if [
                clap::error::ErrorKind::DisplayHelp,
                clap::error::ErrorKind::DisplayVersion,
            ]
            .contains(&e.kind())
            {
                if json {
                    println!("{}", serde_json::json!({"help":e.to_string()}))
                } else {
                    print!("{e}")
                }
                return;
            }
            let msg = t(
                en,
                "命令或参数无效；请使用 --help 查看帮助",
                "Invalid command or arguments; use --help",
            );
            if json {
                println!(
                    "{}",
                    serde_json::json!({"error":{"code":"cli_invalid","message":msg}})
                )
            } else {
                eprintln!("{msg}")
            };
            std::process::exit(2)
        }
    };
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|_| unreachable!());
    cli.lang = Some(lang);
    let result = if !["zh-CN", "en-US"].contains(&cli.lang.as_deref().unwrap_or_default()) {
        Err(anyhow::anyhow!("language_invalid / 语言无效"))
    } else if let Some(cmd) = cli.command.take() {
        run(&cli, cmd)
    } else if cli.json {
        Err(anyhow::anyhow!("subcommand_required / 需要子命令"))
    } else {
        menu(&mut cli)
    };
    if let Err(e) = result {
        let (code, message) = i18n::error(&e, en);
        if cli.json {
            println!(
                "{}",
                serde_json::json!({"error":{"code":code,"message":message}})
            )
        } else {
            eprintln!("{}", color(&message, 31))
        }
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prewarm_estimate_waits_for_live_reset_and_respects_day_cutoff() {
        let config = Config::default();
        let tz = chrono_tz::Asia::Shanghai;
        let next = json!("2026-10-20T16:00:30+08:00");
        let cutoff = chrono::DateTime::parse_from_rfc3339("2026-10-21T00:00:00+08:00")
            .unwrap()
            .timestamp();
        let quota = json!({"server_denied":false,"weekly":{"used":3.0},"five_hour":{"used":21.0,"reset_at":chrono::DateTime::parse_from_rfc3339("2026-10-20T17:41:00+08:00").unwrap().timestamp()}});
        assert_eq!(
            possible_prewarm(&next, &quota, &config, cutoff, None, tz).as_deref(),
            Some("2026-10-20T17:41:30+08:00")
        );
        let next_day = json!({"server_denied":false,"weekly":{"used":3.0},"five_hour":{"used":21.0,"reset_at":chrono::DateTime::parse_from_rfc3339("2026-10-21T02:00:00+08:00").unwrap().timestamp()}});
        assert_eq!(
            possible_prewarm(&next, &next_day, &config, cutoff, None, tz).as_deref(),
            Some("2026-10-21T06:00:30+08:00")
        );
    }
    #[test]
    fn prewarm_estimate_refuses_denial_and_weekly_exhaustion() {
        let config = Config::default();
        let next = json!("2026-10-20T16:00:30+08:00");
        let quota = json!({"server_denied":true,"weekly":{"used":0.0},"five_hour":{"used":0.0}});
        assert!(possible_prewarm(&next, &quota, &config, 0, None, chrono_tz::UTC).is_none());
        let quota = json!({"server_denied":false,"weekly":{"used":100.0},"five_hour":{"used":0.0}});
        assert!(possible_prewarm(&next, &quota, &config, 0, None, chrono_tz::UTC).is_none());
    }
}
