# RIP3 批次报告：P3-2（D3 近似重复容器 worker + 治理编排面，§17.6 格式）

## A 完成内容

1. **rip-gov 容器族**（containerfiles/rip-gov/）：基座 python:3.12-slim 同各族
   digest + datasketch==2.0.0（ADR-0002 台账 2.0 落地，numpy/scipy 传递）；
   gov_tools.py minhash op（D-TC-03 帧协议 + stdout 守卫）：char-bigram ∪ 词元
   shingling（中英混排覆盖）、MinHashLSH 候选对 + jaccard 估计、条目 5000/
   单条 64KiB 帽、**只产候选不判定合并**（R2 5.2 硬性）；network=none 纯本地
   计算；产物 digest `sha256:cb043e45…`（0.1.0）manifest yaml/json + README
   台账三处同步。
2. **intel-db gov_worker**（crates/intel-db/src/gov_worker.rs）：治理编排面
   （与 parser.rs 同链不复刻隔离逻辑：tool-engine submit → Gateway::dispatch
   → execute_container_via_engine；ops 运行库专用 enable_wal）；minhash_scan：
   库内非 REJECTED 卡片 → 容器批量扫描 → NEAR_DUPLICATE 候选边（jaccard 随
   relation_evidence 落库、confidence=估计值、幂等 UNIQUE；FK 失败如实计
   skipped 不静默）。
3. **rp-kc minhash-scan** 子命令（--workspace/--threshold/--num-perm/
   --run-db/--manifest-dir）。
4. **集成测试**（tests/gov_dedup.rs，1/1 真容器 + 真 PG 实跑 3.5s）：近似文本
   两卡召回（pairs≥1）+ 无关文本不误报 + 幂等二跑零新建 + jaccard/confidence
   落库断言。
5. **全链实跑**：登记 rip.gov.minhash（0.1.0）→ 造两近似卡 → minhash-scan
   → `{scanned:4, pairs:2, relations_created:2}`；库内 NEAR_DUPLICATE 边
   confidence 0.875/1.0、SAME_CONTENT（P3-1）0.95 并列在册。
6. D1 复用确认（任务书项）：文件级 sha256 去重 = P1 content_objects 面
   （同文件多来源单对象集成测试已覆盖，本批零改动）。

## B 架构变化

- 治理 worker 编排面成立（gov_worker），与 parser 面同模式独立 crate 内模块；
  容器族从一族（rip-parser）扩展为两族（+rip-gov），拆分原则符合（异构依赖
  分族：解析 ML 栈 vs 治理 datasketch 栈）。

## C 文件变化

新增：containerfiles/rip-gov/（Containerfile + gov_tools.py +
rip.gov.minhash.yaml/json）；crates/intel-db/src/gov_worker.rs；
crates/intel-db/tests/gov_dedup.rs。
修改：containerfiles/README.md（台账两处）；crates/intel-db/src/lib.rs
（gov_worker mod）；crates/intel-db/src/bin/rp-kc.rs（minhash-scan）。

## D 数据库变化

零迁移（D3 候选边落既有 knowledge_relations 十型面，V0004 已建）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| 集成测试 | RP_TEST_PG_DSN=… --test gov_dedup | 1 passed（真容器 3.5s：召回 + 幂等 + evidence 断言） |
| 容器冒烟 | minhash op（同文对 + 异题对） | 同文对 jaccard=1.0 召回，异题对零误报 |
| 全链实跑 | rp-tool register + rp-kc minhash-scan | scanned=4 pairs=2 created=2，库内边核验 |
| 回归 | intel-db lib 24 + domain 62+7 | 全绿 |

## F 性能结果

3 卡扫描 3.5s 含容器冷启动（datasketch 镜像 ~250MB 级）；规模数字属 Phase 8。

## E2 CI 结论

push 后 CI run 43714172411 首跑 failure：mcp-server run_tool_without_token_
fails_closed_before_serving 一例红（与 P3-2 改动面零交集）。本地静场串行复跑
7/7 全过（0.23s）——负载 flake（交接文档第五节既有口径：并发负载时序敏感断
言）。gh run rerun 因 integration token 权限缺失不可用（如实登记），按仓先例
以本 flake 记录提交触发同树重跑。

## G 遗留问题

1. D4 语义候选（pgvector 卡片向量 + bge embedding 容器化）→ P3-3 批。
2. 审核工作流（018 Human 复用 + 批量审核面）→ P3-4 批。
3. evidence_level 导入侧接线（导入面产卡携带 K 分级）→ 随 P3-3 批。
4. minhash 扫描分页（>5000 卡分批）随 Phase 8 规模批。

## H 下一阶段依赖与顺序

P3-3（D4 语义候选 + 向量面）→ P3-4（审核工作流 + 批量审核）→ Phase 3 收口。
