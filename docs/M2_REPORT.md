# M2 交付报告：Rust Core Foundation（M2-A/B/C）

日期：2026-11-06　执行：RIP M2 实施会话（后台 agent）　范围：research-platform/ 下新增
上游依据：SESSION-HANDOFF.md（第三节序列）、MODULE_BOUNDARIES.md §8.1/§8.2/§8.4、UNIFIED_DOMAIN_MODEL.md §7.1/§7.2、PROBE-RESULT.md（DEC-001 已验证）

## 一、交付清单

| 交付物 | 路径 | 说明 |
|---|---|---|
| Cargo workspace | `Cargo.toml`（根，resolver=2）+ `Cargo.lock`（301 行） | 成员：crates/domain、crates/database、crates/audit-engine、crates/resource-engine、tests |
| domain crate | `crates/domain/`（5 模块，10 单测） | Resource 实体（M2 收编字段集）、LifecycleStatus/GovernanceStatus 双轴枚举（K-08/K-16 口径）、QualityGrade、ResourceId 新类型（gh:/url: 校验）、AuditEvent 纯数据类型、DomainError、纯 std UTC 时间戳。零外部依赖（MODULE_BOUNDARIES §8.1"自身零依赖"） |
| database crate | `crates/database/`（6 单测） | rusqlite 连接池（Mutex+Condvar，with_conn/with_tx，内存库单连接，只读/可写开关）、迁移执行器（NNN_name.sql 版本序、user_version 幂等、重复版本号拒绝）。重导出 rusqlite，下游不直依赖 |
| audit-engine crate | `crates/audit-engine/`（4 单测） | AuditEvent -> audit_event 表 Emit、Trace(correlation_id) 回放、旁路缓冲（§8.2.4：有界 1024 丢最旧，emit_best_effort 永不向调用方报错） |
| resource-engine crate | `crates/resource-engine/`（8 单测 + rp-resolve CLI） | ResourceRepository trait（get/count/resolve）+ SQLite 实现，SQL LIKE 预筛（治理轴仅 ACTIVE 可见）+ 词元级打分 + 排序截断，resolve 输出与 eri_p2.resolve 字段级兼容（17 键/命中、8 键/顶层） |
| ERI 对照测试 | `tests/`（rp-eri-parity 包：src/lib.rs 载具 + tests/eri_parity.rs 3 组对照 + tests/m2_integration.rs 3 项集成） | subprocess 调 `python3 -I eri_p2.py resolve --json`（生产 catalog 的 sqlite backup 副本上跑，生产库零写入，已核 md5/mtime 不变）；Rust 侧 in-process resolve 只读 platform-dryrun.db |
| vendor + 离线配置 | `vendor/`（43MB，25 crate）+ `.cargo/config.toml`（vendored-sources） | `cargo build --offline` 全量验证通过（详见 §2） |
| 容器 | `Containerfile` + `.containerignore` | 多阶段：debian:bookworm-slim + rustup(rsproxy.cn) 工具链 1.99.1 -> builder（fmt/clippy/test/build 全 --offline）-> runtime（80.7MB，仅 rp-resolve 产物） |
| 基础镜像决策 | Containerfile 头注 | rust:1.99 直连与 daocloud/1ms 镜像源均在大层 blob 停滞（实测 0 KB/s），改用 debian:bookworm-slim + rsproxy rustup 自建工具链（与 cargo 依赖同源），工具链版本与宿主机一致（1.99.1 + clippy 0.1.99 + rustfmt 1.10.0） |
| rp-resolve CLI | `crates/resource-engine/src/bin/rp-resolve.rs` | --db/--migrate-dir/--keywords/--limit/--task-id/--resource-type/--writable/--json；容器冒烟入口；:memory: + --migrate-dir 支持全内存演示 |

合计 22 个 Rust 源文件 2889 行。未 git commit（按纪律留给协调会话统一推送）。

## 二、质量门（全部 verified）

| 门 | 命令 | 结果 |
|---|---|---|
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 error 0 warning（verified） |
| fmt | `cargo fmt --all` + `-- --check` | 通过（verified） |
| test（宿主机） | `cargo test --offline --workspace` | 34 通过 0 失败：单测 28（domain 10 / database 6 / audit-engine 4 / resource-engine 8）+ eri_parity 3 + m2_integration 3（verified） |
| test（容器内） | builder RUN `cargo test --offline --workspace --exclude rp-eri-parity` + `-p rp-eri-parity --test m2_integration` | 31 通过 0 失败（28 单测 + 3 集成；eri_parity 需 python3 与 eri catalog，属宿主机测试）（verified） |
| 离线构建 | `cargo clean && cargo build --offline --workspace`（vendor + .cargo/config.toml） | 7.91s 成功（verified） |
| 依赖面 | workspace 依赖 | rusqlite 0.32（bundled）+ serde 1 + serde_json 1，无其他外部 crate（verified，见 Cargo.lock） |

## 三、podman 构建（verified）

命令：`podman build -t rp-core .`（podman 6.1.4 rootless），exit=0。

阶段与层：
1. toolchain：debian:bookworm-slim + apt（gcc/libc6-dev/curl）+ rustup 1.99.1 via rsproxy.cn，输出工具链版本四连（cargo 1.99.1 / rustc 1.99.1 / clippy 0.1.99 / rustfmt 1.10.0）。
2. builder：vendor 全量 COPY，fmt --check -> clippy -D warnings -> cargo test（容器内 31 通过）-> cargo build --release --bin rp-resolve，全程 --offline（CARGO_NET_OFFLINE=true）。
3. runtime：debian:bookworm-slim + rp-resolve 静态产物（bundled sqlite，运行时零网络零证书需求），镜像 80.7MB，tag `rp-core:latest`（ID 09f20c9e0e1d）。

清理：两个中间 <none> 镜像已 rmi；alpine（他人既有镜像）未动；无遗留容器。

容器内冒烟（`podman run --rm -v $(pwd)/migrations:/db:ro rp-core`）实测输出（节选，verified）：

```
query_id: erq-20261105190420341808-84db18
status: ok | stats: {candidates: 2, filtered: 0, returned: 2} | weights: {keyword: 0.6, quality: 0.4}
  0.88  gh:xairy/linux-kernel-exploitation  q=70.0 grade=B
  0.852 gh:secfigo/awesome-fuzzing          q=63.0 grade=C
```

## 四、ERI 行为对照测试（M2-B，verified）

方法：3 组关键词（fuzzing / payload,bypass / android），每组双轨：eri_p2.py resolve（--limit 5 --json，跑在 production catalog.db 的 sqlite backup 快照副本上，ERI_DB 注入）vs Rust resolve（in-process，只读 platform-dryrun.db）。断言：双方命中非空、eri 命中元素含 §8.4 必备 8 字段、Rust 序列化输出含同 8 字段、top-5 集合交集非空（硬断言）。top-1 一致性按任务口径仅记录不作为硬断言（权重不同允许顺序差异）。

| 组 | eri top-1 | rust top-1 | top-1 一致 | top-5 交集 | eri 命中数 | rust 命中数 |
|---|---|---|---|---|---|---|
| fuzzing | gh:enablesecurity/awesome-rtc-hacking | gh:xairy/linux-kernel-exploitation | 否 | 2/5（linux-kernel-exploitation、awesome-fuzzing） | 4 | 2 |
| payload,bypass | gh:swisskyrepo/payloadsallthethings | 同左 | 是 | 3/5（集合完全一致） | 3 | 3 |
| android | gh:ashishb/android-security-awesome | 同左 | 是 | 1/5（rust 全命中为 eri 子集） | 2 | 1 |

结论：3/3 组核心命中集合重叠（硬断言全过）；top-1 一致率 2/3。不一致组（fuzzing）差异根源清楚：eri 的匹配 blob 含 github_metadata.topics/language 两列，且 category/tech 两轴依赖 platform 库不存在的表，故 eri 多召回 2 条（awesome-rtc-hacking、rfsec-toolkit 经 topics/category 命中）。逐组明细 JSON 已留 target/parity-*.json（构建产物，不入库）。

对照过程安全性：production catalog.db md5 与 mtime 前后不变（eri resolve 的 resolver_queries/resource_usage 写入只落在副本）；platform-dryrun.db 全程只读打开，无 WAL 产生。

## 五、与 ERI 的差异表（迁移期双轨已知差异）

| # | 项 | eri_p2.resolve | M2 resource-engine | 影响与处置 |
|---|---|---|---|---|
| 1 | 权重 | 六轴（keyword .25/category .20/tech .15/profile .15/relation .15/quality .10） | 简化两轴（keyword .60/quality .40），weights.rs 注释标明 P2 六轴待 M4 迁移 | top-1 可能不同（fuzzing 组实测不同）；归一规则（缺证轴剔除后归一）与 eri 一致 |
| 2 | 匹配 blob | name+description+topics+language | name+description（platform 库无 topics/language 列）；canonical_url 仅参与 SQL LIKE 预筛，不参与词元命中（eri 同口径） | rust 候选集为 eri 子集（fuzzing 组 2 vs 5 候选） |
| 3 | category/tech/relation 证据 | resource_categories / github_metadata.topics / skill_mappings | platform 库无这三张表，恒空 | 输出字段保留（category_matches/skill_relationships/workflow_relationships 空数组），字段级形状不变 |
| 4 | license/maintenance | github_metadata 实值 | UNKNOWN（platform 库无 license 元数据列），license_note 与 eri 缺证分支同文案；adoptable_as_dependency 恒 false | 消费方需按 UNKNOWN 语义处理；license 轴随 M1 第二批字段映射补齐后收编 |
| 5 | relevance_reasons 文本 | python list repr（`['a', 'b']`） | `[a, b]` 无引号 | 展示差异，语义同（keyword hits 截前 5、quality 分数与等级、no evidence for 缺证提示文案一致） |
| 6 | query_id | sha1(输入)[:6] | FNV-1a 64bit[:6]，形状同（erq-20位时间戳-6hex） | 值不可比，形状兼容 |
| 7 | 使用事件 | resolve 写 resolver_queries + resource_usage 到 catalog | 不写（platform-dryrun.db 只读切片） | 使用侧随 M3+ 审计事件流与可写库落地 |
| 8 | 输入维度 | keywords/technologies/site_profile/skill_ids/workflow_ids | keywords/task_id/filters(resource_type/lifecycle/governance)/limit | profile/skill/workflow 维度随 search-engine/workflow-engine 接口面落地（eri 侧这三输入为空时行为与 rust 可比） |
| 9 | ALIASES | 确定性别名扩展 | 同表收编（tokenizer.rs ALIASES 常量，注明同步来源） | 行为一致（android -> apk,mobile 实测两侧同命中） |
| 10 | CJK_BRIDGE | site_profile 中文特征查表 | 未收编（M2 无 site_profile 输入面） | 随 #8 一并落地 |
| 11 | 排序与舍入 | (-score, -quality, resource_id)，4 位小数 | 同 | 一致 |

## 六、未完成项与边界（如实标注）

verified：本报告 §2/§3/§4 所列全部结果均实测执行，非推断。

unverified / 未实现（后续里程碑）：
1. Resolve 的 TaskContext 完整形状（site_profile/skill/workflow 维度）：未实现，见差异表 #8。
2. 六轴权重与 category/tech/relation 证据接入：待 M4 quality-engine / search-engine。
3. license/maintenance 字段：platform 库无数据源，恒 UNKNOWN（待 M1 第二批映射后收编）。
4. 审计冷存归档、Trace 的 sensitivity 过滤、跨进程审计持久队列：emit 失败仅进程内有界缓冲（§8.2.4 的最小实现）。
5. 连接池故障连接回收（锁中毒时池退化为小一号池，正确性不受影响）：简化实现，注释已标。
6. tokio/axum：按任务口径未引入（纯同步 rusqlite 足够，商业化迭代再加）。
7. CI：容器构建未接 CI（工作流属协调会话统一安排）。

边界遵守：
- 未 git commit/push；未改 migrations/ 既有文件（platform-dryrun.db 只读打开，md5 未变）；未碰 workers/、apps/（本仓库内 workers/ 与 schemas/ 为并行会话产出，未触碰）；未改 eri 生产代码（eri_p2.py 以子进程 + 生产库副本运行）。
- git status 中 docs/M3_REPORT.md、schemas/、workers/ 为并行 agent 产出，与本批无关。
