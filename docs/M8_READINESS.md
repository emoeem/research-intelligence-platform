# M8 就绪评审：受控切换检查单与结论

日期：2026-11-06 04:0x（Asia/Shanghai）　执行：主会话　依据：主计划书 §M8 四项退出门槛 + 授权条款（"确定完全可切换再切"）

## 一、四项硬检查

| # | 检查 | 结果 | 证据 |
|---|---|---|---|
| 1 | 对账 100% 通过 | ✅ PASS | `migrations/verify.py` 全量：一致率 100%、exit 0（含本轮语义强化：源快照锁定、活源 drift 收敛豁免、entity_alias 孤儿全域校验 269 子表行补全） |
| 2 | 影子运行差异已解释 | ✅ PASS | M5 `shadow_report.json`：3 组真实查询 verdict=all_diffs_explained，预期差异 10（platform 缺 CJK 桥/topics 轴/license 轴，均有归因），异常 0 |
| 3 | 回滚演练成功 | ✅ PASS | 2026-11-06 04:0x 实操：pre-m5 备份还原 → 自测 13 过 4 败（基线同集）→ 重应用 M5 版 → md5 逐字节一致（2fa9979b…）→ 自测同结果 |
| 4 | 切换前快照完备 | ✅ PASS | `migrations/sources-snapshot/`：catalog/knowledge/platform-dryrun/platform-search 四库冻结副本 + tools.json + 42 task JSON + manifest.sha256（7 项） |

## 二、切换对象与模式（当前状态：READY，未切换）

- 切换开关：`RTIS_RESOLVER_MODE=rp|legacy|off`（生产 rtis.py，默认 **legacy 不变**）。
- rp 路径现状：查 `migrations/platform-dryrun.db`（81 资源 dryrun 口径）。**切到 rp 生产态前的唯一硬前提**：`RTIS_RP_DB` 指向生产化 platform 库（M1 全量批跑后的正式库），并把 M4 已归因的三轴缺口（CJK 桥/topics/license）补齐或接受降级。
- 推荐切换序列（均为可逆操作）：`shadow（现状）→ rp 并行（RTIS_RESOLVER_MODE=rp，legacy 保留）→ 默认翻转（改默认值=单行 diff）→ legacy 封存`。

## 三、已知限制（切换前必须知悉，均已在各阶段报告留痕）

1. evidence 抽样口径：4/26 兼容库、每表前 20 行；22 库结构与行数已登记未迁（M1 第二批报告）。
2. research_task 仅映射核心字段；authorization/environment/knowledge 大 JSON 段留接续批次。
3. rp 侧三轴缺口（M2/M4 报告）：topics 轴已补、CJK 桥 Rust 侧为 2-gram+桥表、license/maintenance 轴数据源未接。
4. Worker embedding 为确定性占位（256 维 hashing），真实模型接入留 VectorModel 接口。
5. M7 工作台为只读骨架（无鉴权，127.0.0.1 绑定缓解）。
6. 生产 rtis.py 自测基线 13 过 4 败为改动前既有（T2/T4/T11/T13），非本平台引入。

## 四、结论

**READY（有条件）**：平台侧全部里程碑（M0-M7）交付并验证，四项硬检查全绿；生产切换的技术与流程条件已备。切换时机授权在项目负责人手中，按第 2 节序列执行、每步可逆、每步有回滚点。
