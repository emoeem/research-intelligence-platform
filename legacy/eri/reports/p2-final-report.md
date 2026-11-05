# ERI P2 最终报告：RTIS 与外部资源智能检索深度集成

生成：2026-11-05T15:31:07Z　执行：主会话　基线：P0/P1 测试 ALL PASS 后动工（23:08 起）

## 十二问逐答

1. **ERI 是否真正接入 RTIS？** 是。gate() 尾部 subprocess 钩子，端到端实测（真实任务 rtis-20261105-231342-2697），结果入 task JSON。可 RTIS_ERI=off 一键关闭，故障静默降级。
2. **哪些任务自动查询外部资源？** 所有走 rtis.py gate 的任务（知识门禁完成后）。按计划顺序：内部知识检索先行，ERI 在其后。
3. **查询排序是什么？** 六维加权（keyword .25 / category .20 / tech .15 / profile .15 / relation .15 / quality .10），无证据维度不计分不虚构，权重可配置。
4. **哪些资源被推荐？** 实测示例：落地页任务 -> hacker101（redirect）；API 任务 -> API Security Checklist；Android 任务 -> android-security-awesome。
5. **为什么推荐？** 每条带 relevance_reasons（命中词/分类/画像/关系/质量），且质量/license/维护状态随行。
6. **哪些被排除？** QUARANTINED/REJECTED/UNAVAILABLE；license 未知的标记"仅供参考不可采用"；无证据匹配不进候选（场景E空集验证）。
7. **外部内容如何读取缓存？** Reader：ETag 条件请求 + 内容哈希 + 章节目录缓存 + tarball 回落 + stale 标注；实测缓存 2 行（readme+license），二读零网络。
8. **Token 消耗？** 全阶段 0（resolver SQL/reader HTTP/审核规则全确定性）；ai_usage_log 记账表就位。
9. **D5 处理？** 68 条全部规则复核留痕：63 NEEDS_MORE_EVIDENCE、5 RELATED_KEEP_BOTH；零删除零合并。
10. **Skills 映射？** 18 条逐项审核完成：16 APPROVED_REFERENCE、1 COMPLEMENTARY、1 NEEDS_MORE_EVIDENCE；125 个 skill 文件未动。
11. **性能？** 10 万合成资源：查询 455-660ms、过滤 12.7ms、4 并发读安全、备份 30.5ms、integrity ok。瓶颈 LIKE 预筛，百万级建议 FTS5/PG（抽象已保留）。
12. **未解决的问题？**
    - api.github.com 对本机间歇 403/TLS EOF：Reader 已有 tarball 回落，但 sync（eri.py）主通道仍是 api meta+commit，若持续恶化需同样回落
    - 63 条 D5 待人工判读（规则只能证伪不能确证）
    - 8 条 A/B 级资源 License 待人工核读
    - SELECTED/APPLIED 漏斗需等首个真实研究任务才有数据

## 交付清单
eri_p2.py（Resolver/Reader/Tracker/审核队列/桥接/性能器）+ tests/run_p2_tests.py + 4 张新表
（resolver_queries/resource_usage/reader_cache/review_log）+ 8 份报告 + rtis.py 集成补丁
（备份 db/backups/rtis.py.pre-p2-20261105-230907.bak + 库备份 catalog-p2-start-20261105-230907.db）

## 变更边界确认
knowledge.db 零接触；81 条资源与全部 ID 不变；125 skills 未动；未 commit/push；无外部安装。
