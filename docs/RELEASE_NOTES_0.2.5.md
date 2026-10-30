# Kairo v0.2.5

首页直接显示已有请求与倒计时确认结果，不再要求普通用户查看 journalctl/JSON。请求、确认记录按账号和请求关联；仅时间关联的倒计时确认保留归因不确定性。服务运行和剩余额度100%都不是成功证据。

- 01页面显示额度、最近预热记录及已有失败原因；原始JSON移到更多设置。
- 恢复完成后隐藏旧复查日期；日常计划明确称为“下次调度检查”。
- 中英文自然时间及带查询时间的缓存额度；五小时与每周剩余同时显示。
- 修复首次请求之后日常检查摘要没有继续同步的问题，复用原有验收文件。
- 输出消费者关闭管道时安静退出，不再产生Rust Broken Pipe panic；正常JSON与业务错误退出码保留。
- 真实旧版本二进制升级/回滚测试包含已确认的倒计时历史，全部使用模拟数据，不发模型请求。

No scheduler algorithm, quota policy, safety gate or new persistent state system. Missing historical evidence is explicitly unavailable, never fabricated.
