mod i18n;
mod panel;
use anyhow::{bail, Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use i18n::{color, t};
use kairo_core::{plan, Config};
use kairo_platform::{management as mg, runtime as rt, timezone, update as up, Store};
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
    about = "Scheduled Codex CLI calls / Codex CLI 定时调用工具"
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
    #[command(about = "Show recorded calls and legacy evidence / 查看调用记录和旧版证据")]
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
    #[command(about = "Manage daily call times / 管理每日调用时间")]
    Schedule {
        #[command(subcommand)]
        action: Schedule,
    },
    #[command(about = "Persist interface language / 保存界面语言")]
    Lang {
        #[arg(help = "zh-CN or en-US / zh-CN 或 en-US")]
        language: String,
    },
    #[command(
        about = "Configure call delay; legacy idle switch is ignored / 设置调用延迟，旧空闲开关已忽略"
    )]
    Prewarm {
        #[arg(long, help = "Legacy switch, ignored / 旧版兼容开关，已忽略")]
        idle: Option<bool>,
        #[arg(long, help = "Call delay in seconds: 0..300 / 调用延迟秒数：0..300")]
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
            help = "Legacy compatibility flag, ignored / 旧版兼容参数，已忽略"
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
    #[command(about = "Run scheduled Codex calls / 运行 Codex 定时调用")]
    #[command(
        about = "Show scheduling readiness; --request resumes / 查看待命状态；--request 启动"
    )]
    Onboard {
        #[arg(long)]
        request: bool,
        #[arg(long)]
        confirm_exclusive_account: bool,
    },
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
    #[command(about = "Migrate local settings and call history / 迁移本地配置和调用记录")]
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
    #[command(about = "Resume scheduling (legacy alias) / 启动定时调用（兼容命令）")]
    Activate {
        #[arg(long, help = "Verified account fingerprint / 已验证账号指纹")]
        account: String,
        #[arg(
            long,
            help = "Legacy compatibility flag, ignored / 旧版兼容参数，已忽略"
        )]
        approve_production_cutover: bool,
    },
    #[command(
        about = "Legacy readiness aliases; no recovery wait / 旧待命检查兼容命令，无恢复等待"
    )]
    Recover {
        #[command(subcommand)]
        action: RecoveryAction,
    },
    #[command(about = "Resume scheduled calls / 启动定时调用")]
    Enable {
        #[arg(
            long,
            help = "Legacy compatibility flag, ignored / 旧版兼容参数，已忽略"
        )]
        approve_production_cutover: bool,
    },
    #[command(about = "Pause Kairo model requests / 暂停 Kairo 模型请求")]
    Pause,
}
#[derive(Subcommand)]
enum RecoveryAction {
    #[command(about = "Legacy readiness status / 兼容待命状态查询")]
    Status,
    #[command(about = "Legacy readiness check; no wait / 兼容待命检查，无恢复等待")]
    Begin {
        #[arg(long)]
        acknowledge_unknown_history: bool,
    },
    #[command(about = "Read readiness without changing pause / 查看待命，不改变暂停")]
    Check,
    #[command(about = "Legacy alias; no recovery wizard required / 兼容命令，不需要恢复向导")]
    Wizard,
}
#[derive(Subcommand)]
enum Schedule {
    #[command(about = "List call times / 查看调用时间")]
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
fn emit(cli: &Cli, value: Value) -> Result<()> {
    if panel::active() {
        panel::render(&value, cli.lang.as_deref() == Some("en-US"));
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}
fn configured(cli: &Cli) -> Result<Store> {
    Store::open(&cli.data_dir)
}
fn next_checks(_store: &Store, c: &Config, hours: i64) -> Result<Value> {
    let tz = timezone()?;
    Ok(
        json!({"timezone":tz.to_string(),"buffer_seconds":c.buffer_seconds,"checks":plan(c,tz,chrono::Utc::now(),hours)?.into_iter().map(|d|d.with_timezone(&tz).to_rfc3339()).collect::<Vec<_>>()}),
    )
}
fn status(store: &Store) -> Result<Value> {
    let c = store.config()?;
    let s = rt::ScheduleState::load(store)?;
    let r = rt::runtime(store)?;
    let next = next_checks(store, &c, 72)?["checks"]
        .as_array()
        .and_then(|a| a.first())
        .cloned();
    let logged = kairo_platform::codex::logged_in(store, &r).unwrap_or(false);
    let quota_cache = store
        .read::<Value>("quota-cache.json")
        .ok()
        .filter(|cache| {
            let account = kairo_platform::identity(&r.codex_home.join("auth.json")).ok();
            account.is_some_and(|id| cache["account"] == id.fingerprint)
                && cache["at"]
                    .as_i64()
                    .is_some_and(|at| at > 0 && at <= chrono::Utc::now().timestamp())
        });
    Ok(
        json!({"version":env!("CARGO_PKG_VERSION"),"timezone":timezone()?.to_string(),"config":c,"enabled":!s.paused,"paused":s.paused,"oauth_present":logged,"next_call":next,"next_check":next,"last_call":s.calls.last(),"calls":s.calls,"quota_cache":quota_cache,"quota_optional":true,"state_valid":true,"scheduler_state_ready":true,"automatic_warmup_enabled":!s.paused,"prewarm_conditional":false}),
    )
}
fn acceptance(store: &Store) -> Result<Value> {
    let s = rt::ScheduleState::load(store)?;
    let old = store
        .read::<Value>("confirmations.json")
        .unwrap_or_default();
    let confirmed = old["tasks"].as_array().is_some_and(|tasks| {
        tasks
            .iter()
            .any(|t| t["countdown_confirmed"] == true && t["request_success"] == true)
    });
    Ok(
        json!({"last_call":s.calls.last(),"calls":s.calls,"request_success":s.calls.last().is_some_and(|c|matches!(c.result.as_str(),"success"|"legacy_success")),"countdown_confirmed":confirmed && s.calls.last().is_some_and(|c|c.result=="legacy_success"),"legacy_confirmation_read_only":true,"new_window_claimed":false}),
    )
}
fn run(cli: &Cli, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Install {
            binary,
            sha256,
            prefix,
        } => {
            let prefix = up::safe_prefix(&prefix)?;
            {
                let s = configured(cli)?;
                if fs::symlink_metadata(s.root.join("config.json"))
                    .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
                {
                    let c = s
                        .retained::<Config>("config.json")?
                        .unwrap_or_else(|| Config {
                            language: cli.lang.clone().unwrap_or("zh-CN".into()),
                            ..Default::default()
                        });
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
                json!({"installed":true,"prefix":prefix,"execution":"scheduled"}),
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

        Cmd::Daemon {
            observe,
            once,
            fixture,
            poll_ms,
            hold,
        } => {
            return if let Some(path) = fixture {
                rt::replay(&cli.data_dir, &path, poll_ms, hold)
            } else {
                rt::daemon(&cli.data_dir, once, observe)
            }
        }
        _ => {}
    }
    let store = configured(cli)?;
    if matches!(cmd, Cmd::Init) {
        if kairo_platform::present(&store.root.join("config.json"))? {
            bail!("already_initialized / 已初始化");
        }
        let c = Config {
            language: cli.lang.clone().unwrap_or("zh-CN".into()),
            ..Default::default()
        };
        c.validate()?;
        store.write("config.json", &c)?;
        store.write("runtime.json", &rt::Runtime::default())?;
        mg::migrate(&store)?;
        return emit(cli, json!({"initialized":true,"execution":"scheduled"}));
    }
    let mut c = store.config()?;
    match cmd {
        Cmd::Status => emit(cli, status(&store)?),
        Cmd::Acceptance => emit(cli, acceptance(&store)?),
        Cmd::RuntimeSettings {
            home,
            codex_home,
            codex,
        } => {
            let mut r = rt::runtime(&store)?;
            let changed = home.is_some() || codex_home.is_some() || codex.is_some();
            if let Some(v) = home {
                r.home = v;
            }
            if let Some(v) = codex_home {
                r.codex_home = v;
            }
            if let Some(v) = codex {
                r.codex = v;
            }
            r.validate()?;
            if changed {
                store.write("runtime.json", &r)?;
            }
            emit(cli, serde_json::to_value(r)?)
        }
        Cmd::Schedule { action } => {
            let mut changed = true;
            match action {
                Schedule::Add { time } => c.anchors.push(time),
                Schedule::Edit { old, new } => {
                    let at = c
                        .anchors
                        .iter()
                        .position(|t| t == &old)
                        .context("time_missing / 时间不存在")?;
                    c.anchors[at] = new;
                }
                Schedule::Remove { time } => {
                    let at = c
                        .anchors
                        .iter()
                        .position(|t| t == &time)
                        .context("time_missing / 时间不存在")?;
                    c.anchors.remove(at);
                }
                Schedule::Reset => c.anchors = Config::default().anchors,
                Schedule::Next => return emit(cli, next_checks(&store, &c, 72)?),
                Schedule::Preview => return emit(cli, next_checks(&store, &c, 24)?),
                Schedule::List => changed = false,
            }
            c.anchors.sort();
            c.validate()?;
            if changed {
                store.write("config.json", &c)?;
            }
            emit(
                cli,
                json!({"anchors":c.anchors,"buffer_seconds":c.buffer_seconds}),
            )
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
                c.buffer_seconds = 30;
            }
            if let Some(b) = buffer {
                c.buffer_seconds = b;
            }
            if let Some(i) = idle {
                c.idle_window_prewarm = i;
            }
            c.validate()?;
            if reset || buffer.is_some() || idle.is_some() {
                store.write("config.json", &c)?;
            }
            emit(
                cli,
                json!({"buffer_seconds":c.buffer_seconds,"idle_window_prewarm":c.idle_window_prewarm,"legacy_idle_setting_ignored":true,"quota_optional":true}),
            )
        }
        Cmd::Quota { fixture } => {
            let now = chrono::Utc::now().timestamp();
            let (f, w, d, account) = if let Some(path) = fixture {
                let v: Value = serde_json::from_slice(&fs::read(path)?)?;
                let (f, w, d) = kairo_platform::parse_usage(&v, now)?;
                (f, w, d, String::from("synthetic"))
            } else {
                let r = rt::runtime(&store)?;
                let id = kairo_platform::identity(&r.codex_home.join("auth.json"))?;
                let v = kairo_platform::quota(&id)?;
                if v["account_id"] != id.account_id {
                    bail!("server_identity_mismatch / 额度账号不匹配");
                }
                let (f, w, d) = kairo_platform::parse_usage(&v, now)?;
                (f, w, d, id.fingerprint)
            };
            let value = json!({"five_hour":f,"weekly":w,"server_denied":d,"account":account,"at":now,"informational_only":true});
            store.write("quota-cache.json", &value)?;
            emit(cli, value)
        }
        Cmd::Auth {
            action,
            name,
            home,
            acknowledge_fresh: _,
        } => match action.as_str() {
            "login" => {
                if cli.json || !io::stdin().is_terminal() {
                    bail!("interactive_login_required / OAuth 登录需要交互终端");
                }
                mg::login(&store)?;
                emit(cli, json!({"success":true}))
            }
            "bind" => emit(cli, json!({"binding_required":false,"ready":true})),
            _ => emit(
                cli,
                mg::account_action(&store, &action, name.as_deref(), home.as_deref())?,
            ),
        },
        Cmd::Pause => emit(cli, rt::pause(&store, true)?),
        Cmd::Enable { .. } | Cmd::Activate { .. } => emit(cli, rt::pause(&store, false)?),
        Cmd::Onboard { request, .. } => {
            if request {
                emit(cli, rt::pause(&store, false)?)
            } else {
                emit(cli, status(&store)?)
            }
        }
        Cmd::Recover { .. } => emit(
            cli,
            json!({"recovery_required":false,"wait_seconds":0,"status":status(&store)?}),
        ),
        Cmd::Migrate {
            import,
            legacy,
            dry_run,
            ..
        } => {
            if import.is_some() || legacy.is_some() {
                bail!("legacy_import_retired / 原始旧记录请保留在原目录，由迁移读取；不再导入额度授权");
            }
            if dry_run {
                mg::validate(&store)?;
            } else {
                mg::migrate(&store)?;
            }
            emit(cli, json!({"migrated":!dry_run,"validated":true}))
        }
        Cmd::Validate => {
            mg::validate(&store)?;
            emit(cli, json!({"validated":true}))
        }
        Cmd::Doctor { repair } => emit(cli, mg::doctor(&store, repair)?),
        Cmd::Service { action } => {
            drop(store);
            emit(cli, mg::service(&action)?)
        }
        Cmd::Logs { lines } => {
            if !(1..=10000).contains(&lines) {
                bail!("log_limit_invalid / 日志数量无效");
            }
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
                bail!("logs_failed / 日志读取失败");
            }
            let logs = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            emit(cli, json!({"logs":logs}))
        }
        Cmd::HelpInfo => emit(
            cli,
            json!({"help":t(cli.lang.as_deref()==Some("en-US"),"Kairo 在预设时间调用一次官方 Codex CLI。额度仅供参考，不保证请求成功或激活新窗口。","Kairo calls the official Codex CLI once at each scheduled time. Quota is informational; requests and new windows are not guaranteed.")}),
        ),
        Cmd::Simulate { fixture } => {
            let v: Value = serde_json::from_slice(&fs::read(fixture)?)?;
            emit(
                cli,
                json!({"checks":plan(&c,timezone()?,chrono::DateTime::from_timestamp(v["now"].as_i64().context("clock_missing")?,0).context("clock_invalid")?,24)?,"model_requests":0}),
            )
        }
        _ => unreachable!(),
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
static PIPE_EXIT_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
extern "C" fn output_closed(_: libc::c_int) {
    // Async-signal-safe: a closed consumer is normal CLI termination, not a Rust panic.
    unsafe { libc::_exit(PIPE_EXIT_CODE.load(std::sync::atomic::Ordering::Relaxed)) }
}
fn main() {
    unsafe {
        libc::signal(
            libc::SIGPIPE,
            output_closed as *const () as libc::sighandler_t,
        );
    }

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
            PIPE_EXIT_CODE.store(2, std::sync::atomic::Ordering::Relaxed);
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
        PIPE_EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
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
