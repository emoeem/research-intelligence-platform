# RIP3 批次报告：P3-3（D4 语义候选 + 向量面 + rip-embed 容器族，§17.6 格式）

## A 完成内容

1. **migrations_pg V0005**：knowledge_card_vectors（card_id PK/FK + embedding
   vector(512) + embedding_model_id/version/dimension 元数据三件 + index_version
   重建追踪 + HNSW vector_cosine_ops 索引；03 号冻结物 1.7 同构卡片级，chunk
   级向量随 Phase 4）。
2. **rip-embed 容器族**（containerfiles/rip-embed/）：基座同各族 digest +
   onnxruntime==1.32.0 / tokenizers==0.23.3 / numpy==2.5.4 钉版 +
   **bge-small-zh-v1.5 模型构建期烘焙** /opt/models（gitignore 运行时数据，
   受控环境构建，镜像属运行时数据 ADR §3.2）；**M8 embedding_worker 真实后端
   类零复制导入**（BgeOnnxModel.embed，容器内保持 workers/python/ 相对布局
   ——模块级 _REPO_ROOT parents[2] 依赖实录）；embed_tools.py op=embed（D-TC-03
   帧 + stdout 守卫 + 64 条/批 + 单条 64KiB 帽）；**无回退面**（模型缺失即
   -32006 显式报错，治理语义向量不静默降 hash——与 M8 worker 的 hash-fallback
   语义差异如实登记）；构建期 dim 512/L2 自检；产物 digest
   `sha256:013dc02b…`（0.1.0）台账三处同步。
3. **intel-db embed_worker**：embed_cards（缺向量卡片批量补齐，64 条/批，upsert
   幂等，V0005）+ semantic_scan（每卡 top-K 余弦近邻 → NEAR_DUPLICATE 候选边，
   relation_source = gov_worker.semantic:0.1.0 与 D3 词法面区分，**禁自动合并**
   R2 5.2 D4 硬性）。
4. **rp-kc embed-cards / semantic-scan** 两子命令（+ rp-kc 启动迁移引导面：
   refinery 幂等，V0004/V0005 治理表面随启动就绪）。
5. **集成测试**（tests/gov_semantic.rs，1/1 真容器 + 真 PG 实跑 1.6s）：同义
   改写对召回（词法面抓不到的语义近对）+ 嵌入幂等 + 候选边不合并卡片本体 +
   二跑幂等。
6. **全链实跑**：embed-cards `{embedded:4}` → 幂等 `{embedded:2, skipped:4}` →
   semantic-scan 阈值 0.83 → `{pairs:6, relations_created:1}`，新边
   semantic source confidence 0.8358 rule="D4 semantic cosine"（同义改写对，
   D3 minhash 面零召回——独立判别力实证）；无关句 cosine 0.325 判别正常。

## B 架构变化

- 容器族第三族（rip-embed），治理向量面成立（卡片级；chunk 级随 Phase 4）；
  D7 本地推理落地（无外部 API，模型烘焙）。
- 坑位实录（已修）：容器内模块相对布局依赖（parents[2]）；CTE 内未类型化
  参数被 PG 推断 text（向量参数须显式 ::vector 或按列取）；LIMIT 参数 PG
  推断 Int8（Rust i32 需转 i64）；rp-kc 启动迁移引导缺失。

## C 文件变化

新增：migrations_pg/V0005__card_vectors.sql；containerfiles/rip-embed/
（Containerfile + embed_tools.py + rip.embed.text.yaml/json）；
crates/intel-db/src/embed_worker.rs；crates/intel-db/tests/gov_semantic.rs。
修改：containerfiles/README.md（台账两处）；crates/intel-db/src/lib.rs
（embed_worker mod）；crates/intel-db/src/bin/rp-kc.rs（两子命令 + 迁移引导 +
RP_DEBUG 错误链）；apps/server/selftest_phase1_pg.sh（钉值 5）。

## D 数据库变化

PG：V0005（knowledge_card_vectors + HNSW 索引）；SQLite 零迁移。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| 集成测试 | RP_TEST_PG_DSN=… --test gov_semantic | 1 passed（真容器 1.6s：改写对召回/嵌入幂等/不合并断言） |
| 容器自检 | 构建期 dim 512 + L2 归一化断言 | 过 |
| 容器冒烟 | embed op 两句 | 无关句 cosine 0.325 判别正常 |
| 全链实跑 | rp-kc embed-cards + semantic-scan | embedded=4 → 幂等 skipped=4 → 0.83 阈值新建 1 边（0.8358） |
| 回归 | intel-db lib 24 / domain 62+7 / clippy -D warnings | 全绿 |
| selftest | selftest_phase1_pg.sh（钉值 5） | 16/16 |

## F 性能结果

三卡嵌入 1.6s（含容器冷启动 + ONNX CPU 推理逐条 batch=1，M8 语义「同输入同
输出」保持）；规模数字属 Phase 8。

## E2 CI 结论

push 后 GitHub CI 全绿为硬门槛（随 push 回填，同前批口径）。

## G 遗留问题

1. 审核工作流（018 Human 复用 + 批量审核面）→ P3-4 批。
2. D4 阈值校准（0.83-0.92 区间实测数据点已落库，Phase 4 Benchmark 面
   Recall@K 校准定值）。
3. evidence_level 导入侧接线随 P3-4/收口批。
4. 向量批处理（>64 条分批已实现，>5000 卡分页）随 Phase 8。

## H 下一阶段依赖与顺序

P3-4（审核工作流 + 批量审核）→ Phase 3 收口（selftest_phase3 + 五道门禁 +
RELEASE + CI 硬门槛）。
