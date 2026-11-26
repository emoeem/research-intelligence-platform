# MA-1 任务书：Caller 身份与多密钥鉴权（SQLite 026 caller/api_key 纯新增 + rp_server 多 key→caller 映射兼容单 Token + per-caller 访问审计 + rp-admin key 管理面）

- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实现批次按本任务书执行）
- 依据：[MULTI-AGENT-ACCESS-PLAN.md](MULTI-AGENT-ACCESS-PLAN.md) §三 设计原则（caller ≠ 用户账号；不建第二套事实源；行级 caller 维度）、§四 MA-1 行、§六 禁止事项；侦察锚点 A1-A13 为 2026-11-22 本会话实测
- 设计裁决：MA-0 规划 §三/§六 已定原则；本批特有裁决见 §0（不另立 ADR 文件——设计增量小且全部可溯源至规划文档，如有分歧以规划文档为准并回改本档）

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **身份两表**：`caller`（caller_id PK / display_name / permission_level T0-T2 词表 / created_at / revoked_at）+ `api_key`（key_hash PK / caller_id FK / label / created_at / last_used_at / revoked_at）；caller 停用 = revoked_at 非空，其 key 全部失效（校验时 join 判定） | 计划 §四 MA-1 行「caller 表 + key 哈希 + 权限位」；权限位挂 caller 级（key 是凭据不是权限主体） |
| D2 | **key 格式**：明文 `rp1_<43 位 base64url>`（32 字节 /dev/urandom → base64url 无填充）；库存仅 sha256 hex（`key_hash = sha256(明文全文)`，无 salt——明文熵 256bit 唯一性足够，字典攻击面不存在）；校验 = sha256(请求 key) 后按主键单行查询 + `hmac.compare_digest` 二道比对 | 零 salt 免查全表；主键查询 O(1)；constant-time 双保险对齐既有 require_auth 口径 |
| D3 | **单 Token 兼容**：既有 RP_AUTH_TOKEN / RP_AUTH_TOKEN_FILE 语义零变化，映射为隐式 caller `default`（display_name="legacy-single-token"，库中无行、不可吊销、审计 caller 记 `default`）；与多 key 并存，任一命中即通过 | 计划 §四「兼容既有单 Token」；零配置向后兼容（验收判据 2 = 既有 selftest_auth.sh 零改动通过） |
| D4 | **server 读 key = 直查只读**：`RP_DB_PATH`（platform.db）同库 026 新增两表；server 校验路径对 api_key/caller 零写、mode=ro 连接复用既有读面惯例；写入一律走 rp-admin CLI | selftest_phase6.sh 零直连 SQL 写门禁（侦察 A5）；吊销即时生效（无缓存，每请求直查——本地 SQLite 单行主键查询成本可忽略） |
| D5 | **审计扩展**：`audit_api_access` JSONL 行新增 `caller` 字段（default / caller_id / "-"=匿名未过鉴权请求）；`external_api_audit` 表（019）不加列（该表由 Rust 面写入，列形状不动）；audit_event.actor 既有语义不动 | 侦察 A3：JSONL 行形状是 per-caller 审计天然扩展点；零 ALTER 需求 |
| D6 | **CLI 落 rp-admin**（crates/database/src/bin/rp-admin.rs）新增子命令：`caller-add` / `caller-list` / `key-gen` / `key-list` / `key-revoke`；key 明文仅在 key-gen 输出一次（JSON `key` 字段 + stderr 警示「仅此一次展示」）；哈希用 database crate 既有 `sha2 = "0.10"` 依赖（Cargo.toml:21 注释：sha2 已在 vendor），随机源 `/dev/urandom` 32 字节（std 零新依赖）；退出码 0/1/2 口径不变 | 侦察 A4：rp-admin 是 database crate 运维 CLI 投影、手写解析形态参照；Python 侧 hashlib/secrets 全 stdlib；Cargo.lock 399 恒定红线 |
| D7 | **per-caller 限流/并发帽不做**：RP_RATE_LIMIT_RPM 全局限频保持原样；per-caller 并发/速率/预算帽 = MA-2（计划 §四），本批零实现 | 边界纪律：MA-1 只做身份/鉴权/审计三件 |
| D8 | **权限位语义**：caller.permission_level 是**登记属性**（写审计行对齐 audit_event.permission_level 词表），本批不接授权判断（写端点授权面 = 既有 Tool Policy T0/T1/T2 语义，TOOL-CONTRACT 不动）；enforcement 留 MA-2/MA-3 按需消费 | 侦察 A1：写端点鉴权是二元的（token 对/403 关闭）；本批不改变授权行为，只补身份维度 |

## 1. 现状锚点（2026-11-22 侦察实测，A1-A13）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 server 形态 | rp_server 不是 Rust crate——Python 单文件 `apps/server/rp_server.py`（4621 行，stdlib ThreadingHTTPServer）；Rust 侧无 server crate | rp_server.py:69-70；Cargo.toml members 24 crate 清点 |
| A2 令牌加载 | `_load_auth_token`：RP_AUTH_TOKEN > RP_AUTH_TOKEN_FILE > 仓库根 `.rp-workbench-token`；整文件 strip = **单明文令牌**（多行不支持）；权限非 600 仅告警 | rp_server.py:142-165 |
| A3 校验面 | `require_auth`：`Authorization: Bearer` vs `f"Bearer {AUTH_TOKEN}"` hmac.compare_digest（明文常时比较）；失败 `time.sleep(0.3)`（AUTH_FAIL_DELAY:141）后 401 | rp_server.py:4255-4266 |
| A4 401 形状 | JSON `{"error":{"code":"unauthorized","message":…}}` + `WWW-Authenticate: Bearer`；429 另带 Retry-After；鉴权关闭时写端点 403 `auth_required_for_write` | rp_server.py:4244-4253,4405-4545 |
| A5 强制面与豁免 | do_GET 所有 `/api/*` 统一 `check_rate_limit()+require_auth()`（:4318-4323）；写端点逐一 require_auth；静态页豁免（login.html 无令牌可访问，selftest_auth.sh #4 断言）；写面一律 subprocess 委派（零直连 SQL 写门禁 selftest_phase6.sh） | rp_server.py:4318-4323,4268 |
| A6 访问审计 | `_finish_audit`/`audit_api_access` JSONL（RP_API_AUDIT_LOG）：行形状对齐 019 `external_api_audit`（trace_id/correlation_id/auth_enabled），**无调用者身份** | rp_server.py:4297-4315；migrations/019 |
| A7 迁移链头 | 最新 `025_wsw_exchange.sql`（023_body_blob/024_wsw_intercept 在位）→ **026 空位**；`discover_migrations` 强制 NNN_name.sql + user_version 幂等；单行 ALTER 守卫（migrate.rs:62-105）；024/025 惯例：纯新增零 ALTER + 尾部 schema_version 登记行 | migrations/025；crates/database/src/migrate.rs:33-105 |
| A8 审计表现状 | `audit_event` 有 actor（自由字符串，非身份原语）+ permission_level，**无 caller**；`tool_run` 无 caller/owner（归属列 = MA-2 027，本批不碰）；015 给五表加 trace_id + v_trace_chain 视图 | migrations/001:80-95,005:34-60,015 |
| A9 CLI 形态 | rp-admin（crates/database/src/bin/rp-admin.rs:1-60）：enum Mode + usage() + 退出码 0/1/2 + JSON 输出；全仓零 clap | rp-admin.rs:1-60 |
| A10 测试形态 | `apps/server/selftest_auth.sh`：RP_PORT+RP_AUTH_TOKEN 起 server，curl 断言 401/WWW-Authenticate/200/静态豁免/405/鉴权关闭模式；Rust e2e 范式 = rp-mcp-parity/tests/parity.rs:170-181（Command 起 python server env 注入） | selftest_auth.sh:13-17；parity.rs:170-181 |
| A11 词表占用 | `caller`/`tenant` 全 src 零命中（未占用）；`actor` 仅 audit_event 自由串；`resource.owner` = GitHub repo owner 非身份；caller ⊥ wsw_session（024:45 代理 scope 工作区） | 全仓 grep |
| A12 配置面 | env 清单 RP_PORT(7800)/RP_HOST/RP_DB_PATH(platform.db)/RP_AUX_DB/RP_AUTH_TOKEN/RP_AUTH_TOKEN_FILE(.rp-workbench-token)/RP_RATE_LIMIT_RPM(600)/RP_API_AUDIT_LOG/…（rp_server.py:50-72 头注 + :96-141） | rp_server.py:50-141 |
| A13 依赖面 | Cargo.lock 399 包恒定；`sha2 = "0.10"` 已是 crates/database 依赖（vendor 在库，Cargo.toml:19-21 注释）；Python 侧 hashlib/secrets 全 stdlib | Cargo.lock；crates/database/Cargo.toml:19-21 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `migrations/026_caller.sql` | 新增 | caller + api_key 两表纯新增（零 ALTER；D1 词表 T0/T1/T2 CHECK 约束；FK caller_id→caller；ix_api_key_caller）+ 尾部 schema_version 登记行（024:88 形态）+ 文件头「纯新增零 ALTER」纪律注释（025:6-7 同款） | S2 |
| `apps/server/rp_server.py` | 修改 | 三扩展点：①`_load_auth_token` 增多 key 加载面（直查 026 两表 mode=ro，构建 key_hash→caller 内存映射 + legacy 单令牌并存，D3/D4）；②`require_auth` 扩 key 校验（sha256→主键查询→compare_digest→caller 解析→吊销/停用判定→401 形状不变，D2）；③`audit_api_access` 行加 `caller` 字段（D5）。鉴权关闭模式（403 auth_required_for_write）语义不动 | S3 |
| `crates/database/src/bin/rp-admin.rs` | 修改 | 五子命令（D6）：caller-add/caller-list/key-gen/key-list/key-revoke；手写解析 + usage 更新 + JSON 输出 + 退出码口径；key 生成（/dev/urandom 32B → base64url → `rp1_` 前缀）与 sha256 哈希（sha2 既有依赖） | S4 |
| `apps/server/selftest_auth.sh` | 修改（additive） | 追加多 key 用例块：双 caller 双 key 互不串扰 + default 兼容并存 + 未知 key 401 + 吊销后 401（revoke 经 rp-admin）+ 审计 JSONL caller 字段断言；**既有用例零改动**（判据 2） | S5 |
| `crates/rp-mcp-parity/tests/caller_auth.rs`（或 rp-mcp-parity 内新测试文件） | 新增 | Rust e2e（parity.rs:170 范式）：Command 起 python server（临时库 026 已迁移）→ rp-admin 预置双 caller 双 key → 多 key 并发访问互不串扰 + 单 Token 兼容 + 未授权 401 + 吊销即时生效 + 鉴权增量延迟粗断言 | S5 |
| `docs/rip3/MA-1-BATCH-REPORT.md` | 新增 | 批次验收报告（S6） | S6 |
| `docs/README.md` | additive | 索引补 MA-1 行 | S6 |

### 2.2 不修改

- `apps/server/selftest_auth.sh` 既有用例行（追加块之外零 diff——判据 2 载体）。
- `migrations/` 既有 001-025（历史迁移禁改纪律）；`schemas/`；`crates/tool-registry/`（caller 不进 Registry——MA 轨禁止事项 3）。
- `crates/tool-engine/`、`crates/workflow-engine/`、`crates/agent-engine/`（并发/预算帽属 MA-2；TaskClaim/ma_message 属 MA-3）。
- ResearchTask/Workflow 归属列（SQLite 027 = MA-2）；`external_api_audit` 表列形状（019 冻结）。
- 生产 `platform*.db`/`platform-dryrun.db`、`integrations/rtis/`；`Cargo.toml` 依赖面与 `Cargo.lock`（399 恒定——sha2/rand 零新增）。
- RP_RATE_LIMIT_RPM 全局限频行为（D7）；写端点授权判定语义（D8——本批鉴权只加身份维度不改授权结果）。

## 3. 实施步骤（S1→S6 串行，每步独立可验证）

- **S1 门禁基线核验**：Cargo.lock 399；`git status --short` 干净（仅本批文档）；`apps/server/selftest_auth.sh` 现状全绿记录（判据 2 before 基线）；rp_server.py 三扩展点行号实勘（:142/:4255/:4297，漂移则按函数名重定位记录）；migrate.rs 幂等行为复读。
- **S2 迁移 026**：`migrations/026_caller.sql` 落表（D1）；`migrate()` 对临时库实跑断言 user_version 升至 26 + 重跑幂等 + 单行 ALTER 守卫零触发（纯新增）。
- **S3 rp_server 三扩展点**：按 D2-D5 落；行为不变式：legacy 单令牌路径逐字节不变（compare_digest 同函数同 sleep 同 401 形状）；多 key 路径 = sha256 → api_key 主键查询（JOIN caller 吊销判定）→ compare_digest 二道比对 → request.caller；审计行 `caller` 字段三态（caller_id / default / "-"）。
- **S4 rp-admin 五子命令**：按 D6 落；`rp-admin usage` 输出更新；单测或 e2e 覆盖 key-gen 输出形状（key 明文 47 字符 `rp1_` 前缀、key_hash=sha256 全文 hex、JSON 可解析）与 key-revoke 幂等（重复 revoke 非致命）。
- **S5 测试面**：selftest_auth.sh 追加多 key 用例块（既有零 diff）+ Rust e2e `caller_auth.rs`（临时库 → 026 迁移 → rp-admin 预置 → server 起服 → 六项断言：互不串扰/default 兼容/未知 401/吊销即时 401/审计 caller 字段/相对 legacy 的配对增量 p95 < 5ms）。
- **S6 收口**：§4 全部命令实跑（离线 + --test-threads=1 Rust 面）+ clippy -D warnings + deny 双检 + lock 399=399 + `git status` 复核只含 §2.1 落点 + 批次报告落盘 + docs/README.md 登记 + 显式 add → commit → push → CI watch 至绿。

## 4. 完成标准（八项判据）

| # | 判据 | 验收 |
|---|---|---|
| 1 | 026 迁移纯新增幂等 | migrate 首跑 user_version=26；重跑零改动；单行 ALTER 守卫零触发；回滚 = drop 两表零残留 |
| 2 | 单 Token 兼容回归 | 既有 selftest_auth.sh 全部既有用例零改动通过；legacy caller 审计记 `default` |
| 3 | 多 key 互不串扰 | 双 caller 双 key 并发访问：各自请求成功、审计 JSONL caller 字段各归各、A caller 的 key 访问不到 B caller 专属资源语义（本批资源面=审计行区分，行为隔离列 MA-2/027 兜底声明） |
| 4 | 未授权 401 形状不变 | 未知 key / 吊销 key / caller 停用：401 + WWW-Authenticate + 形状同 A4；失败节流 sleep 保持 |
| 5 | 吊销即时生效 | key-revoke 后下一请求 401（无缓存，server 直查）；caller-add 后立即可用 |
| 6 | key 管理面完整 | rp-admin 五子命令 JSON 输出 + 退出码口径；key 明文仅 key-gen 一次；key-list 不回明文；revoke 幂等 |
| 7 | 零直连 SQL 写 | rp_server.py 对 caller/api_key 零写符号（grep 断言）；key 写入仅 rp-admin；selftest_phase6.sh 门禁语义保持 |
| 8 | 鉴权增量成本 | 多 key 校验 = sha256 + 主键单行查询：与 legacy 单令牌请求交替配对采样，配对增量 p95 < 5ms；避免将 HTTP 往返与系统调度噪声计入鉴权增量 |

## 5. 测试面（收口命令）

```bash
# Rust 面（rp-admin + e2e）
cargo test --offline -p database -- --test-threads=1
cargo test --offline -p rp-mcp-parity -- --test-threads=1
cargo clippy -p database -p rp-mcp-parity --all-targets --offline -- -D warnings
# server 面
bash apps/server/selftest_auth.sh
bash apps/server/selftest_phase6.sh   # 零直连 SQL 写门禁（既有，零改动回归）
# deny / lock
cargo deny check advisories && cargo deny check licenses
grep -c '^\[\[package\]\]' Cargo.lock   # 399
```

## 6. 红线（八条）

1. **零新依赖**：Cargo.lock 399 恒定；sha2 用 database 既有依赖，随机源 /dev/urandom，Python 全 stdlib（S1/S6 双点复核）。
2. **单 Token 兼容零破坏**：既有 selftest_auth.sh 用例零改动通过；legacy 路径逐字节不变（判据 2 硬门）。
3. **server 零直连 SQL 写**：caller/api_key 写入仅 rp-admin CLI；rp_server.py 校验/审计路径零写（判据 7；selftest_phase6.sh 门禁保持）。
4. **不建第二事实源**：026 是 platform.db 同库纯新增两表；零分库零分 schema（计划 §三.1）。
5. **caller 不进 Tool Registry**：crates/tool-registry 零 diff；caller 是运行时维度（计划 §六.3）。
6. **历史迁移禁改**：001-025 零 diff；026 纯新增零 ALTER（migrate.rs:62 守卫语义）；schema_version 登记行随尾。
7. **生产库零触碰**：测试一律临时库；platform*.db / platform-dryrun.db / integrations/rtis 零 diff。
8. **边界纪律**：per-caller 限流/并发/预算帽、TaskClaim、知识供体面、ResearchTask 归属列、授权 enforcement 全部不进本批（D7/D8；越界即停批上报）。

## 7. 回滚

- 单 commit 交付；`git revert` 整批撤销。026 对已迁移库残留两空表（无害；回滚说明登记 drop 语句）。
- rp_server.py 三扩展点独立可逆：多 key 路径失败自动落 legacy 分支（加载面 try/except 兜底——server 不会因 026 缺表拒起：key 表查询失败 → 记 warn 降级 legacy-only，不崩）。
- rp-admin 新子命令纯新增，revert 后零残留。

## 8. 交付物

1. `migrations/026_caller.sql` + `apps/server/rp_server.py` 三扩展点 + `crates/database/src/bin/rp-admin.rs` 五子命令。
2. 测试：selftest_auth.sh 多 key 块 + `caller_auth.rs` e2e（八判据逐项有实测）。
3. `docs/rip3/MA-1-BATCH-REPORT.md`（命令原文 + 数字实测 + 坏消息单列）+ docs/README.md 登记。
4. MA-2 前置输出：per-caller 审计行形状样例（MA-2 预算面直接消费 caller 维度）+ caller 表权限位语义备忘（MA-3 节点角色绑定输入）。
