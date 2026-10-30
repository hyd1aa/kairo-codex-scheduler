use serde_json::Value;
use std::io::{self, IsTerminal};
pub fn t<'a>(en: bool, zh: &'a str, english: &'a str) -> &'a str {
    if en {
        english
    } else {
        zh
    }
}
pub fn color(s: &str, code: u8) -> String {
    if io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").as_deref() != Ok("dumb")
    {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.into()
    }
}
pub fn bilingual(s: &str, en: bool) -> String {
    if let Some((a, b)) = s.split_once(" / ") {
        if en {
            a.replace('_', " ")
        } else {
            b.into()
        }
    } else {
        s.into()
    }
}
pub fn error(e: &anyhow::Error, en: bool) -> (String, String) {
    let s = e.to_string();
    if s.starts_with("activation_history_missing / ") {
        return ("io_error".into(), t(en,
            "部署授权或调度历史文件缺失；OAuth 校验成功不代表已绑定或已授权。请运行 status、doctor 并人工核对历史；不要自动 FRESH、伪造状态或删除保护记录",
            "Deployment approval or scheduler history files are missing. OAuth verification does not imply binding or production approval. Run status and doctor and review history; do not automatically FRESH-bind, fabricate state or delete guards").into());
    }
    if s.contains(" / ") {
        let code = s
            .split(" / ")
            .next()
            .unwrap_or("operation_failed")
            .to_lowercase()
            .replace([' ', '/'], "_");
        let english=match code.as_str() {
            "account_recovery_required"=>Some("Binding or scheduler history is missing. Choose main menu04 Start for automatic safe recovery, or use maintenance wizard Advanced06 ->03. Do not declare FRESH."),
            "unknown_history_ack_required"=>Some("Acknowledge unknown history only after stopping other schedulers and manual requests; use the existing-account recovery wizard."),
            "recovery_wait_required"=>Some("The conservative wait has not ended or new activity extended it. Run recover check for the updated deadline; no production approval was granted."),
            "recovery_quota_blocked"=>Some("Quota is invalid, weekly allowance exhausted or the server denied access. Keep observation mode and recheck identity/quota; recovery is not authorized."),
            "account_guard_corrupt"=>Some("The global account ledger is corrupt. Preserve it and review backups; automatic overwrite is forbidden."),
            "recovery_guard_missing"=>Some("The account guard disappeared during quarantine. Run recover begin again to restart conservative waiting; do not approve directly."),
            "recovery_owner_or_clock"=>Some("Recovery belongs to another data directory or the clock moved backward; approval is blocked."),
            "recovery_owner_mismatch"=>Some("Global recovery approval and local history/directory do not match. Run recover check; model execution is blocked."),
            "recovery_approval_required"=>Some("Unknown-history recovery is not approved. Complete the wait, recheck and explicitly enable scheduling."),
            "managed_root_required"=>Some("Automatic service cutover requires root and the standard /var/lib/kairo installation. Development directories never modify the real service."),
            "live_status_unconfirmed"=>Some("The daemon did not confirm a fresh live status. Authorization was rolled back; inspect logs before retrying enable."),
            "cutover_rollback_incomplete"=>Some("Service cutover failed and rollback is incomplete. Inspect kairo.service and deployment approval; do not keep retrying automatically."),
            "systemd_cutover_failed"=>Some("The systemd handoff failed. Production approval is being rolled back; inspect service logs."),
            "managed_service_changed"=>Some("The unit differs from the managed installer template. Custom service files are preserved; review the handoff manually."),
            "custom_service_override"=>Some("Custom or runtime systemd overrides exist. They will not be deleted; review them manually."),
            "pause_before_recovery"=>Some("The account differs or production is already approved. Pause scheduling and inspect the records before recovery."),
            _=>None,
        };
        let message = if en {
            english
                .map(str::to_string)
                .unwrap_or_else(|| bilingual(&s, true))
        } else {
            bilingual(&s, false)
        };
        return (code, message);
    }
    let code = if e.chain().any(|x| x.is::<std::io::Error>()) {
        "io_error"
    } else if e.chain().any(|x| x.is::<serde_json::Error>()) {
        "json_invalid"
    } else {
        "input_invalid"
    };
    let msg = match code {
        "io_error" => t(
            en,
            "文件或进程操作失败；请检查路径、权限及服务状态",
            "File or process operation failed; check paths, permissions and service state",
        ),
        "json_invalid" => t(
            en,
            "JSON 数据无效或版本不兼容",
            "JSON data is invalid or its schema is incompatible",
        ),
        _ => t(
            en,
            "输入无效；请检查参数并查看帮助",
            "Invalid input; check arguments and help",
        ),
    };
    (code.into(), msg.into())
}
pub fn label(k: &str, en: bool) -> String {
    if en {
        return match k {
            "state_valid" => "Complete scheduler readiness (not production approval)".into(),
            "existing_records_valid" => {
                "Existing local configuration/record integrity (absence is not readiness)".into()
            }
            _ => k.replace('_', " "),
        };
    };
    match k {
        "version" => "版本",
        "timezone" => "系统时区",
        "config" => "配置",
        "next_check" => "下次实际检查",
        "execution" => "执行模式",
        "account_origin" => "账号来源声明",
        "files" => "调度文件",
        "oauth_discovered" => "已发现 OAuth（非服务端验证）",
        "local_binding_valid" => "本地绑定有效",
        "account_guard_valid" => "全局账号保护有效",
        "history_proves_new_account" => "存在可证明新账号的完整历史",
        "recovery" => "未知历史恢复记录",
        "recovery_record_invalid" => "恢复记录无效",
        "next_step" => "下一步",
        "history_before_start" => "基线之前的请求历史",
        "not_before" => "最早恢复时间",
        "owner" => "恢复所属目录",
        "started_at" => "恢复开始时间",
        "approved" => "恢复已批准",
        "observation_override" => "systemd 强制观察模式",
        "managed_service" => "标准安装器服务",
        "service_active" => "服务正在运行",
        "state_valid" => "调度状态完整就绪（不代表生产授权）",
        "existing_records_valid" => "已有配置/文件完整性（缺失不算就绪）",
        "scheduler_state_ready" => "调度状态完整就绪",
        "production_authorized" => "生产授权",
        "automatic_warmup_enabled" => "自动预热已启用",
        "recovery_status" => "恢复诊断",
        "classification" => "账号与记录分类",
        "state_file_valid" => "本地状态文件格式有效",
        "service_mode" => "实际服务模式",
        "daemon_live_confirmed" => "已确认正式守护进程",
        "language" => "语言",
        "anchors" => "激活时间",
        "max_anchors" => "时间数量上限",
        "minimum_spacing_minutes" => "最小间隔（分钟）",
        "buffer_seconds" => "安全缓冲（秒）",
        "checks" => "计划检查时间",
        "idle_window_prewarm" => "空闲窗口预热",
        "non_optional_guards" => "不可关闭的保护",
        "state" => "窗口生命周期状态",
        "five_hour" => "五小时额度",
        "weekly" => "每周额度",
        "server_denied" => "服务端拒绝",
        "duration" => "窗口时长（秒）",
        "used" => "已用百分比",
        "reset_at" => "重置时间戳",
        "remaining" => "剩余秒数",
        "account_fingerprint" => "账号指纹",
        "oauth_present" => "已发现 OAuth",
        "identity_server_verified" => "服务端身份已验证",
        "distribution" => "发行版",
        "distribution_version" => "发行版版本",
        "arch" => "架构",
        "systemd" => "systemd 可用",
        "codex_safety_flags" => "Codex 安全参数可用",
        "status" => "状态",
        "active_state" => "运行状态",
        "load_state" => "加载状态",
        "success" => "成功",
        "schema" => "数据版本",
        "account" => "账号绑定",
        "last_clock" => "最近请求前时钟",
        "attempts" => "调用记录",
        "legacy_attempts" => "迁移调用与处理记录",
        "request_ok" => "原请求已成功",
        "source" => "来源类型",
        "positive_until" => "已知活跃窗口截止",
        "pending" => "待检查时间",
        "anchor" => "锚点",
        "at" => "记录时间戳",
        "completed_at" => "请求完成记录时间",
        "before" => "调用前额度",
        "after" => "调用后额度",
        "outcome" => "结果",
        "decision" => "调度决策",
        "model_requests" => "真实模型请求数量",
        "observations" => "窗口观测历史",
        "phase" => "生命周期阶段",
        "quota" => "额度",
        "bound" => "账号已绑定",
        "quarantine_until" => "保守保护截止",
        "selected" => "当前账号配置",
        "profiles" => "账号配置列表",
        "path" => "路径",
        "repair_applied" => "已执行安全修复",
        "service" => "服务",
        "action" => "操作",
        "event" => "调度事件",
        "error" => "错误",
        "message" => "消息",
        "code" => "诊断代码",
        "update_available" => "有可用更新",
        "target" => "目标架构",
        "sha256" => "SHA256 校验值",
        "url" => "下载地址",
        "prefix" => "安装目录",
        "data_dir" => "数据目录",
        "installed" => "已安装",
        "validated" => "已校验",
        "migrated" => "已迁移",
        "initialized" => "已初始化",
        "enabled" => "允许真实执行",
        "eligible" => "满足预热条件",
        "approved_at" => "生产批准时间",
        "simulated_calls" => "模拟调用次数",
        "logs" => "运行日志",
        "MESSAGE" => "日志消息",
        "authenticity_verified" => "更新来源已验证",
        "live_enabled" => "允许真实执行",
        "main_pid" => "主进程编号",
        "status_file" => "守护进程状态",
        "acceptance" => "预热验收",
        "natural_dispatch" => "自然调度验收",
        "recent_request_at" => "最近真实预热时间",
        "recent_request_outcome" => "最近真实预热结果",
        "confirmation_status" => "窗口确认状态",
        "request_success" => "模型请求成功",
        "countdown_confirmed" => "窗口稳定倒计时已确认",
        "time_correlated" => "与请求时间吻合",
        "attribution_uncertain" => "请求归因不确定",
        "last_confirmation_check" => "最近确认检查时间",
        "pending_confirmation" => "存在待确认任务",
        "next_confirmation_check" => "下次确认检查时间",
        "confirmation_checks" => "已检查次数",
        "confirmation_max_checks" => "确认次数上限",
        "confirmation_history" => "确认任务历史数量",
        "confirmation_last_event" => "最近确认事件",
        "confirmation_last_error" => "最近确认异常原因",
        "confirmation_details" => "最近确认任务详情",
        "samples" => "只读额度样本",
        "events" => "确认事件",
        "checked_at" => "检查时间",
        "remaining_seconds" => "剩余秒数",
        "used_percent" => "已用百分比",
        "kind" => "事件类型",
        "deadline" => "确认截止时间",
        "check_count" => "确认检查次数",
        "request_at" => "模型请求时间",
        "current_quota" => "当前额度",
        "next_possible_prewarm" => "下次可能预热时间",
        "prewarm_conditional" => "预热取决于安全门禁",
        "recent_reason" => "最近调度或拒绝原因",
        "reset_at_local" => "本地重置时间",
        "at_local" => "本地请求时间",
        "completed_at_local" => "本地完成时间",
        "approved_at_local" => "本地批准时间",
        "next_check_local" => "本地下次检查",
        "quota_or_identity_unavailable" => "额度或身份暂不可用",
        "help" => "帮助",
        "codex_home" => "Codex 认证目录",
        "codex" => "Codex 可执行文件",
        "home" => "用户主目录",
        "interval_seconds" => "配置检查间隔（秒）",
        "runtime" => "运行配置",
        "deployment" => "部署授权",
        "account_count" => "账号配置数量",
        "paused" => "已暂停真实执行",
        "removed" => "已删除",
        "valid" => "有效",
        "timestamp" => "时间戳",
        _ => k,
    }
    .into()
}
pub fn value(s: &str, en: bool) -> String {
    static CATALOG: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("messages.json")).expect("validated translation catalog")
    });
    if let Some(message) = catalog[s][if en { "en" } else { "zh" }].as_str() {
        return message.into();
    }

    let (zh, english) = match s {
        "fresh_bound_not_authorized" => (
            "新账号声明已绑定；尚未生产授权",
            "Fresh declaration bound; not production-approved",
        ),
        "recovery_owned_elsewhere" => (
            "恢复属于其他数据目录",
            "Recovery owned by another data directory",
        ),
        "administrator_declared_fresh" => (
            "管理员声明首次新账号（非 API 历史证明）",
            "Administrator declared fresh (not API history proof)",
        ),
        "existing_unknown_history_recovery" => (
            "已有账号未知历史恢复",
            "Existing-account unknown-history recovery",
        ),
        "oauth_missing" => (
            "未发现有效 OAuth；先登录与校验",
            "OAuth missing; log in and verify",
        ),
        "data_corrupt" => (
            "数据损坏或无法读取；保留证据并恢复",
            "Data corrupt/unreadable; preserve evidence and recover",
        ),
        "history_unknown" => (
            "调度历史未知；新旧账号必须明确选择",
            "Scheduler history unknown; explicitly choose first use or recovery",
        ),
        "recovery_waiting" => (
            "已有账号恢复等待中；尚未授权",
            "Existing-account recovery waiting; not approved",
        ),
        "not_authorized" => (
            "状态完整但尚未生产授权",
            "Records ready; production not authorized",
        ),
        "authorized" => (
            "生产授权已取得；仍需验证实际服务模式",
            "Production approved; verify effective service mode",
        ),
        "unknown" => (
            "未知（未还原为无请求历史）",
            "Unknown (not reconstructed as empty history)",
        ),
        "missing" => ("文件缺失", "Missing file"),
        "valid" => ("有效", "Valid"),
        "invalid_or_unreadable" => ("无效或读取失败", "Invalid or unreadable"),
        "processed_without_request" => (
            "原锚点已处理，无请求记录",
            "Legacy anchor processed without a request record",
        ),
        "normalized-kairo-transfer" => ("标准化 Kairo 迁移包", "Normalized Kairo transfer bundle"),
        "weekly" => ("每周额度保护", "Weekly allowance protection"),
        "durable_reservation" => ("调用前持久化保护", "Durable reservation before invocation"),
        "io_error" => ("文件或进程操作失败", "File or process operation failed"),
        "json_invalid" => (
            "数据损坏或版本不兼容",
            "Corrupt data or incompatible schema",
        ),
        "runtime_error" => (
            "运行异常，已安全阻止执行",
            "Runtime error; execution blocked safely",
        ),

        "observation_only" => (
            "只读观察，不发送模型请求",
            "Observation only; no model requests",
        ),
        "live" => ("经批准的真实调度", "Approved live scheduling"),
        "unbound" => ("尚未绑定账号", "Account not bound"),
        "waiting" => ("等待下次检查", "Waiting for next check"),
        "armed" => ("已规划未来检查", "Future check scheduled"),
        "deferred" => ("等待真实窗口重置", "Waiting for actual window reset"),
        "eligible_observation_only" => (
            "符合条件，仅观察未调用",
            "Eligible; observed without a request",
        ),
        "rolled_zero_usage" => ("窗口滚动且零用量", "Rolled window with zero usage"),
        "request_ok_window_unconfirmed" => (
            "请求成功，窗口尚未确认",
            "Request succeeded; window unconfirmed",
        ),
        "countdown_confirmed_time_correlated" => (
            "窗口倒计时及时间关联已确认",
            "Window countdown and timing correlation confirmed",
        ),
        "attribution_uncertain" => (
            "窗口倒计时已确认，归因不确定",
            "Countdown confirmed; attribution uncertain",
        ),
        "check_limit_reached" => (
            "达到确认次数上限，未自动重试模型",
            "Confirmation check limit reached; model not retried",
        ),
        "quota_or_identity_unavailable" => ("额度或身份暂不可用", "Quota or identity unavailable"),
        "quota_or_identity_error" => ("额度或身份检查失败", "Quota or identity check failed"),
        "quota_invalid" => ("额度响应无效", "Invalid quota response"),
        "request_success_window_unconfirmed" => (
            "请求成功，等待窗口确认",
            "Request succeeded; awaiting window confirmation",
        ),
        "pending" => ("等待确认", "Pending confirmation"),
        "timeout" => (
            "确认超时，不重试模型",
            "Confirmation timed out; model not retried",
        ),
        "first_natural_request_recorded" => {
            ("已记录首次自然请求", "First natural request recorded")
        }
        "quota_read_ok" => ("只读额度检查成功", "Read-only quota check succeeded"),
        "five_hour_window_confirmed" => ("五小时窗口已确认", "Five-hour window confirmed"),
        "request_failed" => ("请求失败，不自动重试", "Request failed; no automatic retry"),
        "attempt_persisted_unknown" => (
            "调用前已落盘，结果未知",
            "Attempt persisted; outcome unknown",
        ),
        "attempt_guard" => ("近期或同锚点请求保护", "Recent-request or anchor guard"),
        "server_denied" => ("服务端拒绝保护", "Server denial protection"),
        "quota_invalid_or_weekly_exhausted" => (
            "额度无效或每周额度耗尽",
            "Invalid quota or weekly allowance exhausted",
        ),
        "state_or_clock" => ("状态或时钟保护", "State or clock protection"),
        "daily_cutoff" | "cutoff" => ("已到锚点当日截止时间", "Anchor date cutoff reached"),
        "config_replanned" => (
            "配置变化，计划已安全重算",
            "Configuration changed; plan recalculated",
        ),
        "timezone_replanned" => (
            "系统时区变化，计划已重算",
            "System timezone changed; plan recalculated",
        ),
        "missed_not_replayed" => (
            "已错过检查，不补跑",
            "Missed check skipped without catch-up",
        ),
        "blocked" => ("安全门禁阻止执行", "Execution blocked by safety gate"),
        "active" => ("活跃", "Active"),
        "inactive" => ("未运行", "Inactive"),
        "loaded" => ("已加载", "Loaded"),
        "not-found" => ("未安装", "Not installed"),
        "failed" => ("运行失败", "Failed"),
        "expired" => ("窗口已到期", "Expired window"),
        "zero_usage_uncertain" => ("零用量，窗口尚不确定", "Zero usage; window uncertain"),
        "inconsistent_reset_history" => (
            "重置历史不一致，禁止调用",
            "Inconsistent reset history; request blocked",
        ),
        "start" => ("启动", "Start"),
        "stop" => ("停止", "Stop"),
        "restart" => ("重启", "Restart"),
        "enable" => ("启用开机启动", "Enable at boot"),
        "disable" => ("禁用开机启动", "Disable at boot"),
        "status" => ("状态", "Status"),
        "timezone_unknown" => ("无法识别系统时区", "System timezone unknown"),
        _ => return s.into(),
    };
    t(en, zh, english).into()
}
fn text(v: &Value, en: bool) -> String {
    match v {
        Value::Null => t(en, "暂无记录", "Not recorded").into(),
        Value::Bool(b) => color(
            if *b {
                t(en, "是", "Yes")
            } else {
                t(en, "否", "No")
            },
            if *b { 32 } else { 33 },
        ),
        Value::String(s) => {
            if s.contains(':') && !s.starts_with('/') && !s.starts_with("http") {
                s.split(':')
                    .map(|x| value(x, en))
                    .collect::<Vec<_>>()
                    .join(":")
            } else {
                value(s, en)
            }
        }
        Value::Array(v) => v.iter().map(|x| text(x, en)).collect::<Vec<_>>().join(", "),
        _ => v.to_string(),
    }
}
pub fn render(v: &Value, en: bool, indent: usize) {
    let pad = " ".repeat(indent);
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                if v.is_object() || v.as_array().is_some_and(|a| a.iter().any(Value::is_object)) {
                    println!("{pad}{}:", label(k, en));
                    if let Some(a) = v.as_array() {
                        for v in a {
                            render(v, en, indent + 2)
                        }
                    } else {
                        render(v, en, indent + 2)
                    }
                } else {
                    println!("{pad}{}: {}", label(k, en), text(v, en))
                }
            }
        }
        _ => println!("{pad}{}", text(v, en)),
    }
}
