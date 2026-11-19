# WSW-2 批次收口报告 · Proxy / MITM Runtime

- 批次：WSW-2（PLAN §九 :604 Listener/CONNECT/TLS CA/拦截队列/Forward-Drop）
- 日期：2026-11-19～11-20（批次主体 11-19，S8 实拍复测 23:48/00:02，报告员收口 11-20）
- 依据：`docs/rip3/wsw/WSW-2-TASK.md`（硬验收 §四）+ `docs/rip3/wsw/WSW-2-ADR.md`（W2-D1～D10）+ ADR-0004 D4/D5
- 收口方式：本地工作树实施与验证；主体改动经中断点快照 commit `738ebc14` 入库（"WIP 备份：WSW-2 主体实施中断点快照"，提交者 emoeem，`git show -s` 实核），fmt 收敛、评审批修订（F1–F7）与本报告为工作树未 commit 面；收口提交按 REDLINES 批次循环由流程面执行
- 状态：**本地定向门禁全绿（测试 76 项 / clippy -D warnings / deny licenses / lock 400 / fmt / 024 水位 24——报告员会话 2026-11-20 全部复跑实测）；实拍验收全项通过（S8 实拍 + 评审批复跑）；W0 遗留两项 PASS；GitHub CI 实测红：快照 `738ebc14` 对应 run 44550526104 Rust workspace job failure（迁移钉值测试爆区，本地已复现，详见 E2/G12）——按任务书 §五.13「CI 红不进下一批」，收口提交排在 CI 修复批之后**

## A. 本批完成内容

1. **Listener socket 运行时**（`crates/wsw-runtime/src/listener.rs` + `listener/handler.rs`）：hudsucker 0.25.0 组装（`with_listener` + `with_ca` + `with_http_connector`）；`bind_guard` loopback 拒绑（`ConfigError::NonLoopback`，非 loopback bind 实测 exit=1）；accept 源二次校验落 handler 每请求入口（hudsucker 0.25.0 builder 只暴露 with_addr/with_listener、内部 accept loop 私有，逐连接钩子不存在——二次校验以每请求 `is_loopback_peer(ctx.client_addr)` fail-closed 403 承载，`W2-D1` 边界如实记录）；六态生命周期（既有 `ListenerState::transition` 词表）+ SIGTERM 优雅停机（拦截队列全 EXPIRED）。
2. **WswAuthority**（`src/authority.rs`，ADR-0004 D4 必做项）：自实现 `CertificateAuthority` trait；IP 直连目标签 `SanType::IpAddress`（127.0.0.1/[::1]/v6 纯/::ffff: mapped 归一 V4 四例单测逐位断言 + DNS SAN 例）；moka 缓存 cap=1000、条目 TTL=365d 半程、ALPN 广告 h2+http/1.1（vendor `rcgen_authority.rs` 常量口径同形不复制）；CA 持久化 `rip-data/wsw-ca/`（私钥 0600 create_new no-clobber / 目录 0700 / 重启加载复用同指纹）；指纹=SHA-256(证书 DER) 小写 hex（复用 `wsw_message::cas::sha256_hex`）。
3. **Scope 闸**（`src/scope.rs`，W2-D5）：闸序 scheme→host（合法性→云元数据 deny-never-allow→exclude 优先→include 空 = default deny）→port→path→IP 审计；`private_reason` 全函数自 `document_fetch.rs:448-514` 移植（mapped/NAT64/ULA/CGNAT 分支同语义）；`audit_resolved` 全地址复核（DNS rebinding 面拒绝 + remote_ip 落记录）；`check_redirect_hop` 帽入口（**单测级消费；运行时跳数接线留 WSW-3，G10**——本批重定向保障 = 每跳经代理即新请求复走查点 A）。查点 C 落 `AuditedConnector`（DNS→审计→Pinned-IP TCP 连接，tower Service<Uri> 实现；解析面 `DnsResolver` trait 可注入、生产恒系统解析；`ConnectorError::source()` 把查点 C 记录链入错误下溯链，`handle_error` 查点 C 决策 = `respond_to_scope_reject` 共享实现——集成级负例 `tests/scope_gate.rs` 两例真实执行全链，修订批落，缺陷修复见 G11）。
4. **拦截队列 + 024 持久化**（`src/persist.rs` + `migrations/024_wsw_intercept.sql`）：内存 `PendingQueue`（既有原语）+ 024 三表双面；Forward/Drop/Modified-Forward 三动作（后者查点 A 重跑）；TTL 到期/队满/停机三路径全部 EXPIRED 终态 + 可见记录；重启对账（QUEUED 未过期回载沿用原 expires_at、过期标 EXPIRED）；行式状态机 SQL `WHERE state='queued'` 强制终态唯一。
5. **错误五态 + 脱敏日志**（W2-D7/D12）：`ProxyErrorClass` 五值全链路收敛；`cause_chain_summary`（source() 链帽 8 层）；日志只落 method/uri/host/status/长度/五态计数 + `wsw_message::redact` 命中头名列表，**任何头值/体值不落日志**；查点 C 拒绝经 connector 错误链 downcast 转 403。
6. **rp_server WSW 端点 + web 两页**（W2-D8）：GET proxy-status/ca-status/intercept-queue 只读三项（`apps/server/rp_server.py:3103/3131/3159`）+ POST ca/export、ca/regenerate（confirm=true 缺失 400 实测，`_wsw_confirm_or_400` :3195）、listener/start（confirm + 已运行 409 防呆 + zombie pid 判定 `_wsw_listener_alive` :3183）、listener/stop（SIGTERM 优雅收口）；web 侧 `settings/wsw-proxy`（六态徽标/计数/启停按钮/队列摘要）与 `settings/wsw-cert`（指纹/有效期/导出/重生成/各平台手动信任说明）两页挂 Settings 工作区 tab 11.2/11.3（`web/src/workspaces.ts:306/:313`）；全程零 networksetup/gsettings/注册表写面。
7. **rp-wsw 命令行面**（`src/bin/rp-wsw.rs`）：listener/ca status|export|regenerate/db migrate 四子命令（Python 子进程消费，rp-doc/rp-workflow 先例模式；rcgen CA 与迁移执行只在 Rust 侧）。
8. **W0 遗留两项终态（S9，均 PASS，原文见本报告尾部 S9 小节）**：(1) 上游纯 h2-only 本地夹具实测 PASS（服务端实核协商 ALPN==Some("h2")，ADR-0004 差距 (b) 收口）；(2) 容器内 aws-lc-sys 构建 PASS（cmake 回退未触发，宿主机零系统级安装，差距 (f) 收口）。

## B. 架构变化和复用说明

- **不新建 crate**（W2-D1 否决 wsw-proxy 独立 crate）：五模块原地落 `crates/wsw-runtime`（listener/authority/scope/persist + listener::handler）；既有根原语（ListenerConfig/ListenerState/InterceptState/PendingQueue/is_loopback_peer，`src/lib.rs`）零改动；crate doc 的"no socket IO"声明按 ADR D1 作废改模块化声明。
- **× wsw-message**：只消费不动（git status/diff 实核 `crates/wsw-message/**` 零变更）——CAS `store_body` + `wsw_body_blob` 登记（023 表）、`redact` 日志投影（`redact.rs` 挂展示/日志侧）、`sha256_hex` 指纹与 ExchangeId 词表；不预建 `wsw_exchange`（W2-D9 留 WSW-3）。
- **× RT-1 面**：零接触。RT-1 面（`crates/tool-engine` native_rust adapter/capability、`crates/mcp-server` dispatch、`crates/tool-registry` validate）在本批快照与工作树 diff 中零文件（`git show --name-status 738ebc14` 实核）；代理数据面与 Native Rust Adapter 无调用关系，能力词表/信封截断语义未被本批触碰。
- 复用：`database`（rusqlite 重导出 + migrate 执行器唯一入口）、POC 四段只读参照搬运（CA 生成/loopback 守卫/connector 装配/错误链，`wsw-poc` 本体零改动，D2 不转正）。
- 依赖面：hudsucker/rcgen/moka/time/rand/hyper-rustls/webpki-roots/tokio-rustls/futures/tracing/tracing-subscriber/tower-service 全部传递依赖转正直接声明 + 等号钉 lock 既有版本（WSW-1 sha2/base64 先例口径）；`tokio = { workspace = true, features = ["net"] }` additive 增补仅落 wsw-runtime 自己的 Cargo.toml（POC 先例 `crates/wsw-poc/Cargo.toml:37-38`），workspace features 不动。

## C. 修改文件清单（实测逐文件，2026-11-20 报告员会话复核）

证据源：`git show --name-status 738ebc14`（快照批 33 文件）+ `git status --porcelain`（工作树 14 修改 + 本报告 untracked）。两列 = 快照批动作 / 报告员会话工作树实态。

| 文件 | 快照 738ebc14 | 工作树（2026-11-20 实测） |
|---|---|---|
| `Cargo.lock` | M（仅 wsw-runtime 依赖清单行，`[[package]]=400`） | 干净 |
| `apps/server/rp_server.py` | M（WSW 端点七项 :3103–:3532） | 干净 |
| `crates/document-router/Cargo.toml` | M（+`publish = false` 一行，既有 deny 红修复，G5） | 干净 |
| `crates/wsw-runtime/Cargo.toml` | M（依赖增补 + tokio net） | M（fmt + rand 注释对齐 0.10.4） |
| `crates/wsw-runtime/src/lib.rs` | M（模块声明 + crate doc；根原语不动） | 干净 |
| `crates/wsw-runtime/src/authority.rs` | A（WswAuthority） | M（F5 fail-closed 分支跳过 cache.insert） |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | A（CLI 面；任务书清单外增补，G6） | M（fmt） |
| `crates/wsw-runtime/src/listener.rs` | A（bind 守卫/Proxy 组装/六态/AuditedConnector） | M（F2 DnsResolver trait + `ConnectorError::source()`） |
| `crates/wsw-runtime/src/listener/handler.rs` | A（handler/拦截桥/五态/tee 捕获） | M（F2 respond_to_scope_reject 共享实现 + F6 CONNECT 日志剥 userinfo） |
| `crates/wsw-runtime/src/persist.rs` | A（024 三表 store） | M（fmt） |
| `crates/wsw-runtime/src/scope.rs` | A（ScopeRules/check/audit_resolved） | M（F1 模块文档帽口径同步） |
| `crates/wsw-runtime/tests/authority_ca.rs` | A（5 例） | M（fmt） |
| `crates/wsw-runtime/tests/intercept_queue.rs` | A（7 例） | M（fmt） |
| `crates/wsw-runtime/tests/mitm_loopback.rs` | A（6 例，含 h2-only 夹具） | M（F7 [SKIP] curl 缺失可见化） |
| `crates/wsw-runtime/tests/scope_gate.rs` | A（14 例 = 12 矩阵 + 2 集成级负例 F2） | M（fmt） |
| `docs/rip3/batches/browser-e2e/wsw-2/01-range-index-via-proxy.png` | A | 干净 |
| `docs/rip3/batches/browser-e2e/wsw-2/02-marker-via-proxy.png` | A | 干净 |
| `docs/rip3/batches/browser-e2e/wsw-2/03-out-of-scope-403.png` | A | 干净 |
| `docs/rip3/batches/browser-e2e/wsw-2/04-https-connect-mitm.png` | A | 干净 |
| `docs/rip3/batches/browser-e2e/wsw-2/05-ui-wsw-proxy-page.png` | A | M（e2e 重摄覆盖） |
| `docs/rip3/batches/browser-e2e/wsw-2/06-ui-wsw-cert-page.png` | A | M（e2e 重摄覆盖） |
| `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md` | M（§2 入口位一处 :66，W2-D1；:17/:207 残留两处见 G13） | 干净 |
| `docs/rip3/wsw/WSW-1-BATCH-REPORT.md` | M（§5/§6 CI 终态回填，其他节未动；事实态报告员会话复核通过，见 E2） | 干净 |
| `docs/rip3/wsw/WSW-2-ADR.md` | A（W2-D1～D10） | M（F1 帽口径 + F4 rand 0.10.4） |
| `docs/rip3/wsw/WSW-2-TASK.md` | A（S1→S10 + 红线 13 条） | M（F1 §一.4 帽注记 + F2 §四.2 集成级口径 + F4 rand 行） |
| `migrations/024_wsw_intercept.sql` | A（纯新增零 ALTER） | 干净 |
| `web/scripts/rip3_wsw2_browser_e2e.mjs` | A（chromium 经代理 e2e） | M（自包含版 16/16） |
| `web/src/App.tsx` | M（路由） | 干净 |
| `web/src/routes/SettingsPage.tsx` | M（tab 11.2/11.3 链接） | 干净 |
| `web/src/routes/WswCertPage.tsx` | A（证书状态页） | 干净 |
| `web/src/routes/WswProxyPage.tsx` | A（代理状态页） | 干净 |
| `web/src/workspaces.ts` | M（两 tab 注册） | 干净 |
| `web/tests/workspaces.test.ts` | M（路由数钉值 18→20，设计内扩展的过时钉值更新，如实记录） | 干净 |
| `docs/rip3/batches/WSW-2-BATCH-REPORT.md` | —（本文件） | ?? untracked（本报告） |

本地许可面（gitignored，不进 git status，实测核账）：`migrations/dev-b1.db`（开发库副本水位 23→24，`.gitignore:35 *.db`）；`rip-data/wsw-ca/`（CA 私钥 600/目录 700）与 `rip-data/wsw-state/`（导出 PEM/状态文件/日志，运行时数据 `.gitignore:47 rip-data/`）。

不动面实测零改动：`crates/wsw-message/**`、`crates/wsw-poc/**`、`migrations/0{01..23}.sql` 历史、`migrations_pg/**`（PG 链头 V0006 未动）、workspace `Cargo.toml` tokio features、`vendor/**`、RT-1 面（tool-engine/mcp-server/tool-registry）、`migrations/platform*.db` 五库（D 节水位反证）。

## D. 数据库迁移、Schema 和兼容性变化

- `024_wsw_intercept.sql`：纯新增三表 `wsw_listener`/`wsw_session`/`wsw_intercept`（列 CHECK 词表逐列：ID 前缀/六态/五态/方向/ISO8601ms 时间戳）+ 三索引 + `schema_version` INSERT OR IGNORE (24,...) + `PRAGMA user_version=24` 双写；零 ALTER、零既有表改动。
- 执行面：测试内存库（intercept_queue 套件 user_version 断言）+ 开发库副本 `migrations/dev-b1.db`（`rp-wsw db migrate` 实跑 `[24] applied, user_version=24`）；其余开发库副本保持原值不越界——报告员会话实测：dev-b1=**24**，dev-b2=23、dev-b34=5、dev-b34-b3=23、dev-b34-b4=23、dev-b5=23（`PRAGMA user_version` 只读逐库）。
- 生产库零触碰反证（报告员会话实测）：五库 `platform.db`/`platform-dryrun.db`/`platform-fullbatch.db`/`platform-governance.db`/`platform-search.db` user_version = 1/1/1/0/0 且 `sqlite_master` 中 `wsw%` 表 = 空；`git status` migrations/ 零行。
- PostgreSQL 链不动：`migrations_pg/V0001__extensions.sql … V0006__search_hybrid.sql` 七文件链头 V0006 零变更（`git diff 077a8dd4..HEAD --name-status -- migrations_pg/` 空 + `git status --porcelain migrations_pg/` 空实测）。
- **兼容性新债（G12/E2）**：仓库级迁移钉值测试（agent-engine 1 文件 + database 4 文件）的版本清单钉在 1..23，024 入链即红——本批定向门禁（`-p wsw-runtime`）不覆盖该面，CI 实测暴露；修复 = 五文件钉值 1..23→1..24，属 CI 修复批必做项。

## E. 测试命令及真实结果（报告员会话 2026-11-20 复跑实测）

```text
cargo test --offline -p wsw-runtime -- --test-threads=1
→ lib 44 passed（9 既有基线 + 35 新增：SAN 四例/DNS/缓存同 Arc/持久化复用/0600/
  fail-closed/闸序 12 例/队列桥 7 例/时间历法等）
→ authority_ca 5 passed；intercept_queue 7 passed；scope_gate 14 passed
→ mitm_loopback 6 passed（靶场明文/CONNECT MITM/超范围 403/云元数据 403/
  重定向越界 403/队列 TTL 自动放行/非 loopback 拒绑 v4+v6/h2-only 上游 ALPN=h2/
  CA 重生成指纹变化/超帽不落 CAS）
→ 合计 76 passed 0 failed（--test-threads=1；rp_wsw bin 0 + doc-tests 0）

修订批增量（评审收口，76 = 74 + 2，全部实测回显）：
- scope_gate +2 = DNS rebinding 集成级负例（任务书 §四.2 行集成级口径落地）：
  解析桩注入（DnsResolver trait，免改 /etc/hosts）→ AuditedConnector::call
  拒携带 remote_ip（rfc1918）→ respond_to_scope_reject（handle_error 同一
  实现）403 + x-wsw-scope-decision 头；顺带修复查点 C glue 链缺陷（G11）。
- authority.rs gen_server_config fail-closed fallback 分支跳过 cache.insert：
  既有两处 cache_entry_count()==0 断言（authority.rs 单测 + authority_ca.rs
  invalid_host_fail_closed）由"moka entry_count 最终一致性滞后侥幸通过"变为
  真实成立（测试注释"fail-closed 条目不进缓存"与代码行为对齐）。
- 测试账目环境前提：mitm_loopback 三例 curl 子进程用例（matrix/intercept/
  h2-only）在无 curl 环境打 [SKIP] 标记后空跑返回（计 passed、断言数 0）——
  收口环境有 curl（matrix 2.6s 全断言真实执行）；缺 curl 环境复跑须按 [SKIP]
  输出核对实拍矩阵缩水面。

cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
→ Finished（零 warning 零 error，exit 0）

cargo deny check licenses → licenses ok
grep -cE '^\[\[package\]\]' Cargo.lock → 400
cargo fmt -p wsw-runtime -- --check → exit 0
dev-b1 user_version → 24（报告员只读实测）；platform*.db 五库 wsw% 表全空

仓库级钉值面（报告员会话实测，CI 修复批对象，详见 E2）：
cargo test --offline -p agent-engine --test migration_idempotent → FAILED
  （migration_idempotent.rs:67 钉值 [1..23] vs 实际 [1..24]）
database 四套件 migrations_{agent_data_plane,tool_contract,runtime_node,
  context_broker} → 6 测试 FAILED（同一钉值模式 1..23 vs 1..24）
```

三通道实拍摘要（S8 实拍批实测 + 评审批复跑核对；报告员会话未重建实拍栈，实拍数字以下列两次实测为准，报告员仅复核清理态：ss 3128 无监听、pgrep 零残留）：

| 通道 | 命令 | 结果 |
|---|---|---|
| 明文 HTTP 经代理 | `curl -s -x http://127.0.0.1:3128 http://127.0.0.1:8443/p59c-marker` | `RIP3-P59C-LOCAL-RANGE-MARKER:200` |
| HTTPS CONNECT MITM | `curl --cacert rip-data/wsw-state/exported-ca.pem -x … https://127.0.0.1:9443/p59c-marker` + `openssl s_client -proxy …` | 200；issuer=CN=WSW-2-ROOT-CA、SAN `IP Address:127.0.0.1`、Verify return code 0 (ok) |
| 越界拒绝 | `curl -x … http://example.com:80/` / `http://169.254.169.254/…` / 重定向第二跳 | 三例全 403（reason=host_not_included / metadata_endpoint；allow_private=true 仍拒） |

S8 实拍命令原文与关键回显（S8 实拍批留证；评审批逐项复核）：

```text
./target/debug/rp-wsw listener --bind 0.0.0.0:3128 → exit=1，stderr "[rp-wsw] error: config: NonLoopback"
curl -sL -x http://127.0.0.1:3128 http://127.0.0.1:8443/redirect-out-of-scope → 第二跳 403
  （final-url:http://out-of-range.example.invalid:80/nope；listener 日志 scope_decision=out_of_scope_blocked）
ss -tlnp | grep rp-wsw → LISTEN 127.0.0.1:3128 仅此一条；非 loopback 绑定面 grep 空（pass）
stat -c "%a %n" rip-data/wsw-ca/ca.key.pem → 600；git status --porcelain | grep rip-data → 零行
  （报告员会话复核：600/目录 700 + git check-ignore 命中 .gitignore:47 + 零 rip-data 行）
wsw_body_blob 行面证据：sqlite rip-data/wsw-e2e.db SELECT sha256,size_bytes,mime_type,first_seen_exchange_id
→ ('5104ad88abb95810555a405ab8bc67ec8780f505aa17f32847d04fe0d1d90ff4', 28,
   'text/plain; charset=utf-8', 'wsw-1795046916948548-0-6295')，CAS 文件
   rip-data/artifacts/wsw/51/5104ad88… 在盘；重复请求 INSERT OR IGNORE 去重（blob_count 1→1）
web e2e：PW_CHROMIUM=/usr/sbin/chromium node scripts/rip3_wsw2_browser_e2e.mjs
→ 自包含版 16/16 PASS exit 0（评审批复跑值）：CA 导出无 confirm 400 confirm_required /
  confirm=true 200 / 靶场首页经代理 / marker 经代理 / 越界 403 文案 / CONNECT MITM marker /
  时序 5 项 / 捕获计数增量 requests_total 0→9；CI run 44550526104 Web job=success 为 738ebc14
  时点独立旁证（骨架版 6/6）
npm --prefix web run build → ✓ built；npm --prefix web run test → Test Files 19 passed，Tests 124 passed
  （S8 实测；CI Web job success 旁证；报告员会话未复跑 npm 面）
rp_server WSW 端点实拍（RP_AUTH_TOKEN=wswtest RP_PORT=7892）：
  POST /api/wsw/ca/export {} → 400 confirm_required（无 confirm 拒绝执行）
  POST /api/wsw/ca/export {"confirm":true} → exported=true + PEM 落
        rip-data/wsw-state/exported-ca.pem + fingerprint b851210fc181d5dc…
  POST /api/wsw/ca/regenerate {} → 400 confirm_required
  POST /api/wsw/listener/start（运行中再 start）→ 409 listener_already_running
  POST /api/wsw/listener/stop → stopped=true "SIGTERM 优雅停机：拦截队列全 EXPIRED 后退出"，
        终态 status 文件 state=stopped；停后再 start 成功（zombie pid 判定修复后复测）
```

## E2. CI 状态（2026-11-20 报告员实测回填；终态待 CI 修复批回填）

**实测：快照 `738ebc14` 已被推送并触发 CI，结论 failure（`gh run view 44550526104` / `gh run list` 实核）**。前稿"WIP 快照不触发 CI、无可核验 run"与实测不符，本节按实测改写。

- run **44550526104**（headSha=738ebc14d85eb73c1f643048d07e0b95327dd8bc，event=push；日志时间戳 2026-11-19T15:06Z）：四 job 三绿一红——`Rust workspace（测试与 clippy 零警告）`=**failure**；`Web 工作台`/`Python worker 测试`/`协议一致性载具`=success。
- 失败测试（`gh run view 44550526104 --log-failed` 原文）：`agent-engine::migration_idempotent::migrations_copy_is_idempotent_with_no_schema_diff`——`crates/agent-engine/tests/migration_idempotent.rs:67` 断言 `all migrations must apply once`，left=[1..24] right=[1..23]；`Process completed with exit code 101`。根因 = 仓库级迁移钉值测试的版本清单钉在 1..23（清单注释止于"023 为 WSW-1 wsw_body_blob CAS 表"），024 入链即红；CI fail-fast 只暴露 agent-engine 一处。
- 本地复现（报告员会话实测，非 CI 环境特有）：`cargo test --offline -p agent-engine --test migration_idempotent` → 同断言 FAILED；同钉值模式在 `crates/database/tests/` 另有四文件全部实测 FAILED（migrations_tool_contract 2 例、migrations_agent_data_plane 1 例、migrations_runtime_node 1 例、migrations_context_broker 1 例，共 **5 文件 6 测试**）。
- 修复面（CI 修复批必做）：五文件钉值 `1..23` → `1..24`（agent-engine/tests/migration_idempotent.rs:69 + database/tests/migrations_{agent_data_plane.rs:82, tool_contract.rs:106, runtime_node.rs:71, context_broker.rs:107}）并补 024 清单注释行。
- 前批 CI 终态（本会话 gh 实核，与 `WSW-1-BATCH-REPORT.md` §5 回填一致）：`077a8dd4` → run **44293771053 success**（`gh run view`）；`2cef9439` → run **44394568183 success**（`gh run list`）。
- 收口序列：任务书 §五.13「CI 红不进下一批」→ 本批收口提交排在 CI 修复批（钉值 1..24）之后；本节终态待修复批 run 回填。

## F. 性能数据

单请求代理延迟增量（S8 实拍批实测：curl `-w time_total`，**直连 vs 经代理各 20 次**取中位数，本机靶场 127.0.0.1:8443/p59c-marker；阈值 <50ms；评审批复跑 direct/proxy 同量级同结论；报告员会话未重建实拍栈不重复计时）：

| 面 | 中位数 | runs（20 次全列，ms） |
|---|---|---|
| 直连 | 0.678 ms | 0.593 0.614 0.821 0.967 0.635 0.599 0.877 0.588 0.776 0.679 0.637 0.652 0.609 0.665 0.802 0.708 0.677 0.822 0.772 0.759 |
| 经代理（明文） | 1.599 ms | 2.089 1.468 1.616 1.538 1.515 1.519 1.472 1.751 1.581 1.559 2.179 1.764 1.576 1.658 1.320 1.543 3.004 2.208 2.392 1.863 |
| **明文代理延迟增量** | **+0.921 ms（<50ms 阈值 PASS）** | |
| 经代理（CONNECT MITM HTTPS，含双向 TLS 握手；repeat=3 参考） | 44.179 ms | 44.179/43.492/45.912 |

UI 时序断言（硬门，写进 `web/scripts/rip3_wsw2_browser_e2e.mjs` 本身，任一超阈值 e2e exit 1；S8/评审批实测值）：

| 断言 | 阈值 | 实测 |
|---|---|---|
| 经代理首屏加载（靶场首页 domcontentloaded） | <3s | 28 ms |
| UI 首屏（代理状态页可交互，waitForSelector 数据面） | <3s | 456 ms |
| 页内交互响应（证书状态页「导出 PEM」按钮出确认消息） | <1s | 57 ms |
| API `/api/wsw/proxy-status` p95（n=20） | <300ms | 3.3 ms（median 1.6ms） |
| API `/api/wsw/ca-status` p95（n=20） | <300ms | 1.9 ms（median 1.1ms） |
| API `/api/wsw/intercept-queue` p95（n=20） | <300ms | 3.3 ms（median 2.7ms） |

PLAN §11.6 其余项（吞吐/并发连接/大 Body 内存峰值/长跑稳定性）如实登记**缺测**：本批无可复用压测夹具，未跑即不填数（PLAN :754 禁预填）；无吞吐基线；并发/内存面属 WSW-3 History 落库批随 History 压测夹具补测。

## G. 未解决问题与风险（坏消息单列）

1. **accept 源二次校验落点妥协（W2-D1 边界）——与"本机访问令牌"合并为一条显式遗留跟踪项（修订批口径）**：hudsucker 0.25.0 builder 只暴露 with_addr/with_listener，内部 accept loop 私有，无逐连接 accept 钩子——"accept 源二次校验"落为 handler 每请求入口 `is_loopback_peer(ctx.client_addr)` fail-closed 403（非 loopback 对端任何请求都不被服务；bind 守卫前置保证本就只有 loopback 能连）；连接级（CONNECT 隧道建立、下游 TLS 握手阶段）无 client_addr 校验入口，"本机其他进程滥用代理"的面在 WSW-3 本机访问令牌方案落地前完全依赖 bind_guard 前置 + 每请求 fail-closed 二次校验承载。**跟踪项 = 连接级校验缺口 + 令牌面同点收口**（H 节 WSW-3 依赖清单同步）：WSW-3 令牌方案落地时一并收口（令牌 = 逐请求凭据校验的强形式）；上游 hudsucker 若发布带逐连接 accept 钩子的版本，优先升级收口连接级校验。连接级二次校验需上游面（hudsucker issue 方向）或换栈才可做。
2. **拦截超时自动放行（fail-open 到透传）**：TTL 到期/队满时请求自动放行 + EXPIRED 终态 + 可见记录（PLAN :536 语义）。拦截面是可用性便利不是安全闸——安全闸是 scope（default deny）。若消费方预期 fail-closed 语义，需 WSW-3 裁决改默认。
3. **tee 捕获的 trailer 帧不进 data 流**：hudsucker `Body::from_stream` 只承 data 帧；trailer 丢弃（罕见面，记录于代码注释）。
4. **capture 超帽不落 CAS**：>max_body_bytes 的体只记 truncated 日志不落 CAS 行（避免半真数据入 CAS）；fidelity=truncated 的 sha256 登记留 WSW-3 与 envelope 口径对齐后再补。
5. **既有 deny 红修复（非本批引入）**：DI-1 批 `crates/document-router/Cargo.toml` 缺 `publish = false`，`cargo deny check licenses` 在干净 HEAD（git stash 实测）即 FAILED（document-router unlicensed）——违反 deny.toml:19 "workspace 自有 crate 已标 publish=false" 契约。本批补一行修复，deny 转绿。
6. **rp-wsw bin 为任务书清单外增补**：`crates/wsw-runtime/src/bin/rp-wsw.rs`——rp_server 端点需 Rust 侧子进程承载 rcgen CA 生成/迁移执行/监听器生命周期（Python stdlib 无此能力），rp-doc/rp-workflow 子进程先例模式；`src/bin` 自动发现，Cargo.lock 无变化。
7. **wsw_session/wsw_listener 行的 API 写入面**：listener 启停经 rp_server→rp-wsw 编排，024 行登记由 Rust 侧完成；session 行当前仅 CLI/测试面写入，UI session 管理留 WSW-3。
8. **本机 chromium 走 playwright 用户缓存探针落空**（WSW-2 期间 `~/.cache/ms-playwright` 无 chromium-*），e2e 以 `PW_CHROMIUM=/usr/sbin/chromium` 系统浏览器执行（脚本探针原样保留，缓存恢复后自动切回）。
9. **实拍期间环境扰动如实记录**：S8 后段机器重启一次（内核 7.3.4→7.3.5），实拍栈重建后复测（三面 curl 200 + e2e 16/16 PASS 为复测值）；S8 前段证据（matrix 测试/STATUS 计数/队列行）为重启前实测，两类证据相互独立不互替。
10. **`max_redirects` 跳数帽运行时未接线（修订批登记）**：`scope.rs::check_redirect_hop` 帽入口仅单测级消费（scope.rs 单测 + tests/scope_gate.rs），listener/handler 生产路径零调用点——真实代理无法感知与累计重定向跳数。本批重定向保障 = 每一跳经代理即新请求复走查点 A（越界目标不会被放行，mitm_loopback 越界重定向 403 实测）；帽语义（逐跳计数，Host+Referer 链或 session 级计数）留 WSW-3 于 handler/连接面落真实跳数追踪。ADR W2-D5 查点 B（:143）、scope.rs 模块文档与任务书 §一.4 已同步改口径（防裁决文档与实现漂移）。
11. **查点 C glue 链缺陷修复（修订批集成级负例实测暴露，坏消息单列）**：`ConnectorError::ScopeRejected` 的拒绝记录原藏在枚举变体内且 `impl StdError` 未覆写 `source()`——hyper-util legacy client 以 `e!(Connect, src)` 把 ConnectorError 本体挂为 legacy::Error 的 source（registry hyper-util-0.1.21 `client.rs:464/:517` 实读），`handle_error` 侧 `downcast_ref::<ScopeRejectRecord>()` 永远匹配不到，DNS rebinding（查点 C）拒绝在生产面会落成 502 upstream_connect 而非 403。修复：`ConnectorError::source()` 把 record 链入下溯链 + `handle_error` 查点 C 决策提取为 `respond_to_scope_reject` 共享实现（集成测试喂同链形错误走同一路径）；解析面抽 `DnsResolver` trait 可注入（生产恒 `SystemDnsResolver`），`tests/scope_gate.rs` 新增两例集成级负例真实执行全链（此前该防线为"零测试执行 glue"）。
12. **CI 红：仓库级迁移钉值测试爆区（本报告员实测，收口阻断项）**：run 44550526104（738ebc14）Rust workspace job failure——五文件六测试钉值 [1..23] 未跟上 024 入链（agent-engine 1 + database 4，本地已全部复现，详见 E2）；本批定向门禁 `-p wsw-runtime`/`-p wsw-message` 不覆盖该面，快照入库时未发现。按任务书 §五.13 收口提交排在 CI 修复批后；教训入 K 节。
13. **DESIGN §2 入口位残留两处**：`05-DESIGN-AND-THREAT-MODEL.md:17`（Wire 行仍写 "WSW-2 `crates/wsw-proxy`（采集）"）与 `:207`（仍写 "`wsw-proxy::listener::bind_guard`"）未随 W2-D1 回改——本批按任务书 S10 口径只回改一处（:66 STOPPED→STARTING 行，git show 738ebc14 diff 实核）；其余两处属残留漂移，WSW-3 开批或修订批清理，防后续按旧名找 crate。

## H. 下一批依赖和明确的非目标

- **先决（CI 修复批，先于本批收口提交）**：五文件迁移钉值 1..23→1..24（清单见 E2/G12）+ 修复批 run 绿后回填 E2 终态；可顺带清理 DESIGN :17/:207 残留（G13）。
- WSW-3 依赖（History + Repeater）：`wsw_exchange` 元数据表（D9 留批）+ History 落库 + audit-engine Emit 出口裁决 + 拦截队列 UI 编辑器（本批只读摘要）+ **本机访问令牌完整方案（显式遗留跟踪项 = G1 连接级 accept 校验缺口与令牌面同点收口；上游 hudsucker 出 accept 钩子版本优先升级收口）** + **重定向跳数帽接线（G10：handler/连接面真实跳数追踪，Host+Referer 链或 session 级计数）** + performance 压测夹具（F 节缺测项：吞吐/并发/内存峰值）。
- 非目标（本批未做，做了即越界）：透明代理/TPROXY/HTTP-3；History/Repeater/Fuzzer UI 与发送面；WebSocket Workbench（本批仅 WS 帧透传挂载，未实测——POC 同口径）；拦截规则结构化编辑器（本批最小 host/method/path 匹配）；wsw_exchange/audit-engine 接线（D9）。

## I. UI 截图与浏览器验收证据

`docs/rip3/batches/browser-e2e/wsw-2/`（6 张，全部实拍，报告员会话 ls 实测文件 9.8KB–341.5KB 非空、时间戳 2026-11-20 00:02 重摄覆盖）：

| 文件 | 内容 |
|---|---|
| 01-range-index-via-proxy.png | chromium 经代理渲染靶场首页 |
| 02-marker-via-proxy.png | marker 经代理命中 |
| 03-out-of-scope-403.png | 越界请求代理 403（out_of_scope_blocked 文案） |
| 04-https-connect-mitm.png | CONNECT MITM HTTPS marker 渲染 |
| 05-ui-wsw-proxy-page.png | 代理状态页实拍（六态 running/计数/队列 expired 行/五态计数） |
| 06-ui-wsw-cert-page.png | 证书状态页实拍（指纹/有效期/私钥 0o600/信任安装说明） |

e2e 脚本 `web/scripts/rip3_wsw2_browser_e2e.mjs` **自包含版 16/16 PASS（exit 0，评审批复跑）**：脚本自行起停 local_range 靶场、rp-wsw listener（loopback + scope 规则）、rp_server（WSW 端点宿主）与进程内 HTTPS 夹具/静态服务（web/dist + /api 反代），退出清理全部子进程；断言含 CA 导出 confirm 负例 400/正例 200、明文/越界 403/CONNECT MITM、UI 时序三硬门（超阈值 exit 1）与新增 API 端点 p95（20 采样）；chromium 探针 ms-playwright 缓存 → `PW_CHROMIUM` 覆盖 → /usr/sbin/chromium 兜底。

## J. RTIS 对账和数据安全回归结果

- RTIS 零接触：本批零 RTIS 文件/Schema/写面改动（红线 9；intel-db 未进依赖面，RT-1 面同零接触见 B 节）。
- 凭据不落日志：日志投影无任何头值/体值；`wsw_message::redact` 命中头名列表可出现、值不可出现（设计 + 代码注释 + 测试面）；CONNECT 日志 authority 面同口径——`should_intercept_connect` 改打 `split_authority` 归一 host:port（userinfo 剥离，absolute-form `user:pass@host` 形态不落日志，修订批收口）。
- CA 私钥 0600 落 rip-data/wsw-ca/（报告员会话 stat 复核 600/目录 700）；`git status` 零 rip-data 行（实测 grep=0）；`git check-ignore rip-data/wsw-ca/ca.key.pem` 命中 `.gitignore:47`。
- 禁静默安装/禁默认改系统代理：全代码面无 networksetup/gsettings/注册表写调用；导出/重生成端点无 confirm 一律 400（实拍两例）；e2e HTTPS 截图经 `ignoreHTTPSErrors` 仅作渲染路径证明，信任链证据由 curl --cacert + openssl s_client（Verify return 0）承载。
- 实拍栈清理态（报告员会话复核）：ss 3128 无监听、靶场/listener 进程零残留；rip-data/wsw-state 运行时文件在盘且不入库。

## K. 本批教训实录

1. **仓库级迁移钉值测试是隐性合同，新增迁移必须同批 grep 锁定钉值面**：`vec![1,2,…,23]` 钉值散布 agent-engine + database 五文件六测试，本批定向门禁 `-p wsw-runtime` 挡不住，快照入库即 CI 红（G12/E2 实测）。新迁移入链的固定动作 = `grep -rn "vec!\[1, 2, 3" --include="*.rs" crates/` 全量对账；CI fail-fast 只露第一处，本地要按文件面跑全。
2. **"快照/备份 commit 不触发 CI"是假设不是事实**：738ebc14 push 即跑 run 44550526104（gh 实测）——任何 commit 落库前先核 CI 面，不按 commit 性质推断（前稿 E2 即按推断写、实测推翻）。
3. **单测直调测不出 glue 链缺陷**：`ConnectorError::source()` 缺失使查点 C 拒绝在生产面会落 502 而非 403，单元级 audit_resolved 全绿完全无感——只有集成级负例真实执行 hyper-util 错误链才暴露（G11，评审批修出）。防线必须有"真实执行过全链"的集成级测试，单测绿≠防线通。
4. **缓存计数断言不可依赖最终一致性**：moka `entry_count` 滞后使"fail-closed 条目不进缓存"的错误语义断言侥幸通过（F5）；计数类断言要么对齐代码语义，要么改断言不变式（同 Arc/行为面），不赌缓存时序。
5. **裁决文档的依赖版本要与 lock 实读对账**：ADR W2-D1 写 rand 0.9.5、Cargo.lock 实值 hudsucker 依赖链 0.10.4（F4 修正）——后续批次若按 ADR 选 API 面会直接翻车；文档数字落笔前先 `grep Cargo.lock`。
6. **"实现存在"≠"运行时接线"**：`check_redirect_hop` 帽入口在库但生产路径零调用（G10）——ADR/任务书承诺的机制必须在批次报告标注接线状态（单测级/生产级），否则裁决文档与实现长期漂移。
7. **上游库 builder 能力边界要在设计期核实**：hudsucker 0.25.0 无逐连接 accept 钩子，accept 校验被迫降格每请求面（G1 合并跟踪项）——选型阶段把关键 API（builder 方法面/钩子）逐一实读，而不是组装时才发现。
8. **全仓 fmt 门与既有漂移是两个工作面**：`cargo fmt --all` 波及 89 个既有漂移文件（context-broker/intel-db/wsw-poc 等，git checkout 还原），本批只 scoped fmt；主 Containerfile 的 fmt RUN 门在当前 HEAD 即红——既有漂移不越界代修，aws-lc-sys 证明改用定向构建承载（A8）。
9. **环境扰动要分层记证**：S8 中段机器重启（内核 7.3.4→7.3.5，/tmp 基线丢失），重启前/后证据相互独立不互替；502 排障过程（夹具证书错位/进程误杀）本身成为错误五态日志的实拍证据——排障记录不删除，按证据归类。
10. **环境前提型测试要可见化**：curl 子进程用例缺 curl 时空跑计 passed、断言数 0（F7 [SKIP] 输出可见化）——环境依赖用例的"绿"必须可核对缩水面，否则实拍矩阵在别的机器上静默变空。

红线核算分布索引（13 条红线逐条落位，本报告对应节）：Rust-First/零新外部包/tokio 单点→B 节；024 迁移纪律→D 节；loopback 默认→A1/G1；CA 红线→A6/J 节；Scope 后端强制→A3/G10/G11；wsw-poc 不转正→B 节；不建第二事实体系→B/D/J 节；测试纪律→E 节；宿主机零系统级安装→A8；敏感面→J 节；收口纪律→报告头部/E2/H 节。

## 附：S9 W0 遗留两项终态（均 PASS，证据原文）

1. **上游纯 h2-only 本地夹具实测：PASS**——`tests/mitm_loopback.rs::h2_only_upstream_fixture`（报告员会话复跑套件含此例）：rustls 服务端 ALPN 只广告 h2（hyper http2-only builder），经代理请求 200 + marker，服务端实核协商 ALPN==Some("h2")（断言挂测试，非日志推断）；ADR-0004 差距 (b) 收口。
2. **容器内 aws-lc-sys 构建：PASS（cmake 回退未触发）**——toolchain 层（gcc+libc6-dev 无 cmake，`Containerfile:21-23` 实读）+ `cargo check --offline -p wsw-runtime` 定向构建：容器内实见 `Compiling aws-lc-sys v0.47.0` → `AWS-LC-SYS-IN-CONTAINER-OK`（exit 0）→ `Successfully tagged localhost/wsw-aws-lc-test:latest`（验证后镜像即删，`podman rmi`）。日志另见 `Compiling cmake v0.1.58` = aws-lc-sys 的 Rust 构建依赖 crate（非系统 cmake 二进制），C 构建走 gcc 路径；**宿主机零系统级安装**。ADR-0004 差距 (f) 收口。该证明于本会话早段构建（证据引用同会话构建日志，未重复 15 分钟量级构建）。
   - 附带发现（坏消息，非本批引入）：主 `Containerfile` 的 RUN 门 `cargo fmt --all -- --check` 在当前 HEAD 即红——workspace 既有 fmt 漂移（`git stash` 前后对比实证非本批引入），全量 fmt 门修复属独立工作面，本批不越界代修；aws-lc-sys 证明以定向构建承载（不触主 Containerfile）。
