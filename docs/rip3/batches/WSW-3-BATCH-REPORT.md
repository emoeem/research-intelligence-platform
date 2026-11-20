# WSW-3 批次收口报告 · HTTP History + Repeater

- 批次：WSW-3（PLAN §九 :605：History 列表详情、过滤搜索、请求编辑、重放、Tab 历史；不包含大规模 Fuzz）
- 日期：2026-11-20（主体实施经中断点快照 `9aff134a` 入库 → 续跑收口批 `36638fe5` → 实拍与联动验收追加批 `a6aea6a4` → 评审修订批 F1-F5 与本报告收口，本节全部数字为 2026-11-20 报告员会话复跑实测，留档回显面逐条标注来源批次）
- 依据：`docs/rip3/wsw/WSW-3-TASK.md`（硬验收 §四）+ `docs/rip3/wsw/WSW-3-ADR.md`（W3-D1～D8）+ `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md` §1.2/§3.2/§3.4 + WSW-2-ADR W2-D4/D5/D9 遗留授权面
- 状态：**本地定向门禁全绿且修订批后复跑实测（wsw-runtime 108 项 / clippy -D warnings / deny licenses / lock 399 = HEAD / reqwest 0 / web build+test 124 项 / e2e 32/32 exit 0）；CI 三连绿（快照 44645109640 / 收口批 44648573212 / 追加批 44759852926 均 completed success）；评审修订批 F1-F5（policy 帽消费接线 / Location 相对引用归一化 / cancel 滞留显性化 / completed_at 语义 / COUNT 前瞻登记）已提交 `6aa95bb6` 并复绿——**尚未推送（origin/main 停在 `a6aea6a4`，`origin/main..HEAD` = 1，实测）**，CI 未覆盖 `6aa95bb6`，推送后需 watch 回填（坏消息单列 G17）**

## A. 本批完成内容

1. **025 exchange 持久化**（`migrations/025_wsw_exchange.sql`，纯新增四表零 ALTER）：`wsw_exchange`（单行纪律：一 exchange 一行，响应未到 = 响应列 NULL + parse_status='partial'；Body 大内容不入行，行内只落 CAS BodyRef 派生列 sha256/uri/len/mime/fidelity）+ `wsw_repeater_tab` / `wsw_repeater_tab_version`（append-only：UNIQUE(tab_id, version_no)，无 UPDATE/DELETE 路径）+ `wsw_repeater_send`（六态词表 CHECK）；schema_version(25) + `PRAGMA user_version=25` 双写；列 CHECK 词表逐列（direction/source/protocol/scope_decision 三态/capture_fidelity/parse_status/origin/state）。链头 024→025 单文件推进。
2. **capture→落库异步面**（`listener.rs` + `listener/handler.rs`，W3-D2）：请求入口 snapshot → 响应流末 join Exchange → 有界 `tokio::sync::mpsc::channel(2048)`（handler.rs:349 实勘）→ `try_send` 满载丢行 + `persist_dropped_total` 原子计数 + tracing_error 结构化日志（handler.rs:388-402，转发热路径零阻塞）→ 单 worker 批量事务落行（≤128 行或 50ms 窗口 flush，listener.rs:285/312 实勘）+ 10s 超龄 partial 行清扫兜底；`PersistStore::open_file` 内 `enable_wal`（WAL 读写并发 ro 可见实测 exchange_persist.rs:267）；落库失败放行（W2 先例语义延续，listener.rs:337-372 salvage 计数）；CaptureMeta 扩展（method/scheme/authority/path/头投影/sequence_no 原子计数/started_at/remote_ip/scope_decision）；行内 capture_fidelity='degraded_no_wire' 如实落列（W2-D9 实态）。
3. **Repeater 发送引擎**（`src/sender.rs` 新建 + `bin/rp-wsw.rs` send 子命令，W3-D4）：`wsw_message::serialize::serialize_request`（Outbound 唯一事实）→ 原始 h1 字节写（TcpStream / tokio-rustls，hyper 写路径否决 D4.3）→ `parse_wire` 读回；重定向 none/follow(max_redirects=10，sender.rs:96 实勘) 逐跳 re-serialize + `scope::check_redirect_hop` 帽接线（sender.rs:759，W2-G10 遗留收口；**跳数帽生效面 = min(SendPolicy.max_redirects, ScopeRules.max_redirects) 双源取 min——--policy JSON 只可收紧，scope rules 安全帽兜底不放大，修订批 F1 接线 sender.rs:739-745**）+ 每跳查点 C IP 审计；Location 形态全谱（绝对 URL / protocol-relative / absolute-path / relative-path——relative 面 RFC 3986 §5.2 merge + §5.2.4 remove_dot_segments 归一化，修订批 F2 修双斜杠/`../`/`//host` 误并三面，join_reference/remove_dot_segments sender.rs:652-710）；connect/read/total 三超时；SIGTERM/SIGINT → cancelled 终态；发送历史 = wsw_repeater_send 行 + exchange(source=repeater) 行 + outbound bytes CAS（逐字节重发依据）；scope 规则自 wsw_session 行读，缺行 = default deny。
4. **History API**（`apps/server/rp_server.py` +885 行，W3-D3/D7）：GET /api/wsw/history 游标分页（(started_at DESC, exchange_id DESC) + base64url cursor + 过滤指纹 sha256 前 16 hex 守卫 → 400 filter_cursor_mismatch；limit 1..500 默认 100（WSW_LIST_LIMIT_CAP=500，rp_server.py:3339 实勘），越界 400 limit_exceeds_cap；禁 OFFSET）+ 过滤（authority/method/path/status/mime/source/tags/from/to）+ q 有界 search_text LIKE；GET /history/{id} 全元数据 + headers JSON（列表投影禁选 headers_json/notes/search_text 大列，实测列表行字段清单零禁选列）；GET /history/{id}/body 按需（单响应帽 8MiB=WSW_HISTORY_BODY_CAP rp_server.py:3338、超帽 413 body_too_large rp_server.py:3615 实勘，x-wsw-body-sha256 校验头）；PATCH /history/{id} 只收 tags/notes；Repeater API（tabs/tabs/{id}/request PUT=新版本行/tab patch/close/restore/send-to-repeater/POST send 子进程 spawn/GET sends?tab_id=/GET send-status）；读面 mode=ro + query_only（`_wsw_ro_conn` :3358）、tags/notes/tab/version 小控制行 Python 直写 busy_timeout 5s（写权分治 W3-D7）；Bearer 鉴权同既有面（未鉴权 401 实测）。
5. **History 页 UI**（`web/src/routes/WswHistoryPage.tsx` 新建）：过滤构建器（10 维 + q，防抖 300ms）+ 自研窗口化 VirtualList（`components/VirtualList.tsx`，固定行高 + overscan，零新依赖）+ 详情六视图（Raw/Pretty/Headers/Params/Body/Hex，`components/MessageViews.tsx`）+ Body 按需加载按钮 + Send to Repeater 行操作；websec 工作区 order 12 两 tab（workspaces.ts:326-344）+ App.tsx 两路由；路由钉值 20→22（workspaces.test.ts:109，设计内扩展如实记录）。
6. **Repeater 页 UI**（`web/src/routes/WswRepeaterPage.tsx` 新建）：多 Tab（新建/重命名/关闭/版本历史回退 restore）+ Raw/结构化双模编辑器（textarea 零新依赖，W3-D5）+ Send/Cancel（发送中态 + 六态徽标）+ 响应视图 + 版本行 append-only 展示；版本词表归一（raw 请求行 HTTP/1.1 → http/1.1，`normalizeVersionToken`，修复见 G8）。
7. **e2e 全集**（`web/scripts/rip3_wsw3_browser_e2e.mjs`）：≥1000 种子经代理（1101 实测）+ 虚拟滚动仅视口渲染（行节点 17 ≤80）+ 过滤防抖（5 键入 → 1 请求）+ Body 按需（列表零 /body、详情恰一次）+ 详情视图切换（Headers/Pretty/Raw 互斥）+ Send to Repeater 联动（origin='from_history' + source_exchange_id API 核验）+ 发送往返（marker + send succeeded + source=repeater 行 + send-status 一致）+ 多 Tab + 编辑发送（method/path/body 改写 API 核验）+ 版本回退（origin=restore append-only）+ 脱敏展示投影（红线 10）+ 性能硬门四项 + 截图 7 张；exit 1 模式（失败断言即 exit 1，全绿 exit 0 实测）。
8. **脱敏展示投影挂钩（追加批落地，红线 10 实做面）**：`wsw_message::redact`（redact.rs:55 唯一实现）此前仅存在函数与单测，展示链路零消费点——本批落 `rp-wsw redact-headers` 子命令（stdin 头数组 → stdout 掩码投影 + redacted_header_names；bin 单测 1 项）+ rp_server 详情投影 `_wsw_display_redact`（子进程消费；**fail-closed**：子进程失败/超时 → 全部头值 `[REDACTED-UNAVAILABLE]`，凭据绝不以明文进展示层）。原始行面不动（DB request_headers_json 保持原样），API/UI 只见掩码面。头名大小写实态：MITM Parsed 视图 hyper 归一小写（x-custom-keep），值原样保留。
9. **评审修订批（F1-F5，本批收口面）**：① F1 policy 帽死配置收口为消费面（sender.rs:739-745 redirect_cap = policy × rules 取 min，hop 闸 sender.rs:752-760 产出 `ScopeRejection::RedirectLimit{hop, cap}`——policy 只可收紧、rules 兜底不放大，document_fetch.rs:757 MIN(cap) 同型）；② F2 Location 相对引用归一化（join_reference = RFC 3986 §5.2.3 merge + remove_dot_segments = §5.2.4 两缓冲原算法；protocol-relative `//host` 显式 host 切换继承 scheme；修双斜杠/`../` 字面/protocol-relative 误并三形态）；③ F3 cancel SIGKILL 滞留缺口显性化（send-status 读面 stale 检测只读提示，Python 不代写 send 行守 W3-D7 分治，rp_server.py `_wsw_send_status`/`_wsw_send_cancel`）；④ F4 partial 行 completed_at 语义 = 行定型时刻（ADR W3-D2.1 文本收口 + handler.rs build_row doc 同步，实现不变）；⑤ F5 history COUNT 前瞻面登记（DESIGN §3.4，retention 批收口）。新增测试 6 条（sender_send.rs 8→14）。

## B. 架构变化和复用说明

- **不新建 crate**（W2-D1 边界延续）：sender.rs/persist 扩展/listener 扩展全落 `crates/wsw-runtime`；wsw-message 只消费公开面（parse/serialize/cas/redact/exchange/ids/store）零语义改动（`git status` 实核：`crates/wsw-message/**` 无变更——快照批对 store.rs 与 cas_e2e.rs 的改动属 025 钉值跟随面，语义为消费侧适配）。
- **发送路径 hyper client 否决**（W3-D4.3）：HeaderMap 迭代序 ≠ 编辑头序——sender.rs 用原始 h1 字节写（TcpStream/tokio-rustls），serialize_request 是发送字节唯一来源（红线 9）；tokio-rustls 0.26.6 已直依赖（Cargo.toml:66），零新依赖。
- **写权分治**（W3-D7）：exchange 行 Rust-only（listener worker 批量 + rp-wsw send）；tags/notes/tab/tab_version 小控制行 rp_server Python 直写；读面全 mode=ro。修订批 F3 坚守该边界：cancel 滞留面只做只读 stale 提示，不代写 send 行。
- **UI 零新 npm 依赖**（W3-D5）：VirtualList 自研（固定行高 + scrollTop 换算 + overscan）、编辑器 textarea、详情六视图抽自 ResearchEvidencePage 雏形；`web/package.json` 零变更。
- **复用**：`database::Pool::enable_wal`（pool.rs:258 P1-1 先例）、023 `wsw_body_blob` + `cas.rs`（Body 按需读回）、024 水位/钉值先例（5d146e24 七文件钉值形态）、rp-wsw 子进程先例（rp_server Popen spawn）、`wsw_message::redact`（search_text 生成侧排除 Authorization/Cookie/Set-Cookie 头值，可搜头名不搜凭据值）、document_fetch `MIN(cap)` 双帽同型先例（document_fetch.rs:757）。

## C. 修改文件清单（实测逐文件）

证据源：`git show --stat 9aff134a`（快照批 34 文件 +7395/-84）+ `git show --stat 6aa95bb6`（修订批 14 文件 +356/-20 实测：8 源码/文档 + 7 截图二进制再生成）+ `git status --porcelain`（当前仅剩本报告）。

| 文件 | 快照/收口/追加批 | 修订批（`6aa95bb6` 实测） |
|---|---|---|
| `migrations/025_wsw_exchange.sql` | 新建（181 行四表+索引+双写） | 零改动 |
| `crates/wsw-runtime/src/persist.rs` | 修改（+768 四表 store 接口+WAL） | 零改动 |
| `crates/wsw-runtime/src/listener.rs` | 修改（+144 worker/计数器/enable_wal） | 零改动 |
| `crates/wsw-runtime/src/listener/handler.rs` | 修改（+712 CaptureMeta/snapshot/join/mpsc） | **修改（F4：build_row doc +3 行 completed_at 语义）** |
| `crates/wsw-runtime/src/sender.rs` | 新建（发送引擎） | **修改（F1/F2：+113 行——redirect_cap 双源 min 接线 + join_reference/remove_dot_segments + protocol-relative 分支 + 模块/字段 doc）** |
| `crates/wsw-runtime/tests/sender_send.rs` | 新建（8 测） | **修改（+205 行，8→14 测：相对段/../归一化/根基准零双斜杠/protocol-relative host 切换/policy 收紧/rules 兜底 6 条新增）** |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改（+255 send 子命令；追加批 +redact-headers） | 零改动 |
| `crates/wsw-runtime/tests/exchange_persist.rs` | 新建（11→12 测，+吞吐实测） | 零改动 |
| `apps/server/rp_server.py` | 修改（+885 History/Repeater API；追加批 +`_wsw_display_redact`） | **修改（F3：+45 行 send-status stale 只读检测 `_wsw_iso_age_seconds` + cancel docstring 缺口登记）** |
| `web/src/routes/WswHistoryPage.tsx` | 新建（602 行） | 零改动 |
| `web/src/routes/WswRepeaterPage.tsx` | 修改（normalizeVersionToken 版本词表归一） | 零改动 |
| `web/src/components/MessageViews.tsx` / `VirtualList.tsx` | 新建（258 行六视图 / 83 行窗口化列表） | 零改动 |
| `web/src/workspaces.ts` / `App.tsx` / `components/Layout.tsx` | 修改（websec order 12 两 tab/两路由） | 零改动 |
| `web/tests/workspaces.test.ts` / `shell-smoke.test.tsx` | 修改（路由钉值 20→22 如实记录） | 零改动 |
| `web/scripts/rip3_wsw3_browser_e2e.mjs` | 修改（种子逐请求 socket/waitFor falsy/waitMs async；21→32 断言） | 零改动 |
| 迁移钉值跟随（11 文件） | 修改（agent-engine×2 + database×5 + plugin-runtime + workflow-engine + wsw-message store/cas_e2e + wsw-runtime intercept_queue） | 零改动 |
| `scripts/local_range.py` | 修改（allow_reuse_address=True，G12） | 零改动 |
| `docs/rip3/wsw/WSW-3-ADR.md` | 追加批 doc | **修改（F1：W3-D4.4 双源 min 生效面；F4：W3-D2.1 completed_at 语义改写）** |
| `docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md` | — | **修改（F5：§3.4 已知前瞻面 COUNT(*) + cancel sweeper 收口登记）** |
| `docs/rip3/batches/browser-e2e/wsw-3/*.png` | 新建 7 张 | **7 张全量再生成（报告员会话 e2e 复跑实拍，23:45）** |
| `docs/rip3/batches/WSW-3-BATCH-REPORT.md` | 新建 | **修改（A.3/A.4 行号勘误 + E.1 账目 + E2 回填 + F 复跑数 + G 编号修正与 G17）** |
| `Cargo.lock` / `vendor/**` / workspace `Cargo.toml` | 零变更 | 零变更（`git diff HEAD -- Cargo.lock` 空、`git status` 无 vendor 面，实勘） |

## D. 数据库迁移、Schema 和兼容性变化

- **025 链头推进**：`migrations/025_wsw_exchange.sql` 纯新增四表 + 6 索引（UNIQUE(listener_id,sequence_no) + cursor/authority/method/status/source/started 游标检索族）+ `schema_version` INSERT OR IGNORE (25,…) + `PRAGMA user_version = 25` 双写（024 :87-93 同款）；零 ALTER 零历史迁移改动（本会话实勘：`git status --porcelain migrations/` 0 文件、`git diff HEAD --stat -- migrations/` 空）。
- **水位实测（本会话 2026-11-20 只读复查）**：dev-b1=**25**、dev-p13=**25**（链尾，tool-registry 测试重建承载）；rip-data/wsw-e2e-3.db=25（exchange_rows=1124，e2e 复跑累积）；platform*.db = **1/1/1/0/0 反证不动**（python3 sqlite3 ro 直查五库 PRAGMA user_version 实测）；dev-b2/dev-b5=24/dev-p7=12/dev-p8=18（非本批消费面副本未推进，如实登记）。
- **生产库零触碰**：platform*.db 五库 `git status` 零变更（内容 = git 已提交态）；023 文件头 :30 纪律遵守；025 只在测试内存库与 dev 副本执行。
- **迁移钉值 24→25 全跟**（S2 实勘，模式扫描不预设文件数）：`grep -rln "1, 2, …, 24, 25" crates/` 五文件 vec 链清单（migration_idempotent / migrations_agent_data_plane / migrations_tool_contract / migrations_context_broker / migrations_runtime_node）+ `Some(25)`/`user_version must be 25`/`must land 025`/`025_wsw_exchange` 链尾断言族（backup_restore 等）+ store.rs/lifecycle_full_path/migration_replay/cas_e2e/intercept_queue 消费面适配共 11 文件；`Some(24)/must land 024/user_version must be 24` 残留扫描零命中。套件实跑全绿见 E.1b（本会话复跑复证）。

## E. 测试命令及真实结果

### E.1 定向门禁（WSW-3-TASK §4.1；2026-11-20 报告员会话修订批后全部复跑实测）

```
$ grep -cE '^\[\[package\]\]' Cargo.lock        → 399
$ git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'   → 399（HEAD 对比基准一致）
$ git diff HEAD -- Cargo.lock | wc -l           → 0（lock 零 diff）
$ grep -c 'name = "reqwest"' Cargo.lock         → 0
$ cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
                                                → Finished（零警告零错误）
$ cargo deny check licenses                     → licenses ok
$ cargo test --offline -p wsw-runtime -- --test-threads=1
  unittests src/lib.rs            49 passed
  unittests src/bin/rp-wsw.rs      1 passed    （redact-headers 脱敏投影单测）
  tests/authority_ca.rs            5 passed
  tests/exchange_persist.rs       12 passed   （快照 11 + 吞吐实测 1）
  tests/intercept_queue.rs         7 passed
  tests/mitm_loopback.rs           6 passed
  tests/scope_gate.rs             14 passed
  tests/sender_send.rs            14 passed   （修订批 +6：相对段/../归一化/根基准零双斜杠/protocol-relative/policy 收紧/rules 兜底）
  合计 108 passed / 0 failed（收口批时点为 102，修订批 sender_send.rs 8→14）
$ npm --prefix web run build                    → ✓ built in 763ms
$ npm --prefix web run test                     → Test Files 19 passed (19) / Tests 124 passed (124)
$ cd web && node scripts/rip3_wsw3_browser_e2e.mjs   → **32/32 PASS，exit=0**（断言回显见 E.4）
```

### E.1b 迁移钉值与依赖面套件（同会话复跑实测）

```
$ cargo test --offline -p database --test migrations_agent_data_plane --test migrations_context_broker
    --test migrations_runtime_node --test migrations_tool_contract --test backup_restore
  → 1+2+2+2+4 = 11 全部 ok
$ cargo test --offline -p agent-engine --test migration_dev_copy_p7_3 --test migration_idempotent
  → 1+1 ok
$ cargo test --offline -p plugin-runtime --test lifecycle_full_path → 6 ok
$ cargo test --offline -p workflow-engine --test migration_replay → 3 ok
$ cargo test --offline -p tool-registry
  → 19+2+3+11+7 = 42 ok（该套件重建 dev-b1 至链尾 25；收口批报告转写 19+2+3+11=35 漏计 sk3_tool_manifests 7 项，本会话补全账目）
$ cargo test --offline -p wsw-message           → unittests 70 + cas_e2e 2 + parse_conformance 3 = 75 ok
```

注：`wsw-message cas_e2e` 首轮 1 失败（cas_e2e.rs:128 断言 dev-b1=25 而实况 24）为 S2 步骤"dev-b1 副本 025 推进"未执行的暴露面——该断言语义 = dev 副本应达链尾（tool-registry 测试重建承载），跑 tool-registry 重建后复跑即绿，非代码缺陷。

### E.2 游标协议 curl 实测（收口批 S5 门，脚本 /tmp/wsw3-curl-evidence.sh 留档回显；修订批未触碰 History API 面本会话未复跑）

```
== total=1100 ==
E1 首页 limit=500：rows: 500 has_more: True total: 1100 next_cursor_len: 123
   row0 keys 全投影；forbidden big columns in list: NONE（列表零 headers_json/notes/search_text/body 引用列）
E2 分页遍历（E1 cursor 起步）：pages: 2 rows walked: 600 unique: 600 dups: 0
   全链路零重零漏专项：pages=3 walked=1100 unique=1100 dups=0 → ZERO-DUP-FULL-COVERAGE PASS (1100/1100)
E3 过滤变更后用旧 cursor：http=400 {"error":{"code":"filter_cursor_mismatch","message":"过滤参数与 cursor 签发时不一致（漏行/重行防线，D3 指纹守卫）"}}
E4 limit 越界：limit=501 → http=400 {"code":"limit_exceeds_cap","message":"limit 越界（1..500）：501"}
              limit=0   → http=400 {"code":"limit_exceeds_cap","message":"limit 越界（1..500）：0"}
E5 末页：last page rows: 100 next_cursor: '' has_more: False（空 cursor = 末页协议）
E6 详情端点：detail 43 键含 headers JSON 与 body 引用列、零 body 字节字段；
   PATCH tags/notes → http=200 "patched": true；body 按需 http=200 len=532 sha-header=cb93fa56…
E7 搜索面：q=p59c-marker hits: 1100；search_text payload 命中 163；credential value leaks: 0
   （search_text LIKE '%Bearer wsw3-e2e-token%' 零命中——凭据头值不入搜索投影）
E8 未鉴权 /api/wsw/history → http=401
```

### E.2b 实拍与联动验收独立证据（追加批，/tmp/wsw3-live-evidence.sh 留档回显）

```
L1 curl 循环经代理 1000 条（for i in $(seq 1 1000); do curl -s -o /dev/null -w '%{http_code} '
   -x 127.0.0.1:13128 http://127.0.0.1:18443/seed/$i; done）：
   loop-seconds=7.2；codes-distribution: 404=1000 other=0（全量成功往返）
L2 落库断言：total=1000（≥1000 PASS）
L3 代理增量：direct median=0.61ms（20 样本全列）/ proxy median=1.52ms（20 样本全列）
   → INCREMENT=0.91ms（n=20 each，中位数差；门 <50ms PASS；落库链路在环）
L4 History 列表 API p95（1103 行库 limit=200，20 采样）：p95=8.8ms median=6.3ms min=5.8 max=22.8（门 <300ms PASS）
```

### E.3 Repeater append-only 活体断言（收口批 S7 门 curl 实测 + 本会话 e2e 复证）

```
$ POST /api/wsw/repeater/tabs（建行）→ tab_id；PUT request ×3 → http=200 ×3
$ GET /api/wsw/repeater/tabs/{id} → version rows: 4 version_nos: [4,3,2,1] origins: ['manual',…]
  （建行种子 v1 + 三次 PUT 各追加一行 v2/v3/v4，零覆盖）
  PATCH /tabs/{id}/version/1 → http=405（无该路由）
  rp_server.py PATCH/DELETE × version 路由扫描：NONE（append-only 面无端点）
本会话 e2e 复证：回退 v1 → origin=restore 新行，版本行 2→3（E.4 回显）
```

### E.4 e2e 断言面（32 项全集，2026-11-20 报告员会话 23:45 复跑回显原文）

```
[PASS] 种子流量与落库 :: total=1101 seed=3421ms
[PASS] 时序：History 首屏 <3s :: 457ms
[PASS] 虚拟滚动：仅视口渲染 :: 行节点=17；滚动后行节点更换（复用窗口）:: 行节点=22
[PASS] 过滤防抖：5 次快速键入 → 请求 ≤2 :: 请求数=1
[PASS] 时序：详情交互响应 <1s :: 21ms
[PASS] Body 按需：详情打开零 /body + 按钮恰一次 :: 详情打开时=0 加载后=1
[PASS] 详情视图切换：Headers 表格渲染 / Pretty 互斥 / 回 Raw
[PASS] Send to Repeater：跳转 /websec/repeater + 编辑器含 method/path
[PASS] Send to Repeater：origin=from_history + source_exchange_id（API 核验）
[PASS] Repeater 发送往返：响应 marker + send 行 succeeded + History source=repeater 行
[PASS] send-status 端点：六态 succeeded + exchange 引用一致
[PASS] Repeater 多 Tab：新建后 tabbar +1 :: tabs=1→2
[PASS] Repeater 编辑发送：改 method/path/body :: latest_version_no=2 method=POST target=/p59c-marker body=RIP3-EDIT-BODY-E2E
[PASS] Tab 修改历史保留：回退 v1 → 新 origin=restore 行（append-only 不覆盖）:: 回退前 2 行 → 回退后 3 行
[PASS] 脱敏展示：Authorization/Cookie 头值 = 掩码（UI 详情侧）:: 凭据明文零出现=true
[PASS] 脱敏展示：非命中头值原样保留 :: keep-me=true 头名小写在场=true
[PASS] 脱敏展示：API 投影掩码 + redacted_header_names 在场 :: names=["authorization","cookie"]
[PASS] 时序：API history p95 <300ms :: p95=8.1ms median=6.9ms n=20
[PASS] 时序：API history/{id}/body p95 <300ms :: p95=10.6ms median=9.8ms n=20
[PASS] 时序：API repeater/send-status p95 <300ms :: p95=3.0ms median=2.6ms n=20
[PASS] 时序：API repeater/tabs/{id} p95 <300ms :: p95=3.1ms median=2.3ms n=20
[PASS] 时序：代理增量 <50ms（落库链路在环）:: direct=0.67ms proxy=1.89ms 增量=1.22ms（n=21）
32/32 PASS，exit=0
```

## E2. CI 状态

- 快照基线 `9aff134a`：run **44645109640 completed success**（6m11s）。
- 收口批 `36638fe5`：run **44648573212 completed success**（5m42s）。
- 追加批 `a6aea6a4`：run **44759852926 completed success**（5m36s；`gh run view 44759852926` 本会话实测回显：`{"conclusion":"success","headSha":"a6aea6a4e3e93ea7bcac55a57d7a6b219ec875c3","status":"completed","createdAt":"2026-11-20T15:05:55Z"}`）。
- **修订批 F1-F5 已提交 `6aa95bb6`（2026-11-20 23:45:56，14 文件 +356/-20，`git show --stat` 实测）但尚未推送**：origin/main = `a6aea6a4`、`git log --oneline origin/main..HEAD` = 1（本会话实测）——**CI 未覆盖 `6aa95bb6`**；本地门禁已在修订后代码上复跑全绿（E.1/E.1b/E.4）。推送后按红线八步.6 watch 至终态并回填本节（下一动作，见 G17/H）。

## F. 性能数据（全部数字来自真实命令回显，来源逐条标注）

**e2e 性能硬门（`web/scripts/rip3_wsw3_browser_e2e.mjs` 内建断言，任一超阈值 exit 1；来源 = 2026-11-20 报告员会话 23:45 复跑实测）：**

| 断言 | 阈值 | 实测 |
|---|---|---|
| History 首屏（1101 行库，列表行在屏） | <3s | **457 ms** |
| 虚拟滚动 DOM 行节点 | ≤80 | 17（滚动后 22，复用窗口） |
| 过滤防抖（5 次快速键入） | /api/wsw/history 请求 ≤2 | **1 次** |
| 详情交互（行点击→详情面板） | <1s | **21 ms** |
| Body 按需 | 列表零 /body + 详情恰一次 | 0 → 1 |
| API /api/wsw/history p95（n=20） | <300ms | **8.1 ms**（median 6.9） |
| API /api/wsw/history/{id}/body p95（n=20） | <300ms | **10.6 ms**（median 9.8） |
| API /api/wsw/repeater/send-status p95（n=20） | <300ms | **3.0 ms**（median 2.6） |
| API /api/wsw/repeater/tabs/{id} p95（n=20） | <300ms | **3.1 ms**（median 2.3） |
| 代理增量（直连 vs 经代理 curl time_total 中位数差，n=21，**落库链路在环**） | <50ms | **+1.22 ms**（direct 0.67 / proxy 1.89） |

**独立实拍交叉面（追加批 L3/L4 留档回显，非 e2e 脚本内）**：代理增量 **+0.91ms**（n=20 each 中位数差，direct 0.61 / proxy 1.52）+ History API p95 **8.8ms**（n=20，median 6.3，max 22.8）——两组独立命令同向印证，门 <50ms/<300ms 均 PASS。

**落库吞吐实测**（来源 = 收口批实测命令留档 `cargo test -p wsw-runtime --test exchange_persist insert_batch_throughput_measurement -- --test-threads=1 --nocapture` 三次采样；临时文件库 WAL 同 listener 实态，128 行/批 × 40 批 = 5120 行；该测试本会话随套件复跑通过 12/12，未重取吞吐采样）：

```
insert throughput: 10091 rows/s (5120 rows in 507.4ms, 40 batches x 128 rows, file WAL db)
insert throughput: 10334 rows/s (5120 rows in 495.4ms, 40 batches x 128 rows, file WAL db)
insert throughput:  9919 rows/s (5120 rows in 516.2ms, 40 batches x 128 rows, file WAL db)
```

中位 ≈ 10091 rows/s（单 worker 批量事务面）。端到端参照：e2e 种子 1101 请求经代理 3.42s 全部落库（capture 率 ≈322 行/s，瓶颈在种子逐请求往返而非 worker——吞吐余量 ~30×）。

**种子落库收敛观测**（收口批专项脚本双路观测 api/直查库每 100ms 留档）：串行种子 1100 结束后 t=28ms api=1087、t=150ms 收敛 1100，全程 api==db 零分歧——批量 flush 窗口（≤128 行或 50ms）生效，读侧无快照滞后。

**PLAN §11.6 其余项如实登记缺测**：并发连接峰值/大 Body 内存峰值/长跑稳定性不可达面未测（无压测夹具，未跑即不填数；本批并发种子仅 1100 单连接序列 + 8 并发复现脚本，非并发压测口径）。

## G. 未解决问题与风险（坏消息单列）

1. **fidelity 债（W3-D6.2 否决备选承接）**：捕获面 body-only wire（W2-D9 实态）→ 请求头区无原始 wire 字节，History→Repeater 重建 = 编辑态重建语义，`capture_fidelity='degraded_no_wire'` 如实落列 + UI 徽标明示"重建自 History：头序/绝对形式不保证逐字节"；**逐字节重发面只对 source=repeater 的发送开放**（outbound bytes 已 CAS）。捕获期全量 tee 请求 wire 不做（流式转发冲突），债保留至后续批裁决。
2. **retention 执行器遗留（DESIGN §3.4 查询面落本批）**：025 只落行数/blob 计数查询面（`_wsw_history_stats`）；超帽 LRU 淘汰执行器与 blob GC 执行位留 retention 批（023 注释口径延续，如实登记不冒充收口）。
3. **FTS5 显式缓议（W3-D7）**：bundled SQLite 有 FTS5（vendor/libsqlite3-sys/build.rs:177）但 CJK tokenizer 缺省不可用 + 索引维护进批量写路径；搜索 = search_text（≤8KiB 有界投影）LIKE。实测 p95 8.1ms << 300ms 门——升级预案保持缓议，连续两轮 p95 红才另立裁决。
4. **重定向帽的代理转发面承接不变**：W2 每跳经代理即新请求复走 scope 闸的保障面保持；sender.rs 的 check_redirect_hop + F1 双源 min 帽接线覆盖 **Repeater 主动发送面**，代理被动转发面跳数追踪仍无（W2-G10 原登记的运行时跳数接线以 Repeater 面收口，被动面如实保留）。
5. **拦截超时自动放行 / tee trailer 丢弃 / capture 超帽不落 CAS**：WSW-2 G2/G3/G4 遗留原样延续，本批未触碰。
6. **token 面未落地**：WSW-2 G1"本机访问令牌"跟踪项原排 WSW-3——本批 API 面沿用既有 Bearer（TOKEN 环境变量）未新增独立代理令牌方案，跟踪项**保持开放**（e2e 用 TOKEN 变量即 Bearer 面），下一批或安全批收口。
7. **种子流量定界形态（e2e 基础面修复）**：local_range.py 为 `BaseHTTPRequestHandler` 默认 protocol_version="HTTP/1.0"（scripts/local_range.py:35）→ 上游每响应关连接、经代理转发为 HTTP/1.0 无 CL/无 chunked（EOF 定界）。快照版 e2e 单连接 keep-alive 种子模式在此形态下第二请求必超时（实测复现：第一请求完整捕获、socket close 事件后 8s 超时）。修复 = 逐请求 socket（curl -x 同型）+ CL/chunked 快路径 + EOF 兜底；分支注释固化实测依据。
8. **Repeater 版本词表归一（UI 修复）**：`HttpVersion` 词表为小写（types.rs:146-155 from_str_vocabulary），raw 请求行 "HTTP/1.1" 原样入 OutboundRequest JSON → serde 拒收（`[rp-wsw] error: unknown http version: HTTP/1.1`）→ send 卡 pending。修复 = normalizeVersionToken（HTTP/0.9/1.0/1.1/2/3 → 词表形，未知回 http/1.1）；e2e 发送往返断言由 pending 转 succeeded 实证。
9. **rp-wsw send 预飞失败落终态（Rust 修复）**：此前 request-json/policy/rules 解析失败直接 return Err，send 行永久滞留 pending（六态不收敛）；修复 = 预飞段失败经 persist_send_failure 落 `state='failed'` + error_code（request_invalid/preflight_failed）再退出（rp-wsw.rs:404-458）。
10. **e2e waitFor 真值字符串坑（测试面）**：种子落库等待条件未达标时返回 `total=N` 非空字符串 → truthy → waitFor 首轮即解析，实际 total 从未达标（两轮 1085/1095 假象由此）。修复 = 未达标返回 falsy；配合 waitMs 修复后种子断言实值 1100 全绿。
11. **e2e waitMs 阻塞事件环坑（测试面）**：`Atomics.wait` 阻塞 Node 主线程——page.on('request') 的 CDP 事件在阻塞期入队、`page.off` 先执行 → 过滤防抖计数恒 0（两轮实测复现）。修复 = waitMs 改 async sleep（事件环继续转）；修复后防抖计数=1 实证。
12. **local_range.py REUSEADDR（基础设施面，任务书清单外改动如实登记）**：`socketserver.ThreadingTCPServer` 默认 allow_reuse_address=False，HTTP/1.0 服务端先关连接的子连接 TIME_WAIT 挂 18443 达 2×MSL → 60s 内重绑 EADDRINUSE（e2e 快速重跑/复现脚本三连踩实测）。修复 = 子类开 allow_reuse_address=True（消除 TIME_WAIT 重绑阻塞，不改监听语义；`git diff scripts/local_range.py` 一类一实例化两处）。首次改写误将 do_GET/log_message 挂入新类（实测 501 Unsupported method 'GET' 暴露），整文件重写修正——过程如实记录。
13. **e2e 首轮连败如实记录（同法两次纪律）**：种子面失败 2 次（逐请求 socket 修复后 total 假象 2 次）→ 第 3 次换根因面（waitFor/waitMs 事件环）修复后全绿；每步根因有实测回显（G7-G11），未谎报。
14. **追加批 e2e 教训**：① 真值字符串坑（G10）再犯——新写的过滤生效 waitFor 非匹配分支返回模板字符串被 truthy 首轮解析，导致点击了未过滤首行（repeater POST）而非凭据行，两轮假阴性后诊断行（credRowId vs detailShown 对照）定位，修为布尔返回；② 断言读取序列化形状要先看 API 投影——版本行 latest_request_json 的 body 为 base64 内联、头名 hyper 归一小写，两处断言先误后修；③ e2e 从错误 cwd（web/ 外）运行报 MODULE_NOT_FOUND 属环境面非断言面，如实区分。
15. **cancel SIGKILL 兜底的 send 行滞留缺口（F3 显性化，六态收敛边缘面）**：cancel 端点 SIGTERM 后 5s（50×0.1s）未死即 SIGKILL（rp_server.py `_wsw_send_cancel`）——SIGKILL 路径 rp-wsw 子进程无机会执行 cancelled 落库，send 行永久滞留 pending/sending；advance_send 的写权面 = 子进程（W3-D7 send 行 Rust-only），行不可再推进。**已落显性化面（F3）**：send-status 读面 stale 检测（pending/sending + pid 死/缺且行龄 >30s → `stale:true` + stale_note，只读不写，Python 不代写 send 行守分治；功能实测四分支：A 老行+pid 缺→stale=true、B 新行+pid 缺→无误报、C pid=4000000 必死→stale=true、D 终态行→不检测）；**滞留行推进 sweeper/对账留 retention 批**（DESIGN §3.4 F5 同点登记收口）。
16. **修订批其余面**：① F1 跳数帽双源 min 接线（policy 帽此前定义未消费静默失效——SendPolicy.max_redirects 字段随 --policy JSON 可配但 sender 内零消费点，A.3/ADR W3-D4.4 同步）；② F2 Location 相对引用归一化（相对引用分支原拼接产 `//a/bar` 双斜杠、`../` 字面透传、protocol-relative 误并本 host——rewrite 后 sender_send.rs 新增 6 测全绿）；③ F4 partial 行 completed_at 语义 = 行定型时刻（ADR W3-D2.1 文本收口，实现不变——capture 面 fidelity 词表无法区分超帽截断与连接中断，无响应/中断行不另造 NULL 时刻语义）。
17. **修订批已提交未推送——CI 覆盖缺口（本报告收口时点坏消息）**：F1-F5（sender.rs/sender_send.rs/handler.rs/rp_server.py/ADR/DESIGN）+ 截图 7 张再生成已提交 `6aa95bb6`（14 文件 +356/-20），但 origin/main 停在 `a6aea6a4`（`origin/main..HEAD` = 1 实测），**CI 尚未覆盖 `6aa95bb6`**；本地门禁复跑全绿（E.1/E.1b/E.4）。推送 + CI watch 回填完成后方满足 H 节下一批开工前提；批次报告本身亦未提交（工作树唯一 dirty 文件）。

## H. 下一批依赖和明确的非目标

- **非目标不变**（任务书 §一）：Fuzzer（W4）、WebSocket Workbench（W5）、Decoder/Comparer/Passive Audit（W6）、HAR 导入导出、retention 执行器（含 cancel 滞留行 sweeper）、FTS5、捕获期全量 wire tee。
- **留 W4**：CodeMirror/monaco 选型复审（W3-D5 登记开放项，Fuzzer 高亮需求更强时一并走依赖审计）；fuzz 面可用 search_text 投影与 scope 词表直接复用。
- **留下一批/安全批**：本机访问令牌（G6 跟踪项保持开放）。
- **下一批开工前提**：`6aa95bb6`（修订批）+ 批次报告推送后 CI 终态绿（REDLINES 红线 5，见 G17/E2）。
- **WSW-4 或 RT-2 可直接复用面**：025 exchange 表与游标协议（Fuzzer 观测落库）、sender.rs 序列化/帽子弹（Fuzz 请求生成）、search_text 投影（fuzz payload 命中面）、scope 词表与 check_redirect_hop（生成面合法性闸）。

## I. UI 截图与浏览器验收证据

**7 张**实拍入库 `docs/rip3/batches/browser-e2e/wsw-3/`（文件存在且非空，`ls -la` 本会话实测 2026-11-20 23:45 再生成：243007/285981/160529/152292/163037/171680/191573 字节，e2e 32/32 复跑实拍）：

1. `01-history-list.png` — History 页 1100 行库首屏（虚拟滚动 17 行节点 + 过滤构建器 + total 计数）；
2. `02-history-detail-body.png` — 详情六视图 + Body 按需加载后（/body 恰一次）；
3. `03-repeater-from-history.png` — Send to Repeater 联动落 Repeater 页（编辑器含 method/path + from_history 版本）；
4. `04-repeater-send-roundtrip.png` — Repeater 发送往返（响应视图 marker + succeeded 徽标）；
5. `05-repeater-multitab-send.png` — 多 Tab + 编辑请求（method=POST/path/body）发送往返响应视图；
6. `06-repeater-version-restore.png` — 版本历史回退（v1 → origin=restore 新行）；
7. `07-history-redacted-detail.png` — 凭据面 exchange 详情 Headers 视图脱敏（authorization/cookie=[REDACTED]、x-custom-keep 原样）。

## J. RTIS 对账和数据安全回归结果

- **RTIS 零接触**：`crates/intel-db/**` 与全部 RTIS 面本批零文件（`git status --porcelain | grep -cE 'intel-db|rtis'` → **0**，本会话实测）；漏洞事实读写面零改动；RTIS 引用只走既有 evidence 链（025 六 ID 引用列 trace/task/tool_run/artifact/evidence 均为既有体系 ID 形状，无第二套事实源）。
- **数据安全回归**：生产库 platform*.db 五库零触碰（D 节水位 1/1/1/0/0 + git 零变更实测）；`rip-data/` 运行时数据不入库（e2e DB/CA/status 全在 rip-data 下，.gitignore:35 覆盖）；凭据面 = search_text 排除 Authorization/Cookie/Set-Cookie 头值（E.2 E7 leaks:0 收口批实测）+ 展示投影挂 wsw_message::redact fail-closed（A.8）+ e2e 脱敏断言本会话复跑三连 PASS（E.4）+ 日志零头值体值（W2-D7 延续）。
- **红线 12 UI 纪律**：web/package.json 零变更（零新 npm 依赖）；两页无静态假数据（grep mock/fake/dummy 零命中，placeholder 属性除外）；深浅主题沿 RIP Token（daisyUI 类 + 既有组件范式）。

## K. 本批教训实录

1. **e2e 断言辅助函数本身可以是断言失败根因**：waitFor 未达标分支返回真值字符串、waitMs 用 Atomics.wait 阻塞事件环——两类"脚手架坑"各吃掉一轮修复；后续 e2e 编写先验：等待条件必须返回布尔、浏览器事件计数窗口内禁阻塞等待。
2. **本地靶场协议形态要先实测再写种子**：HTTP/1.0 EOF 定界形态（上游每响应关连接）决定了"单连接 keep-alive 种子"不可行——复现实验（node 逐请求 + EOF 兜底）比对照 W2 e2e 更快定位。
3. **serde 词表反序列化是边界敏感面**：UI 层自由文本 → Rust serde 强类型词表（HttpVersion）之间必须有显式归一层；错误回显（`unknown http version: HTTP/1.1 at line 1 column 65`）在子进程日志尾行，排查路径 = spawn 日志落盘面先看。
4. **状态机"禁 pending 悬挂"要覆盖引擎前失败面**：六态收敛设计只覆盖了引擎错误路径，预飞（解析/rules/policy）失败路径漏收敛——集成 e2e 走真实子进程链才暴露，纯单测面不可见。
5. **shared 基础设施（local_range）的隐式约束要在批内固化**：allow_reuse_address 缺省行为 + 服务端先关连接 = 重绑窗口，属于"谁写 e2e 谁踩"类环境约束；一行修复 + 注释固化实测依据，比每批绕 60s 更值。
6. **迁移钉值债务的实勘清单制有效**：五模式扫描（vec 链/Some(N)/must land N/user_version must be N/文件名引用）+ 不预设文件数，11 文件一次跟全，零 CI 爆区复发（对比 WSW-2 G12 五文件漏网教训）。
7. **死配置面（字段定义未消费）靠评审 + 消费点 grep 审计暴露**：SendPolicy.max_redirects 定义且可配但 sender 内零消费点，用户配置静默失效——F1 以双源 min 收口（可配面收紧 + 安全帽兜底不放大，document_fetch MIN(cap) 同型）；后续可配面验收加"字段→消费点"审计步骤。
8. **Location/相对引用解析必须走 RFC 3986 标准算法，不手拼字符串**：`format!("{}/{}/{}", base_dir, …)` 拼接产双斜杠、`../` 字面透传、protocol-relative 误并三形态一并发；`join_reference`（§5.2.3 merge）+ `remove_dot_segments`（§5.2.4 两缓冲）一次收全，6 条形态测试钉住。
9. **测试账目转写要在跑完当场核对**：收口批 tool-registry 计数漏计 sk3_tool_manifests 7 项（35 vs 42）——套件计数从 `test result:` 行逐行取，不凭印象补合计。
