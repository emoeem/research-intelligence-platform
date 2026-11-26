# MA-1 批次验收报告：Caller 身份与多密钥鉴权

- 日期：2026-11-22
- 任务书：[MA-1-TASK.md](MA-1-TASK.md)（§0 裁决 D1-D8 / §3 步骤 S1-S6 / §4 八判据 / §5 测试面 / §6 红线，全部照办）
- 状态：实施完成，已按任务书 §3 S6（显式 add → commit → push → CI watch）收口入库：main 三 commit——0b950345 收口 / 06d347f9 CI 修复批一 / 6233f127 CI 修复批二（HEAD=6233f127，2026-11-22 03:33+0800），工作树 `git status --porcelain` 干净（2026-11-22 复核实测）；本行原稿「工作树留待审查（未 commit / 未 push / 未 git add）」为收口前时点快照，随 CI 修复批登记修订失效
- 环境：`CARGO_TARGET_DIR=/home/emo/rt3-target`，Rust 全部 `--offline`，测试 `--test-threads=1`

## 一、S1 基线核验记录

| 项 | 结果 |
|---|---|
| Cargo.lock 包数 | `grep -c '^\[\[package\]\]' Cargo.lock` = 399 ✓ |
| git status | 干净（仅未跟踪 `docs/rip3/MA-1-TASK.md`）✓ |
| selftest_auth.sh 基线 | PASS=11 FAIL=0，exit 0（判据 2 的 before 基线） |
| selftest_phase6.sh 基线 | PASS=48 FAIL=1，exit 1——**既有红**（详见坏消息 1），非本批引入 |
| 扩展点行号实勘 | `_load_auth_token` rp_server.py:142-165 / `require_auth` :4255-4266 / `_finish_audit` :4296-4315 / `audit_api_access` :228-240——与任务书锚点 A2/A3/A6 无漂移 |
| migrate.rs 幂等行为 | 复读确认：user_version 跳过 + 执行器补写 + 单行 ALTER ADD COLUMN 守卫（migrate.rs:62-105） |

## 二、改动文件清单（三 commit 聚合 git diff --numstat 实测：0b950345^..6233f127，22 文件）

| 文件 | 行数 | 说明 |
|---|---|---|
| `migrations/026_caller.sql` | 新增 74 行 | caller + api_key 两表纯新增（零 ALTER）+ ix_api_key_caller + 尾部 schema_version 登记行 + user_version 双写；文件头「纯新增零 ALTER」纪律注释（025:5-6 同款） |
| `apps/server/rp_server.py` | +107 / -3 | 三扩展点：①`_load_auth_token` 后新增 `_load_caller_keys`（026 两表 mode=ro 直查内存映射 + 失败降级 legacy-only）与 `_resolve_caller_key`（sha256→主键直查→compare_digest 二道比对）；②`require_auth` 多 key 并存分支（legacy 分支逐字节不变，见四）；③`_finish_audit` 行尾追加 `caller` 三态字段 + 消费即清；头部注释两处同步（doc-only） |
| `crates/database/src/bin/rp-admin.rs` | +542 / -3 | 五子命令 caller-add/caller-list/key-gen/key-list/key-revoke（enum Mode 变体 + usage + 手写解析 + JSON 输出 + 退出码口径 0/2 沿用）+ `/dev/urandom` 32B + base64url 无填充（data-encoding BASE64URL_NOPAD 既有依赖）+ sha256 hex（sha2 既有依赖）+ `#[cfg(test)]` 5 单测 |
| `apps/server/selftest_auth.sh` | +128 / -0 | 尾部追加 MA-1 多 key 用例块（第 8 节）；**既有用例零改动**（numstat 0 删实证） |
| `crates/rp-mcp-parity/tests/caller_auth.rs` | +432 / -0 | 聚合（收口 421 + CI 修复批一 ensure_bin 化 19/8 净 +11，坏消息 10①）：Rust e2e（parity.rs:170-181 范式）：临时库 001..026 迁移 → rp-admin 预置双 caller 双 key → 真实 rp_server.py → 七项断言 + 判据 8 计时 |
| `crates/agent-engine/tests/migration_idempotent.rs` | +4 / -4 | 链尾陈旧断言顺带更新 25→26（坏消息 3） |
| `crates/database/tests/backup_restore.rs` | +3 / -3 | 同上（Some(25)→Some(26) + 头注） |
| `crates/database/tests/migrations_agent_data_plane.rs` | +6 / -6 | 同上（链尾名/uv 断言） |
| `crates/database/tests/migrations_context_broker.rs` | +7 / -6 | 链尾断言（收口 6/5）+ 批二 :158 断言消息文本对齐 026（+1/-1，消息级勘误断言零改动，坏消息 11②） |
| `crates/database/tests/migrations_runtime_node.rs` | +6 / -6 | 同上 |
| `crates/database/tests/migrations_tool_contract.rs` | +7 / -6 | 同上（链尾断言 6/5 + 批二消息文本对齐 1/1，坏消息 11②） |
| `crates/agent-engine/tests/migration_dev_copy_p7_3.rs` | +5 / -5 | 批一（06d347f9）链尾断言顺带更新（坏消息 3 增补）：dev-p7 副本 13..026 |
| `crates/plugin-runtime/tests/lifecycle_full_path.rs` | +4 / -4 | 批一：009 自钉重放计数 16→17 |
| `crates/workflow-engine/tests/migration_replay.rs` | +3 / -3 | 批一：链尾断言顺带更新 |
| `crates/wsw-message/src/store.rs` | +2 / -2 | 批一：src 内嵌迁移计数单测 |
| `crates/wsw-message/tests/cas_e2e.rs` | +15 / -5 | 批一 3/3 链尾 + 批二 12/2 dev-b1 副本 bring-up 自足化（坏消息 11①） |
| `crates/wsw-runtime/src/persist.rs` | +5 / -3 | 批一：`assert_watermark_25` 断言值 25→26（方法名沿用 025 批次命名先例不改） |
| `crates/wsw-runtime/tests/exchange_persist.rs` | +2 / -2 | 批一：链尾断言顺带更新 |
| `crates/wsw-runtime/tests/intercept_queue.rs` | +1 / -1 | 批一：链尾断言顺带更新 |
| `docs/README.md` | +1 / -0 | MA-1 报告索引行（仿 RT-3 行格式） |
| `docs/rip3/MA-1-BATCH-REPORT.md` | 新增（6233f127 时点聚合 +131） | 本报告；登记修订（含本表补行）发生在其后 commit，不并入该数 |
| `docs/rip3/MA-1-TASK.md` | +121 / -0 | 治理文档：任务书定稿随批入库（0b950345；§2.1 落点清单登记的是交付改动面，任务书自身不在清单内，随本报告一并登记） |

未触碰：`crates/tool-registry/`（红线 5）、既有 migrations 001-025（红线 6）、`Cargo.toml`/`Cargo.lock`（红线 1，git diff 空）、生产 `platform*.db`/`platform-dryrun.db`/`integrations/rtis/`（红线 7）、`selftest_phase6.sh`、`parity.rs` 既有用例。

## 三、§5 测试面逐条实测

| # | 命令 | exit | 输出摘要 |
|---|---|---|---|
| 1 | `cargo test --offline -p database -- --test-threads=1` | **0** | lib 11 ok + rp-admin 单测 5 ok + backup_restore 4 ok + migrations_context_broker 2 ok + migrations_runtime_node 2 ok + migrations_agent_data_plane 1 ok + migrations_tool_contract 2 ok（含 026 全链幂等 + 真实迁移链 001..026） |
| 2 | `cargo test --offline -p rp-mcp-parity -- --test-threads=1` | **0** | `caller_multi_key_auth_e2e` 1 ok（1.40s）+ parity 既有 5 ok（1.86s） |
| 3 | `cargo clippy -p database -p rp-mcp-parity --all-targets --offline -- -D warnings` | **0** | 首跑红 2 处（caller_auth.rs 头注 `>` 字符触发 clippy::doc_lazy_continuation）→ 注释改写后绿（过程如实记，坏消息 7 附） |
| 4 | `bash apps/server/selftest_auth.sh` | **0** | PASS=28 FAIL=0：既有 11 例全过（零改动）+ 追加 17 例（caller-add/key-gen 形状/警示/哈希交叉/key-list 不回明文/并存/双 key 200/未知 401/WWW-Authenticate/revoke 即时/revoke 隔离/重复 revoke 幂等/停用 401/审计三态） |
| 5 | `bash apps/server/selftest_phase6.sh` | 1 | PASS=49 FAIL=1；唯一 FAIL = 「rp_server.py 全文件无 SQL 写语句 (期望=clean 实际=found-sql-writes)」——与 S1 基线同款既有红（坏消息 1）；其余 49 项全过（零直连 SQL 写门禁语义保持） |
| 6 | `cargo deny check advisories` | **0** | `advisories ok` |
| 7 | `cargo deny check licenses` | **0** | `licenses ok` |
| 8 | `grep -c '^\[\[package\]\]' Cargo.lock` | **0** | `399`（红线 1 双点复核；git diff Cargo.lock 为空） |

判据 8 计时证据（`cargo test --offline -p rp-mcp-parity --test caller_auth -- --nocapture` 实测输出）：

```text
MA-1 e2e auth latency：multi p50=3.07ms p95=3.78ms；paired incremental p95=+1.68ms（40 对，判据 8）
```

以多 key 与 legacy 单令牌交替配对采样，验收 `paired incremental p95 < 5ms`。HTTP 往返耗时仅作为测量载体，不再错误地等同于 SHA-256 + SQLite 主键查询的增量成本。

## 四、八判据对照

| # | 判据 | 状态 | 实测 |
|---|---|---|---|
| 1 | 026 迁移纯新增幂等 | **PASS** | migration_idempotent（001..026 首跑、重跑 no-op、sqlite_master 快照零差异、user_version=26）exit 0；026 全文仅 CREATE TABLE/INDEX IF NOT EXISTS + INSERT OR IGNORE，零 ALTER（守卫零触发）。回滚 drop 语句见七 |
| 2 | 单 Token 兼容回归 | **PASS** | selftest_auth.sh 既有 11 例零改动全过（numstat 128/0）；legacy 命中审计记 `default`（selftest「legacy 单令牌与多 key 并存」+ e2e has("default",200) + 样例行）；require_auth legacy 分支 diff 仅增 docstring 与 `_audit_caller` 赋值，compare_digest/expected 构造/sleep/raise 逐字节不变 |
| 3 | 多 key 互不串扰 | **PASS** | 双 caller 双 key 各自 200；审计 JSONL caller 各归各（selftest a200/b200 + e2e has(caller-e2e-a/b,200)）；revoke A 后 A 401、B 仍 200。资源面行为隔离 = MA-2/027 兜底声明（任务书 §2.1 判据 3 原文照办） |
| 4 | 未授权 401 形状不变 | **PASS** | 未知 key / 吊销 key / caller 停用均 401 + `WWW-Authenticate: Bearer` + JSON `error.code=unauthorized`；节流 sleep 保持（样例行 duration_ms=300 实证 AUTH_FAIL_DELAY=0.3s） |
| 5 | 吊销即时生效 | **PASS** | key-revoke 后下一请求 401（selftest + e2e 双面，server 每请求 mode=ro 直查零缓存，D4）；caller-add 即建即用（selftest 预置后立即可用） |
| 6 | key 管理面完整 | **PASS** | 五子命令 JSON 输出 + 退出码口径（0 ok/help、2 参数与执行错误——沿用 rp-admin 既有形态，见坏消息 4）；key 明文 47 字符 `rp1_` 前缀仅 key-gen JSON `key` 字段一次 + stderr「仅此一次展示」警示（selftest 断言）；key-list 不回明文（selftest grep 反证 + 库中零明文）；重复 revoke exit 0 + already_revoked=true |
| 7 | 零直连 SQL 写 | **PASS**（门禁同态） | rp_server.py 新增面 grep `INSERT INTO|UPDATE (caller|api_key|plugin)|DELETE FROM` 零命中；caller/api_key 写入仅 rp-admin 五子命令；phase6 唯一红为基线既有欠账（坏消息 1），非本批引入 |
| 8 | 鉴权增量成本 | **PASS** | 多 key p95=3.78ms；相对 legacy 配对增量 p95=+1.68ms < 5ms（40 对；sha256 + 主键单行查询增量成本） |

## 五、红线八条复核

1. 零新依赖 ✓：Cargo.lock 399 恒定（diff 空）；sha2/data-encoding 用 database 既有依赖；随机源 /dev/urandom（std::fs::File）；Python 面 hashlib/hmac/sqlite3/secrets 全 stdlib。
2. 单 Token 兼容零破坏 ✓：既有 selftest_auth.sh 用例零改动（128 增 0 删）全过；legacy 路径逐字节不变（diff 实证）。
3. server 零直连 SQL 写 ✓：见判据 7。
4. 不建第二事实源 ✓：026 是 platform.db 同库纯新增两表，零分库。
5. caller 不进 Tool Registry ✓：crates/tool-registry 零 diff。
6. 历史迁移禁改 ✓：001-025 零 diff；026 纯新增零 ALTER；schema_version 登记行 + user_version=26 随尾。
7. 生产库零触碰 ✓：测试一律临时库（mktemp/temp_dir）；platform*.db / platform-dryrun.db / integrations/rtis 零 diff；phase6 playwright 顺带重写的 4 张 web/qa 截图已 `git checkout` 还原（坏消息 8）。
8. 边界纪律 ✓：per-caller 限流/并发/预算帽、TaskClaim、知识供体面、ResearchTask 归属列、授权 enforcement 零实现（D7/D8）。

## 六、坏消息单列

1. **phase6 门禁基线红（既有欠账，非本批）**：`selftest_phase6.sh` 第 10 节静态 grep `INSERT INTO|UPDATE plugin|DELETE FROM` 撞上 WSW-3 批次 W3-D7 授权的 rp_server.py Python 直写三处（本批后行号 3860/3882/4051：`INSERT INTO wsw_repeater_tab_version` / `INSERT INTO wsw_repeater_tab` ×2）。S1 基线即红（PASS=48 FAIL=1），本批零新增命中、零修复改动（selftest_phase6.sh 不在落点清单）。建议后续批收窄 grep 词表（如豁免 `wsw_` 前缀表）或把 W3-D7 直写面迁出。
2. **任务书内数字矛盾**：D2「明文 `rp1_<43 位 base64url>`（32 字节 /dev/urandom → base64url 无填充）」数学上恒为全文 47 字符；§3 S4 却写「key 明文 48 字符」。按裁决表 D2 执行（47 字符），单测与 e2e 断言 47；若需 48 需改为带填充（违 D2「无填充」）或 33 字节熵（违 D2「32 字节」）。
3. **链尾陈旧断言顺带更新（§2.1 落点外的最小必要改动）**：14 个测试/源内断言位硬编码迁移链尾。本地门禁先更 6 个（migration_idempotent.rs / backup_restore.rs / migrations_{context_broker,runtime_node,agent_data_plane,tool_contract}.rs）；CI 首跑 fail-fast 又暴露 8 个未跑到的掩盖面：plugin-runtime `lifecycle_full_path.rs`（009 自钉重放计数 16→17）、workflow-engine `migration_replay.rs`、wsw-runtime `intercept_queue.rs` / `exchange_persist.rs` / src `persist.rs` `assert_watermark_25`（断言值 25→26，方法名沿用 025 批次命名先例不改）、wsw-message src `store.rs` 单测、wsw-message `cas_e2e.rs`（:167/:180 副本迁移后链尾；:129 钉值后续批二已改写为 bring-up 构造性保障，见坏消息 11①）、agent-engine `migration_dev_copy_p7_3.rs`（dev-p7 副本 13..026）。均为全量共享 `migrations/` 直连面，026 落地即红；已做链尾 25→26（重放 16→17）+ 注释同步的最小更新，测试 fn 名沿用既有命名先例（intercept_queue `user_version_is_24` 断 25 同款）不改。该组测试是判据 1 的载体面，不更新则本批自破。
4. **退出码口径**：任务书 A9/D6 记「退出码 0/1/2」，rp-admin 既有实现实际只有 0（成功/help）/2（参数与执行错误），无 exit 1 路径。按「口径不变」沿用既有 0/2，未发明新语义。
5. **caller 停用无 CLI**：D6 定死五子命令，无 caller-revoke/caller-disable 运维入口；server join 判定已支持停用语义（判据 4/5 经测试面直写临时库验证），正式停用面建议 MA-2 登记 CLI 或 SQL 运维规程。
6. **加载面内存映射与 D4「每请求直查」的关系澄清**：`_load_caller_keys` 的 key_hash→caller 映射仅作 026 在位健康探针与降级开关（表缺失/查询失败 → warn + legacy-only，server 不拒起，§7 口径）；鉴权判定一律走 `_resolve_caller_key` 每请求 mode=ro 直查（判据 5 无缓存语义），映射内容不参与任何判定。
7. **parity.rs ensure_bin 与构建纪律冲突（环境项）**：parity 既有 e2e 期望 repo 默认 `target/debug/{rp-workflow,rp-mcp}` 在位，而本机纪律禁用默认 target。桥接：两二进制构建进 `/home/emo/rt3-target`，repo `target/debug/` 下放同名符号链接（gitignored 构建目录内，零默认 target 构建，parity.rs 零改动）。中途 clippy 红 2 处（caller_auth.rs 头注 `>` 触发 doc_lazy_continuation）已改写注释消除。
8. **phase6 运行产物**：本次 phase6 的 playwright 步骤顺带重写 `web/qa/plugins/e2e-*.png` 4 张截图（运行产物、内容随每次运行变）；已还原，不在交付 diff 内。
9. **中途测试面返工记录（如实）**：selftest 首跑 3 红（python `print(True)` 大写 / 停用 UPDATE 双连接未 commit / 401 行 caller 归属预期错——D5 语义下吊销 key 未过鉴权记 "-"，预期应为 "-"），均为测试侧错误，server 行为正确；caller_auth.rs 首版一处编译错（String 遮蔽后取 Value 键）修正后绿。最终两测试面全绿。
10. **CI 首跑红 2 处已修（run 44938566213，如实）**：①parity job `cargo test -p rp-mcp-parity` 单独先跑时 rp-admin 二进制缺位——本地门禁顺序（`cargo test -p database` 先行顺带建 bins）掩盖了该顺序依赖，CI 无此前置；caller_auth.rs `rp_admin_bin()` 改为候选全缺位时按 parity.rs ensure_bin:81-92 同范式进程内 `cargo build --offline -q -p database --bin rp-admin` 补建，头注「不做进程内构建防死锁」的顾虑与 rp-workflow/rp-mcp 同范式既有 CI 过门事实矛盾，头注一并改写。②链尾陈旧断言 fail-fast 掩盖面 8 处，见坏消息 3 增补。
11. **CI 修复批二（6233f127）全批补登（如实；报告此前仅登记至批一）**：①`cas_e2e.rs` `dev_b1_copy_applies_migration_023` precondition 钉「dev-b1 副本恰在链尾 25」在 026 落地后必红（CI run 44940771882，cas_e2e.rs:128 panic；根因：本地 gitignored 副本停在 25，CI 测试二进制并行时序同样可能读到重建前状态）——钉值断言改为构造性保障：副本 `user_version < 26` 时先 `migrate()` 幂等迁至链头再断言 26（+12/-2；023..026 建表全 IF NOT EXISTS、登记行全 INSERT OR IGNORE，回放安全；只写副本，源文件零写入仍由 sha256 前后比对承载）。该改动超出坏消息 3「纯计数断言顺带更新」性质（新增 bring-up 逻辑面），故单独登记；坏消息 3 原「:129 钉值不动」条款随之失效，以本条为准。②`migrations_context_broker.rs` / `migrations_tool_contract.rs` 各 +1/-1：断言值本已是 26，仅消息文本残留 025 字样，对齐（消息级勘误，断言零改动）。本地复验（rt3-target --offline）：wsw-message 3/3 ok、database 测试套件维持绿。

## 七、回滚登记（任务书 §7 落地）

- 整批：已按 S6 入库为三 commit（0b950345 / 06d347f9 / 6233f127）——整批撤销 = `git revert 6233f127 06d347f9 0b950345`（任务书 §7 revert 口径不变；任务书「单 commit 交付」实际为三 commit，如实登记）。原工作树期「`git checkout -- <文件>` + 删除两个新文件」指引随入库失效，仅存档备查。
- 026 对已迁移库残留两空表（无害）；彻底清除：
  ```sql
  DROP INDEX IF EXISTS ix_api_key_caller;
  DROP TABLE IF EXISTS api_key;
  DROP TABLE IF EXISTS caller;
  DELETE FROM schema_version WHERE version = 26;
  PRAGMA user_version = 25; -- 仅当该库此前无 026 之后迁移时
  ```
- rp_server.py 三扩展点独立可逆：`CALLER_KEYS` 加载面失败自动降级 legacy-only（server 不因 026 缺表拒起）；revert rp_server.py 即回纯 legacy。
- rp-admin 新子命令纯新增，revert 后零残留。

## 八、MA-2 前置输出（任务书 §8.4）

### 8.1 per-caller 审计行形状样例（JSONL，RP_API_AUDIT_LOG 实捕）

```json
{"ts": "2026-11-21T18:15:23Z", "event": "api_access", "method": "GET", "path": "/api/stats", "remote": "127.0.0.1", "status": 500, "duration_ms": 0, "trace_id": null, "correlation_id": null, "rate_limited": 0, "auth_enabled": 1, "caller": "default"}
{"ts": "2026-11-21T18:15:23Z", "event": "api_access", "method": "GET", "path": "/api/stats", "remote": "127.0.0.1", "status": 500, "duration_ms": 0, "trace_id": null, "correlation_id": null, "rate_limited": 0, "auth_enabled": 1, "caller": "caller-sample-01"}
{"ts": "2026-11-21T18:15:23Z", "event": "api_access", "method": "GET", "path": "/api/stats", "remote": "127.0.0.1", "status": 401, "duration_ms": 300, "trace_id": null, "correlation_id": null, "rate_limited": 0, "auth_enabled": 1, "caller": "-"}
```

要点（MA-2 预算/限流面直接消费）：
- `caller` 三态：`"default"`（legacy 单令牌）/ `caller_id`（多 key 过鉴权）/ `"-"`（匿名未过鉴权——含 401/429 前置失败与鉴权关闭模式）；
- 身份只出现在**过鉴权**行；401 行恒为 `"-"`（吊销 key 无法事后归属属设计内，D5）；
- 行形状 = 019 external_api_audit 前缀列序不变 + 尾部追加 `caller`（装载侧兼容；019 表零 ALTER）；
- 聚合键建议：`(caller, ts)` 直接可作 per-caller 速率/预算记账键；`status` 判 5xx 排除。

### 8.2 caller 表权限位语义备忘（MA-3 节点角色绑定输入）

- `caller.permission_level`：`T0|T1|T2` 词表 CHECK 冻结（001:289 Tool Policy 词表同源）；本批是**登记属性**（D8），写审计行对齐词表，零授权判断（写端点授权仍是既有 Tool Policy 语义，TOOL-CONTRACT 未动）；
- caller 停用 = `caller.revoked_at` 非空 → 名下 key 全失效（server 校验 join 判定，key 停用与否无关）；key 停用 = `api_key.revoked_at` 非空；
- MA-3 绑定建议：节点角色 ←→ `caller_id` + `permission_level` 映射表可挂 026 两表之上（同库新增，禁 ALTER）；enforcement 消费点建议落 rp_server 写端点 require_auth 后的独立判定层，不动本批校验面；
- MA-2 待补登记：caller 级停用 CLI（坏消息 5）、`api_key.last_used_at` 消费面（026 已留列，本批零写入方）。
