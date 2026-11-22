# WSW-5+6 批次收口报告 · WebSocket Workbench + Decoder / Comparer / Passive Audit

- 批次：WSW-5+6 合并批（PLAN §九 :607/:608——WSW-5 交付 = 握手、Frame History、帧编辑和重放，不含 QUIC/HTTP3；WSW-6 交付 = 编解码、Diff、被动规则、候选发现，不含第二套扫描引擎；UI 不入批）
- 日期：2026-11-22
- 基线 HEAD：`aafb87e9`（DI-3/RT-4 收口；工作树仅任务书 `WSW-5-6-TASK.md` 未跟踪；workspace 全量 exit 0 / 1270 passed 零基线红实测）
- 收口方式：本地工作树实施与验证；未 commit、未 push（按批纪律留主会话审查收口）
- 任务书：[WSW-5-6-TASK.md](WSW-5-6-TASK.md)（§0 D1-D10 裁决 / §1 A1-A16 锚点 / §3 S1-S9 / §4 判据 1-12 / §6 八红线全程照办）
- 状态：**本地验收全绿**（workspace 全量零基线红延续 + 定向三 crate + selftest_wsw56 真实 WS 会话 42/42 + clippy -D warnings + deny 双检 + 迁移纪律；坏消息单列见 §E）

## A. 改动文件清单（路径 + 行数）

### A.1 新建文件

| 文件 | 行数 | 内容 |
|---|---|---|
| `migrations/030_wsw_ws_frame.sql` | 128 | D2 纯新增两表：`wsw_ws_frame`（frame_id UNIQUE 幂等键 wsw-fr- 前缀 + connection_exchange_id 引用 wsw_exchange 握手行 + direction 两词表 CHECK + sequence_no 连接内单调 UNIQUE(connection_exchange_id,sequence_no) 幂等锚 + opcode 五词表 CHECK（PLAN :276/:678 text/binary/ping/pong/close 逐字）+ payload_len + payload CAS 四派生列（025 Body 列同型）+ masked 方向词形承载 + captured_at + scope_decision 三态）+ `wsw_passive_hit`（hit_id wsw-pah- 前缀 + exchange_id/listener_id 引用 + rule_id CHECK 词表 14 规则逐字（passive.rs 注册表同源）+ rule_version + severity 四词表 CHECK + match_detail_json 8MiB 帽 + UNIQUE(exchange_id,rule_id) 幂等锚——**零 evidence_id 列零 Finding 提升**，PLAN :307-309/:504）+ 六索引 + schema_version(30) INSERT OR IGNORE + `PRAGMA user_version=30` 双写；全 CREATE IF NOT EXISTS 零 ALTER |
| `crates/wsw-runtime/src/wsframe.rs` | 152 | S3 帧类型面：WsOpcode 词表 enum + tungstenite Message ↔ 词表映射纯函数（opcode_of/opcode_word/payload_of）+ masked_by_direction（RFC6455 方向词形）+ payload_mime（text→text/plain、binary/控制帧→application/octet-stream）+ 单测 |
| `crates/wsw-runtime/src/decoder.rs` | 692 | D5 Decoder 变换链：DECODER_OPS 17 op 词表（12/17 项落地——五变换双向 + Unicode escape/unescape + JWT 结构解析不验签 + JSON pretty/compact/validate + sha256；Gzip/Deflate/Brotli 裁剪面词表外）+ DecodeOp serde snake_case 词形 + 结构化拒绝词表（InvalidHex/InvalidBase64/InvalidUnicodeEscape/InvalidJson/InvalidJwt/UnknownOp/InputTooLarge）+ apply_ops 多级链纯函数 + 8MiB 输入帽 + 单测（五变换双向恒等 + 负例硬门 + mutator pub 面跨面一致性） |
| `crates/wsw-runtime/src/comparer.rs` | 666 | D6 Comparer 消费层：IgnoreRules（skip_line_regexes 行级剔除 + normalize_key_regexes 键值掩码——regex=1.13.1 既有声明纯消费）+ mask_line_regex 三形态（JSON/表单/日志，diff.rs mask_key 同型）+ compare_text（预处理 → diff_normalized 单点）+ compare_json（serde_json 层级递归键序不敏感 + `$.a.b[0]` 路径归因）+ compare_headers（BTreeMap 键序归一逐键对照）+ compare_bytes（首差异 OFFSET 归因 + 差异字节数）+ 相同判定键 = sha256 非长度（PLAN :303）+ persist_report_as_blob（cas/store 既有面 Artifact 落账）+ 单测 |
| `crates/wsw-runtime/src/passive.rs` | 399 | D7 Passive Audit：PASSIVE_RULES 内置注册表 14 规则五类常量（rule_id 与 030 CHECK 逐字同源）+ analyze_exchange 行级纯函数（headers/status/scheme 投影；明文响应不报 HSTS 缺失——无 TLS 事实不伪造）+ analyze_and_persist（INSERT OR IGNORE 幂等锚 + candidate 词形零 evidence 写面）+ 单测 |
| `crates/wsw-runtime/tests/ws_frame.rs` | 396 | S3 集成测试（判据 1/2/8 承载）：tokio-tungstenite 内存 echo 夹具 + 手工 RFC6455 绝对形式升级握手经真实代理（loopback socket）→ 透传捕获对账（握手行 protocol='ws' = 1 + 帧行 8 + opcode 全词表 + sequence 连续零缺口 + direction/masked 归向 + CAS 读回逐字节）+ scope 外升级 403 负例零帧行 |
| `crates/wsw-runtime/tests/decoder_comparer.rs` | 254 | S5/S6 集成测试（判据 4/5 承载）：跨面往返恒等矩阵（mutator pub 面 encode ↔ decoder decode 五变换）+ 负例硬门（奇长 hex/非法 base64/孤立代理区/坏 JSON/JWT 段数）+ CAS/exchange/frame 比较源消费 + Artifact 落账读回 + 四 Diff 归因矩阵 + 相似度数值断言 |
| `crates/wsw-runtime/tests/passive_audit.rs` | 293 | S7 集成测试（判据 6 承载）：N exchange → 命中行数对账 + 幂等重分析零新增 + 注册表词表同源断言 + 零 evidence_id 列表结构断言 + 外键唯一指向断言 + passive.rs 代码面零 evidence 词形反证 + CAS 对账（blob 行数 = 文件数，sha256 寻址幂等）+ 旁路延迟面（100 行 <1000ms 纯函数微秒级） |
| `apps/server/selftest_wsw56.sh` | 640 | S8 收口脚本（V-9 守卫预挂 + python3 stdlib 手工 RFC6455 echo 靶机 ~80 行零新依赖 + HTTP 被动面响应器 + python3 客户端经代理绝对形式升级握手 + rp-wsw ws connect/replay 直连 + rp_server 七端点 curl 全链 + 性能门 20 采样），42 断言 PASS/SKIP 分开计数 |

### A.2 修改文件（git diff HEAD --stat 原文）

```text
 apps/server/rp_server.py                           | 388 ++++++++++++++
 crates/agent-engine/tests/migration_dev_copy_p7_3.rs  |  10 +-
 crates/agent-engine/tests/migration_idempotent.rs     |   4 +-
 crates/database/tests/backup_restore.rs               |   4 +-
 crates/database/tests/migrations_agent_data_plane.rs  |  10 +-
 crates/database/tests/migrations_context_broker.rs    |   9 +-
 crates/database/tests/migrations_runtime_node.rs      |  10 +-
 crates/database/tests/migrations_tool_contract.rs     |   9 +-
 crates/plugin-runtime/tests/lifecycle_full_path.rs    |   8 +-
 crates/rp-mcp-parity/tests/caller_auth.rs             |   4 +-
 crates/workflow-engine/tests/migration_replay.rs      |   4 +-
 crates/wsw-message/src/store.rs                       |   4 +-
 crates/wsw-message/tests/cas_e2e.rs                   |  12 +-
 crates/wsw-runtime/src/bin/rp-wsw.rs                  | 595 ++++++++++++++++-
 crates/wsw-runtime/src/lib.rs                         |   4 +
 crates/wsw-runtime/src/listener.rs                    | 108 +++-
 crates/wsw-runtime/src/listener/handler.rs            | 353 +++++++++++-
 crates/wsw-runtime/src/persist.rs                     | 517 +++++++++++++++-
 crates/wsw-runtime/tests/exchange_persist.rs          |   4 +-
 crates/wsw-runtime/tests/fuzz_job.rs                  |   2 +-
 crates/wsw-runtime/tests/intercept_queue.rs           |   2 +-
 21 files changed, 2005 insertions(+), 57 deletions(-)   （另加 9 个新建文件 + 任务书未跟踪件）
```

- `rp_server.py` +388：D4 七端点（`_wsw_ws_frames_list` 游标分页 + `_wsw_ws_frame_detail` payload=1 CAS 按需 + `_wsw_ws_replay`/`_wsw_decode`/`_wsw_compare` rp-wsw 子进程委派 + `_wsw_passive_hits_list` 游标分页 + `_wsw_passive_rules` 注册表只读投影 + `_wsw_require_030` 水位守卫）+ API_ROUTES 三行 + do_GET 动态路径一段 + wsw_post_route 前置分发；SSE 面（:47 起）零改动；既有 /api/wsw/* 词形零触碰。
- `rp-wsw.rs` +595：D4 `ws connect`（scope 闸前置——--db/--session-id 规则源打开只读 store + wss→https 映射复走查点 A 词形）/`ws replay`（030 帧行 + 握手行还原目标 + 编辑/原样双态）/`decode`/`compare` 三子命令 + status_heartbeat 加性三计数（ws_connections/ws_frames_captured/ws_frames_dropped）+ usage 同步。
- `handler.rs` +353：D3 WswHandler WS 结构化捕获（handle_message :2098 挂点单点扩展：opcode/payload 提取 → store_ws_payload CAS+wsw_body_blob → WsFrameRow 通道投递；Some 透传语义不变）+ WsCaptureState/WsConnCapture（endpoint+client_addr 键共享面）+ ws_conn_entry 握手行同步落库（protocol='ws' source='proxy_capture' capture_fidelity=degraded_no_wire——030 FK 前置，Pool foreign_keys=ON 双通道无序保证下的正确性前提）+ ws_handshake_scope（ws/wss→http/https 映射 scope::check 三态投影，scope.rs 零 diff 纯调用面）+ ListenerShared 帧通道（mpsc 2048 + push_ws_frame_row 满载丢行计数可见）+ Counters 三加性原子。
- `listener.rs` +108：persist_wsframe_worker（批量事务 ≤128 行 / 50ms flush，salvage 语义）+ flush_exchange_batch 落库成功后接 passive::analyze_and_persist 旁路（worker 侧，listener 转发路径零同步调用）+ ListenerStatus 加性三字段。
- `persist.rs` +517：assert_watermark 29→30（doc + `v != 30` + 消息串）+ WsFrameRow/PassiveHitRow 结构 + validate 双保险 + insert_ws_frames/insert_passive_hits（INSERT OR IGNORE 幂等锚批量 salvage）+ get_ws_frame/get_exchange_row/list_frames_by_exchange/list_passive_hits/ws_frame_count/passive_hit_count + generate_frame_id/generate_hit_id。
- 链尾十六硬钉 16 文件全词形随更 29→30（A12 清单；`store.rs` 为 wsw-message 唯一落点例外——A12 载体）。

## B. §5 收口命令实测（exit code + 关键输出行）

| 命令 | exit | 关键输出 |
|---|---|---|
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` | 0 | `test result` 182 段合计 **1301 passed / 0 failed**（S1 基线 1270 + 本批新增 31；零基线红延续） |
| `cargo test --offline -p wsw-runtime -p wsw-message -p database -- --test-threads=1` | 0 | **305 passed / 0 failed**（含本批 ws_frame 2 + decoder_comparer 4 + passive_audit 3 集成） |
| `cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings` | 0 | `Finished \`dev\` profile ... target(s)`（零警告零错误） |
| `rustfmt --edition 2021 --check`（本批 12 文件） | 0 | 本批文件 rustfmt 零 diff（workspace 全仓 `cargo fmt --all -- --check` 为**存量红**——约 120 文件含 sender.rs/diff.rs 等已提交文件，基线即红，见 §E） |
| `cargo deny check advisories && cargo deny check licenses` | 0/0 | `advisories ok` / `licenses ok` |
| `grep -cE '^\[\[package\]\]' Cargo.lock` | 0 | **399**（恒定）；`grep -c 'name = "reqwest"'` = **0**；`grep -c "flate2\|brotli"` = **0** |
| `RP_WSW_BIN=... bash apps/server/selftest_wsw4.sh` | 1 | **33 PASS / 1 FAIL**——唯一 FAIL = 该脚本自带 `user_version=28` 陈旧钉（:39；029 入链起已漂移，HEAD 基线即红：actual=29≠28；本批后 actual=30）。零改动维持，如实登记 §E |
| `bash apps/server/selftest_wsw56.sh` | 0 | **42 PASS / 0 SKIP / 0 FAIL**（真实 WS 会话实测数字见 §C） |
| `rp-wsw db migrate --db <dev-b1 副本>` | 0 | `migrations applied: [30]; user_version=30`；幂等重跑 `applied: []; user_version=30` 零异常 |
| `git status --porcelain migrations/` | 0 | 仅 `?? migrations/030_wsw_ws_frame.sql` 单文件（001-029 零 diff） |
| `grep -rn "user_version=29\|, 29,\|Some(29)" crates/wsw-runtime/src/ crates/wsw-message/src/ \| grep -v 030 \| wc -l` | 0 | **0**（链尾钉残留反证；A12 十六文件全词形 =30 实测） |
| `git status --porcelain \| grep -c "web/\|vendor/\|sender.rs\|scope.rs"` | 0 | **0**（跨轨零 diff 反证）+ `git diff HEAD --stat -- Cargo.lock crates/wsw-runtime/Cargo.toml crates/wsw-message/Cargo.toml` 空 |
| `grep -n "toBe(22)" web/tests/workspaces.test.ts` | 0 | `:109 expect(new Set(routes).size).toBe(22);` 路由钉零随更 |

## C. 真实 WS 会话实测数字（selftest_wsw56.sh，禁模拟冒充——PLAN :754 全部来自命令回显）

```
PASS=42 SKIP=0 FAIL=0 / SELFTEST_WSW56: ALL GREEN
数字面：帧行=2010；§A 会话帧=8；§D 遍历=2010/零重=0；§E 命中=9；
代理增量=2ms；p95 frames=11ms decode=10ms hits=8ms
```

- 链路：python3 stdlib 手工 RFC6455 echo 靶机（sha1 accept + 掩码帧解析）↔ rp-wsw listener（030 落库）↔ python3 客户端绝对形式升级握手经代理——与 tests/ws_frame.rs 同链路的第二独立实现面。
- §A 经代理会话：握手 101 + text/binary/ping/close 双向 8 帧；text/binary 回声逐字节一致；客户端可见 pong ≥1（代理 tungstenite 服务端角色自应答 + 上游透传——RFC6455 5.5.3 合法行为，回声序非确定按词表集合断言）；DB 侧 opcode 全词表（text/binary/ping/pong/close 双向逐帧）+ sequence 连接内连续零缺口。
- §B CLI ws connect 直连：echo = 发送文本 + 靶机 delta 计数；scope 外（default deny）exit 3 + `scope_blocked` 词形。
- §C ws replay：原样回放靶机实收 = 原帧 payload（CAS 读回）+ data_frames delta=+1；编辑回放（--payload EDITED-wsw56）靶机实收 = 编辑值。
- §D 1000 帧经代理：帧行落库 2010（双向 2000 + §A 8 + 2 close 对）；GET /api/wsw/ws/frames 游标分页 limit=500 遍历 **2010 行零重零漏**；filter_cursor_mismatch 400 负例；帧详情 payload=1 CAS base64 回读；401 无鉴权负例。
- §E 被动对账：HTTP 捕获（Set-Cookie 无属性 + Server nginx/1.24.0 + X-Powered-By + CORS 缺省）→ **wsw_passive_hit = 9 行与规则预期逐条对账**（4 缺头 + 3 Cookie 属性 + cacheable + 2 信息泄露）；`wsw_body_blob` 行数 = CAS 文件数（1004=1004，sha256 寻址幂等）；hit 行零 evidence_id 列。
- §F 性能门（判据 9）：WS 握手+回声中位数**经代理 2ms 增量**（repeat 4 直连/经代理对照）< 50ms；GET /ws/frames、POST /decode、GET /passive/hits **p95 = 11/10/8ms**（各 20 采样）< 300ms。
- §G 生产库反证：migrations/platform*.db 五文件 mode=ro user_version 只读指纹（platform.db=1 未触碰）。

## D. 判据逐条状态（§4 十二条）

| # | 判据 | 状态 | 证据 |
|---|---|---|---|
| 1 | WS 透传捕获对账 | **PASS** | tests/ws_frame.rs 2 passed（握手行 protocol='ws'=1、帧行 8、opcode 全词表、sequence 连续零缺口、direction 归向）+ selftest §A/§D（2010 行对账）双面 |
| 2 | 透传保真 + 编辑重放 | **PASS** | Rust 夹具 echo 逐字节断言 + selftest §C replay 原样/编辑双态（靶机 delta=+1、实收=编辑值） |
| 3 | 030 迁移与链尾钉 | **PASS** | user_version=30 + 幂等重跑 no-op + 十六文件全词形 =30 零残留 29 + migrations 单文件纯新增零 ALTER |
| 4 | Decoder 往返恒等（负例硬门） | **PASS** | decoder_comparer.rs 矩阵全绿：五变换 decode(encode(x))==x + 编码像 encode(decode(x))==x + 多级链 + JWT 三段解析不验签断言 + 负例（奇长 hex/非法 base64/孤立代理区/坏 JSON/JWT 段数）结构化拒绝 |
| 5 | Comparer 差异归因（负例硬门） | **PASS** | 增删改区间归因 + normalize_keys/skip_line_substrings/regex 两维收敛 + JSON 键序不敏感 + Header 逐键 + Hex 首差异 OFFSET + 相似度数值断言 + 负例等长不同内容 identical=false（PLAN :303） |
| 6 | Passive 捕获对账 023（候选态负例硬门） | **PASS** | 命中 9 行对账 + 幂等重分析零新增 + wsw_body_blob=CAS 文件数 + 负例零 evidence_id 列 + 全链零 Finding 写面（表结构 + 外键唯一指向 + 源面词形三重反证） |
| 7 | 独立入口面 | **PASS** | `rp-wsw ws connect/replay`、`decode`、`compare` 实跑 exit 0（scope 拒 exit 3 亦实测）+ 七端点 Bearer 401 负例 + 游标分页 2010 行零重零漏 + filter_cursor_mismatch 400 |
| 8 | scope 闸延续 | **PASS** | ws connect scope 外 exit 3 + API replay scope 外 403 + 传输级升级 403（零 ws 行零帧行）+ scope.rs 零 diff 实证（ws/wss 映射复走查点 A 词形，纯调用面） |
| 9 | 性能门 | **PASS** | 代理增量中位数 2ms < 50ms（WS 握手+帧透传场景 repeat 4）；三新端点 p95 = 11/10/8ms < 300ms（20 采样）——全部命令回显，无预填 |
| 10 | 零依赖与跨轨零 diff | **PASS** | Cargo.lock 399 恒定 + vendor 零 diff + 两 Cargo.toml 零 diff + reqwest=0 + web/** 零 diff + sender.rs/scope.rs/audit-engine/mcp-server/tool-engine 零 diff + 路由钉 22 零随更 + porcelain 仅 §2.1 落点 |
| 11 | 全量回归 | **PASS** | workspace exit 0 / 1301 passed 零基线红延续 + clippy -D warnings 绿 + deny 双检绿；selftest_wsw4.sh 零改动（其自带 28 陈旧钉红如实登记 §E——非本批引入） |
| 12 | WSW-7/WSW-8 前置输出 | **PASS** | 见 §F/§G 专节 |

## E. 坏消息单列（四项登记 + 三项实施注记）

1. **Gzip/Deflate/Brotli 解压不入批（D5 裁剪登记）**：flate2/brotli/zstd 在 Cargo.lock 与 vendor/ 双零命中，vendor 红线下解压库引入成本不成比例；Decoder 词表落地 12/17 项（17 op = 12 词表项投影），解压三面留后续批 vendor 立项。API 侧 `ops:["gzip_decode"]` 400 词表外拒绝（负例实测）。
2. **JSON 规则文件加载面不进（观察池）**：被动规则 = 内置 Rust 注册表 14 规则起步（D7 裁决），零规则文件加载面。
3. **SSE additive kind 扩展不进（D9，MA 轨域）**：帧 History = 游标分页拉取（W3-D3 协议复用），零实时推送面；rp_server.py:47 SSE 面零改动实证（本批 diff 未触及 /api/events 段）。
4. **UI 四页不进（D8）**：web/** 全目录零 diff；GUIDE `decoder|comparer|workbench` 零命中维持；四页规格缺口清单见 §G。
5. **`cargo fmt --all -- --check` 全仓存量红（非本批引入）**：约 120 文件含 sender.rs/wsw-message diff.rs/fuzz/* 等已提交文件本就不合 rustfmt——HEAD 基线即红。本批处理 = 仅对本批 12 文件 rustfmt（`--check` exit 0），未触碰红线文件。实施注记：rustfmt 对 lib.rs 递归格式化子模块曾误触 sender.rs/scope.rs/fuzz/*，已 `git checkout HEAD --` 还原为字节零 diff（复核 `git diff HEAD --stat -- ...fuzz/` = 0）。
6. **selftest_wsw4.sh 自带陈旧钉（非本批引入）**：:39 `user_version=28` 断言在 029 入链（MA-3）后即漂移——HEAD 基线实跑同样 33 PASS/1 FAIL；本批维持零改动（测试钉值不改纪律），actual=30 实测入档。
7. **base64 解码 = std 内嵌实现（D5 词形偏差如实登记）**：D5 词形为「Base64 解码用 base64 crate」——但 wsw-runtime Cargo.toml 零 diff 红线禁止新增声明且 wsw-message 无 re-export，解码面落为 decoder.rs std 内嵌（B64_STD/B64_URL 与 mutator.rs 同字母表词形）；编码方向走 mutator.rs `apply_mutators` pub 面（消费面非第二实现）；往返矩阵钉跨面一致性。sha2 走 `wsw_message::cas::sha256_hex` 唯一挂点。
8. **代理 tungstenite 控制帧自应答（引擎合法行为，测试面适配）**：真实客户端 Ping 经代理时，代理 tungstenite（对客户端 = 服务端角色）按 RFC6455 5.5.3 自应答 Pong，与上游 Pong 透传并存——客户端可见 pong 数 1..2、回声序非确定；selftest §A 断言按词表集合 + 逐字节双面承载，DB 捕获面仍逐帧精确（ping c2s 1 + pong s2c 1）。
9. **close 帧透传语义**：客户端发 Close 后 tungstenite 写侧即闭（"Trying to work with closed connection" 日志可见）——服务端后续帧不可达；selftest 客户端交互流 = 先读回声再发 Close（RFC6455 收口序）；DB 侧 close 帧（c2s/s2c）仍逐帧捕获。

## F. WSW-7 前置输出（Scanner/Provider 统一入口被动侧输入）

1. **passive rule_id 词表 + 注册表只读投影**：GET `/api/wsw/passive/rules`（五类 14 规则：`missing_header_strict_transport_security`/`missing_header_content_security_policy`/`missing_header_x_content_type_options`/`missing_header_x_frame_options`/`cookie_missing_secure`/`cookie_missing_httponly`/`cookie_missing_samesite`/`info_leak_server_version`/`info_leak_x_powered_by`/`info_leak_via_header`/`redirect_cross_origin`/`redirect_scheme_downgrade`/`config_permissive_cors`/`config_set_cookie_cacheable`；rule_version=1；severity 四词表 info/low/medium/high；class 五词表）。**单源 = `crates/wsw-runtime/src/passive.rs` PASSIVE_RULES**；030 CHECK 与 rp_server 投影为镜像（selftest 逐条对账钉漂移）；WSW-7 主动 Provider 接线禁第二规则注册表。
2. **七端点 Bearer 鉴权面与错误词形**：全部挂既有 `/api/*` require_auth + AUTH_ENABLED 写面强制；错误词形 = ApiError{code,message}（`wsw_030_not_applied` 503 / `filter_cursor_mismatch` 400 / `bad_ops` 400 / `rp_wsw_bin_missing` 503 / `replay_rejected` 403 / `frame_not_found` 404）；写面（replay/decode/compare）= rp-wsw 子进程委派（subprocess.run 30s/15s timeout，JSON stdout 契约 = `_wsw_rp_wsw_json` 末行 JSON 解析），零直连 SQL 写。

## G. WSW-8 前置输出（UI 批零后端改动输入——WSW-4 G.1 交接词形延续）

1. **七端点响应形状词形**：
   - `GET /api/wsw/ws/frames?limit=&cursor=&connection_exchange_id=&opcode=&direction=` → `{rows:[13 列投影],next_cursor,has_more,total,limit,fingerprint}`（游标序 captured_at DESC, frame_id DESC）
   - `GET /api/wsw/ws/frames/{id}?payload=1` → 帧行全列 + `payload`(base64)/`payload_encoding`（CAS 按需）
   - `POST /api/wsw/ws/replay` body `{frame_id,payload?|binary_hex?,target?,session_id}` → rp-wsw JSON `{state,op,frame_id,edited,url,sent:{opcode,len},echo:{opcode,len,text?},duration_ms}`（scope 外 403）
   - `POST /api/wsw/decode` body `{input|input_base64,ops:[17 词表]}` → `{ops,result,error}`
   - `POST /api/wsw/compare` body `{mode:text|json|header|hex,left:{text|exchange_id|frame_id},right,…,skip_line_regexes?,normalize_key_regexes?,save_artifact?}` → CompareReport（mode/identical/left_sha256/right_sha256/left_len/right_len/similarity?/hunks?/json_diff?/header_diff?/hex_diff{first_diff_offset,differing_bytes,preview}?/artifact?）
   - `GET /api/wsw/passive/hits?limit=&cursor=&exchange_id=&rule_id=&severity=` → `{rows:[8 列投影],next_cursor,has_more,total,limit,fingerprint}`（游标序 created_at DESC, hit_id DESC）
   - `GET /api/wsw/passive/rules` → `{rules:[14 投影],classes,severities,count,note}`
2. **GUIDE 零命中登记 + 四 UI 页规格缺口清单**：`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md`（743 行）grep `decoder|comparer|workbench` 零命中维持——四页（WS 帧展示 PLAN :733 / Comparer :732 / Decoder / Passive）无 Figma 基线，届时按 PLAN §5.2 :345-349 九入口 + yakit 参考图扩展设计；API 形状如上 = 零后端改动输入。
3. **性能门交接**：首屏<3s/交互<1s 随 WSW-8 UI 批适用（D10 不冒充）；本批实测基线 = 代理增量 2ms / p95 frames 11ms decode 10ms hits 8ms（UI 批回归对照基准）。

## H. 未解决项 / 观察池（各按独立批立项，本批零实现）

1. Gzip/Deflate/Brotli 解压 vendor 立项（D5 裁剪遗留；Cargo.lock 399 恒定前提下的 vendor 全量入库 + deny 门成本先行评估）。
2. JSON 规则文件加载面（被动规则热加载；本批注册表常量起步）。
3. SSE additive kind 扩展（wsw_frame/wsw_passive_hit kind——MA 轨词表域外扩面，D9 裁决）。
4. 帧级实时推送观察池（WS 推送 server 三否维持——MA-4 D1 词形）。
5. HAR 导入导出（PLAN :587-594，W3-TASK :21 词形延续；exchange.rs Import 词形已备）。
6. chromiumoxide/浏览器面（PLAN :640，WSW-8）。
7. `cargo fmt --all -- --check` 全仓存量红（约 120 文件）——建议独立 chore 批处理（与本批无关，勿在本批顺手改）。
8. selftest_wsw4.sh `user_version=28` 陈旧钉——建议随下一链尾演进批顺带钉值随更（本批禁改维持）。

## I. 回滚（§7 对照）

- 单 commit 链交付（S2-S7 引擎面同批；selftest/报告可分 commit）；`git revert` 整批撤销 → 030 随 revert 回 029 链尾（十六文件钉值复原 29，030 空位重开）→ handler.rs 捕获扩展回 :2100 透传现状（透传日志词形保留）→ wsframe/decoder/comparer/passive 四新模块零残留 → lib.rs 四行声明清零 → rp_server 七端点消失 → selftest_wsw56.sh 零残留。
- 生产库/RTIS/宿主软件零操作无生产回滚面；web/**/vendor/ 零 diff 无前端回滚面。
