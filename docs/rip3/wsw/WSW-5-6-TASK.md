# WSW-5+6 合并批任务书 · WebSocket Workbench + Decoder / Comparer / Passive Audit（传输/工作台面 + 三工具面串联批，UI 不入批）

- 批次：WSW-5+6 合并（PLAN §九 :607/:608 两行——WSW-5 主要交付 = 握手、Frame History、帧编辑和重放，**不包含 QUIC / HTTP3**；WSW-6 主要交付 = 编解码、Diff、被动规则、候选发现，**不包含第二套扫描引擎**）
- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实施按本任务书执行，走 REDLINES 八步循环，显式 add 本批清单禁 add -A）
- 依据：[WEB-SECURITY-WORKBENCH-PLAN.md](../WEB-SECURITY-WORKBENCH-PLAN.md) §4.5（:274-283，含 :283「不把 WebSocket 帧与普通 HTTP Request 混成同一种实体」）/ §4.6（:285-294）/ §4.7（:296-303）/ §4.8（:305-311）/ §5.2（:345-349 九入口）/ §6.1（:444/:466-467）/ §7.2-7.4（:527-528/:556/:565）/ §8.1（:563-566）/ §11.5.7（:732-733）/ §11.6（:744-754）/ §十二（:758-768）/ §十四（:797「WebSocket、Decoder、Comparer 和 Passive Audit 具有可用的独立入口」）+ [WSW-CONTINUATION-REDLINES.md](WSW-CONTINUATION-REDLINES.md)（八步循环 + 九条红线）+ [WSW-2-ADR.md](WSW-2-ADR.md)（W2-D4 :96 / W2-D5 :136 / W2-D6 :150 / W2-D9 :177 捕获事实归属 / W2-D10 :181）+ [WSW-3-ADR.md](WSW-3-ADR.md)（W3-D2 :105 / W3-D3 :118 游标 / W3-D7 :172 写权分治）+ [WSW-3-TASK.md](WSW-3-TASK.md)（:14 代理增量预算 / :112-118 性能数字口径）+ [WSW-4-TASK.md](WSW-4-TASK.md)（D1 :18 发送单点 / D6 :22 diff 原语供 W6 / D8 :24 UI 不入批先例 / D9 :25 regex 转正）+ [WSW-4-BATCH-REPORT.md](WSW-4-BATCH-REPORT.md) E.7（:248「W6 Comparer 若需正则忽略面须在其批内补」）+ G.1（:278 UI 遗留交接）+ [MA-4-TASK.md](../../phase0/MA-4-TASK.md) D1（:16 SSE 单面落地 WS 零实现）/ A15（:43 十六文件硬钉）/ A16（:44 tokio-tungstenite 在树）
- 前置：WSW-4 已收口（028 入链 + regex 转正 + fuzz 五模块 + 四端点）；DI-3/RT-4 已收口（HEAD `aafb87e9`，工作树干净 `git status --porcelain`=0 行实测）；**workspace 全量基线零红**（本会话 `cargo test --offline --workspace` exit 0 实测——DI-3/RT-4 收口 commit 自述「MA-3 以来首次零基线红」）；030 迁移空位在且**本批立项**（D2）
- 设计裁决：本批特有裁决见 §0 D1-D10（**不另立 ADR**——裁决内嵌本档，WSW-4-TASK 先例；与 PLAN 分歧以 PLAN 为准并回改本档）
- 现状锚点：§1 A1-A16 均为 2026-11-22 本会话实测行号（HEAD `aafb87e9`、工作树干净、workspace 全量 exit 0 实测），实施时若行号漂移按函数名重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **两行合并批内部切分 = S 阶段串联**：WSW-5 传输/工作台面（S3-S5：030 帧表迁移 → WS 帧捕获结构化 → 重放/CLI/API）先行，WSW-6 三工具面（S6-S8：Decoder → Comparer → Passive Audit）后接，**单批单 commit 链交付**（报告按 :607/:608 交付列分节对照）；两行的合并依据 = 共享底座三件已在树（diff 原语/mutator 变换链/023 CAS——A7/A8/A6）+ 030 单文件迁移同批承载两行新表（wsw_ws_frame 与 wsw_passive_hit 一并入链，避免两次链尾硬钉随更成本）+ 独立入口验收 :797 同一句子覆盖四工具 | REDLINES 八步循环按批次不按 PLAN 行号；WSW-4 D10 已把 W5/W6 排除在 W4 外（:26），本批即其顺延承接批；两批交付列零交集（W5 无编解码、W6 无 WS 传输）= 切分线自然成立，S 串联保证 W6 的 Comparer 可消费 W5 的帧 History 行做跨帧对比 |
| D2 | **030 立项（本批唯一迁移）= `migrations/030_wsw_ws_frame.sql` 单文件纯新增两表**：`wsw_ws_frame`（帧元数据行：frame_id UNIQUE 幂等键 + connection_exchange_id 引用 wsw_exchange 行 + direction CHECK('client_to_server','server_to_client') + sequence_no + opcode CHECK 词表（'text','binary','ping','pong','close'——PLAN :276/:678 文本帧/二进制帧/Ping/Pong/Close 面）+ payload_len + payload_body_sha256/payload_body_uri/payload_body_len/payload_body_fidelity CAS 引用列（025 Body 列同型）+ masked/captured_at + scope_decision 三态引用列）+ `wsw_passive_hit`（被动命中候选行：hit_id + exchange_id + listener_id + rule_id CHECK 词表 + rule_version + severity CHECK('info','low','medium','high') + match_detail_json 8MiB 帽 + created_at + candidate 词形——**零 evidence_id 列零 Finding 提升**，PLAN :307-309/:504）+ 索引；链头 029→030 单版本推进，纯新增零 ALTER，schema_version INSERT OR IGNORE (30,…) + `PRAGMA user_version=30` 双写（025:170-181 先例）；**链尾十六硬钉 29→30 随更**（MA-4-TASK.md:43 A15 全清单十六文件全词形扫描——WSW-4 E.2 :245 教训「钉值扫描须全形态非抽样」） | 帧存储形态裁决：WSW-2/WSW-3 消息存储 = **SQLite 元数据行 + CAS 文件双层**（024/025 行 + 023 wsw_body_blob；W2-D9「行即事实」）——Frame History 需要搜索/过滤/分页（PLAN :279），文件存储无查询面，行式 = 唯一符合既有形态；PLAN :283「不把 WebSocket 帧与普通 HTTP Request 混成同一种实体」→ 帧行**不入 wsw_exchange**（025 wsw_exchange 只承载 WS 连接/握手行 protocol='ws'，词表 025:56 已备 + exchange.rs:90 词形在位）；030 空位实测（`ls migrations/030*` No such file；001-029 连续）；wsw_passive_hit 与帧表同文件立项 = 一次链尾成本承载两行 |
| D3 | **WS 传输面 = handler.rs :2098 挂点单点扩展，零第二 WS 栈**：帧捕获/编辑/重放全部挂既有 `impl WebSocketHandler for WswHandler`（listener/handler.rs:2098，`handle_message` :2100——hudsucker 高层 hook，Some(msg) 透传/None 吞帧；hudsucker WebSocketHandler trait vendor/hudsucker/src/lib.rs:175 `handle_websocket` stream/sink 默认泵 = 重放面 sink 侧自控）；扩展 = ①WswHandler WS 帧结构化捕获（opcode/payload/masked 提取经 `hudsucker::tokio_tungstenite::tungstenite::Message`——handler.rs:28 现状 import，再导出面 vendor lib.rs:51）→ 经 capture→persist 异步链（W3-D2 同型：共享 ExchangeCapture 通道 + 有界丢弃计数可见）落 030 wsw_ws_frame 行，Payload 走 `wsw_message::cas::store_body`（cas.rs:133，023 wsw_body_blob mime_type 无 CHECK 词表——text 帧落 'text/plain'、binary 帧落 'application/octet-stream'）+ 同批建 WS 握手行（wsw_exchange protocol='ws'，source='proxy_capture' 025:54 词形直用）；②连接生命周期面（连接建立/关闭帧观察 PLAN :278）落帧行 sequence 连续性 + close 帧行；**rp_server 零 WS server 零新端口**（MA-4-TASK D1 :16 三否词形「Rust WS server = 第二进程第二端口第二事件面」同门适用）；③tokio-tungstenite **零 Cargo.toml 新增声明**——消费全走 hudsucker 再导出（handler.rs:28 先例），opcode/into_data 等帧方法面经同一类型可达 | WSW-2-TASK :21「WebSocket Workbench（W5；本批仅 WS 帧透传挂载，POC 已证面）」——本批即该挂载的承接批；POC 先例 wsw-poc main.rs:131-140（同型 handle_message 透传）+ WSW-2-TASK :32 WebSocketHandler（透传）登记面；第二 WS 栈 = W2-D1 模块边界否决变体；Cargo.lock 399 恒定红线（A13）与 D10 依赖裁决一致 |
| D4 | **独立入口形状 = rp-wsw CLI 子命令 + rp_server API 端点双面，非独立端口**：①rp-wsw（:815-830 既有 listener/send/fuzz/ca/db 五子命令分派）新增 `ws` 子命令（`ws connect` 主动 WS 客户端连靶机发单帧——走 tokio-tungstenite 客户端面经 hudsucker 再导出 + scope 闸前置）与 `ws replay`（读 030 帧行经 rp-wsw 与靶机直连重发单帧/按序重放）；②rp_server 新增端点：GET `/api/wsw/ws/frames`（游标分页 W3-D3 协议复用 + connection_exchange_id/opcode/direction 过滤）、GET `/api/wsw/ws/frames/{id}`（帧详情含 CAS 回读）、POST `/api/wsw/ws/replay`（帧重放 spawn rp-wsw 子进程——W3-D7 send spawn 先例 rp_server.py:4067 同型、WSW-4 :4410 fuzz spawn 先例）、POST `/api/wsw/decode`（Decoder 变换链单步/多级执行）、POST `/api/wsw/compare`（Comparer 两对象 diff）、GET `/api/wsw/passive/hits`（命中游标分页）+ GET `/api/wsw/passive/rules`（规则注册表只读投影）——**全部挂既有 /api/ 分支 + Bearer 鉴权同面**，单端口纪律不动 | PLAN §十四 :797「可用的独立入口」= CLI 直用 + API 直用即达标（四工具同一句子），非四独立端口；PLAN :97「所有入口最终走统一的 Rust Application Service」+ WSW-4 D8「Application API 同源纪律」；rp_server 单进程单端口 = 既有形状（A10），新端口即 :97 旁路面 |
| D5 | **Decoder 底座复用 + 词表裁剪双裁决**：①复用面——编码方向直用 mutator.rs 既有 pub 纯函数（EncodeKind :22 五值 Url/Base64/Base64Url/Hex/HtmlEntity、url_encode :120、html_entity_encode :134、b64_encode/b64_decode 内嵌字母表面 :83-88，同 crate wsw-runtime 直用零声明）；解码方向补逆变换（url_decode/decode_hex/decode_html_entity 新纯函数；Base64 解码用 `base64` crate——wsw-message Cargo.toml sha2+base64 workspace 声明在位（WSW-1 转正先例）且 wsw-runtime 已 path 依赖 wsw-message（Cargo.toml 末行），**零新声明**）；Hash = sha2（同路径）；JSON 美化/压缩/校验 = serde_json pretty/compact（A13）；JWT = 结构解析（serde_json 三段 header/payload/signature 展示 + 不验签不猜测密钥——PLAN :292 原文）；Unicode escape = std char::from_u32；多级编码链 = 逐级变换纯函数链（mutator.rs :155-157 apply 形状复用）②**裁剪面——Gzip/Deflate/Brotli 解压不入本批**：flate2/brotli/zstd 在 Cargo.lock 与 vendor/ **双零命中**（本会话实测），vendor 红线（REDLINES 1「新增依赖必须 cargo vendor 全量入库 + deny 门」）下解压库引入成本不成比例，PLAN :288 解压词表部分落地如实登记限制，留后续批 vendor 立项裁决 | mutator.rs 是 WSW-4 落点的公开面（A8），Decoder 是其消费面非第二实现（「不建第二实现」纪律 = WSW-4-TASK D6 :22「不建第二 diff 实现」同款）；PLAN :285-292 词表全量 = 17 项，本批落地 12 项（五变换双向 + Unicode + JWT 结构 + JSON 三态 + sha2），解压三面裁剪登记 = WSW-4 D10 裁剪登记先例词形；base64/sha2 经 wsw-message 转正声明路径 = 零 Cargo.lock 变化实证（A13） |
| D6 | **Comparer = diff 原语零 diff + regex 忽略面落 wsw-runtime 消费层**：①diff.rs 原语（diff.rs:97 diff_lines / :271 similarity_ratio / :370 normalize_text / :391 diff_normalized，DiffOp :25 / DiffHunk :38 / NormalizeRules :53）**零 diff**——WSW-4 E.7 原文「W6 Comparer 若需正则忽略面须在其批内补（diff 原语保持零依赖红线）」（WSW-4-BATCH-REPORT.md:248），wsw-message 零 regex 结构性成立；②Comparer 新模块（wsw-runtime/src/comparer.rs）消费层预处理：在 NormalizeRules 之上新增正则忽略维（skip_line_regexes 行级剔除 + normalize_key_regexes 键值掩码——regex=1.13.1 已在 wsw-runtime Cargo.toml 转正声明，WSW-4 D4/D9 先例，**零 Cargo.toml 改动**），预处理后进 diff_normalized；③四 Diff 面（PLAN :298）：文本 = diff_lines/DiffOp 高亮投影；JSON 结构 = serde_json 层级递归比较（键序不敏感）；Header = exchange.rs 头投影逐键对照；二进制 Hex = 字节级十六进制视图 + 首个差异偏移（OFFSET 归因）；相似度 = similarity_ratio，**长度相同≠内容相同**（PLAN :303——去重/相同判定键 = sha256 非长度，analyzer.rs 先例同型）；④比较对象 = 两个 wsw_exchange Body（CAS 回读 read_body cas.rs:265）/ 两个帧 Payload（030 行）/ 手工输入文本（API 直传）；结果保存 Artifact 走 wsw_message::artifact.rs 既有面 | E.7 是本批登记在案的承接义务（WSW-4-TASK D6 :22「W6 批 Comparer 复用本原语 + 补 UI」——UI 面归 WSW-8 见 D8）；regex 转正已在 wsw-runtime（wsw-runtime/Cargo.toml 末行 `regex = "=1.13.1"` + lock :2231-2232 实值），本批纯消费零依赖动作；忽略面三维度（子串/键值/正则）对齐 PLAN :301「忽略动态字段、时间戳、随机 Token 和指定 Header」 |
| D7 | **Passive Audit = capture 链旁路分析 + 内置规则注册表起步 + 候选态零提升**：①挂点 = capture→落库异步 worker 旁路（W3-D2 异步链同型扩展）——exchange 行落库后逐行规则分析，**不阻塞转发**（PLAN :307「不阻塞正常请求转发」= 分析在异步 worker 侧，listener 转发路径零同步调用）；②规则面 = 内置 Rust 注册表起步（固定词表五类：安全 Header 缺失 / Cookie 属性 / 信息泄露迹象 / 异常跳转 / 常见配置问题——PLAN :308 逐类；rule_id CHECK 词表 + rule_version 列；规则参数 = 注册表常量，**本批零规则文件加载面**——JSON 规则文件加载登记观察池）；③命中落 030 wsw_passive_hit 行（candidate 词形，零 evidence_id 零 Finding 提升——PLAN :309「被动发现只能生成候选项，不得直接成为已确认 Finding」+ :504 分开建模 + :565 候选语义）；④主动扫描 Provider 零实现（PLAN :310-311 Nuclei/ZAP/RTIS 走 WSW-7 Tool Engine 面；不建第二扫描器注册表）；⑤audit-engine Emit 不接线（W2-D9 :177 词形「AuditEvent 为插件生命周期词表混用即第二体系污染」延续） | 被动分析 = 流量审计面非扫描引擎面（:30「Scanner / Passive Audit：被动流量审计与已有扫描 Provider 的统一入口」——统一入口归 WSW-7，本批只落被动侧）；挂点旁路 = 代理增量 <50ms 预算不受破坏的结构前提（W3-TASK :14 落库链路同款纪律）；030 wsw_passive_hit 表已在 D2 立项承载 |
| D8 | **UI 不入批（纯引擎+CLI+API 批）**：`web/**` 全目录零 diff——①PLAN :607/:608 交付列**无 UI 面**（对照 W3 行 :605 明文「History 列表详情……Tab 历史」UI 交付词形、W2-D8「UI 多做即提前消费」先例）；②`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` 743 行 grep `decoder|comparer|workbench` **零命中**（本会话实测——Figma 基线无对应页，GUIDE :95「Figma 不可用可先实现代码并记录待同步项」反向不适用：不是 Figma 缺失而是页面无基线）；③WSW-4 D8 先例（:24「批次表即明文口径，未列即不做」）。工作台 UI 四页（WS 帧展示 PLAN :733 / Comparer 差异展示 :732 / Decoder 页 / Passive 页）**登记 WSW-8 后批前置输出**（§8.5）；`web/tests/workspaces.test.ts:109` 路由钉 `toBe(22)` **零随更**（W3-D8 20→22 现值） | UI 四页无 Figma 基线 = 提前实现即无依据设计面（W2-D8 否决理由同源）；本批 API 形状先行（D4 端点词形 = UI 批零后端改动输入——WSW-4-BATCH-REPORT G.1 :278 交接词形延续） |
| D9 | **SSE vs WS 实时面 = 本批零推送实现，语义不重复裁决**：MA-4 D1（MA-4-TASK.md:16）「SSE 单面落地，WS 零实现」三否词形 + D2（:17）事件源 = 既有表 tail 三形态 cursor，词表 = run_state/claim_state/message/guardrail/audit（additive）——**MA 轨 SSE 是协作事件面，WSW 帧流是操作数据面，语义不重叠**；本批帧 History = 游标分页拉取（W3-D3 协议复用），**零实时推送面**：①SSE additive kind 扩展（wsw_frame/wsw_passive_hit kind）不入批——SSE 源三形态 cursor 对 030 新表需新增扫描形态（append-only rowid 面），属 MA 轨词表域外扩面登记观察池；②WS 推送 server 三否（D3 已裁决零第二 WS 栈）；③帧列表 UI 刷新 = 客户端轮询 GET /api/wsw/ws/frames（D4 端点）即可承载 PLAN :279 帧列表要求 | PLAN :607 交付列无实时推送词面（「握手、Frame History、帧编辑和重放」）；MA-4 D7 :22 引文「WSW-3 性能门管 UI 页面管不到流式推送——推送面单独数字化」= 推送面独立立项先例；rp_server.py:47 SSE 面零改动 = MA 轨收官资产保护 |
| D10 | **依赖与性能门适用面**：①零新依赖——Cargo.lock 399 恒定（本会话实测）+ vendor/ 零 diff + Cargo.toml 零新增行（regex 转正 WSW-4 已落、tokio-tungstenite 走 hudsucker 再导出 D3、base64/sha2 走 wsw-message 既有声明 D5）+ reqwest=0 保持；②性能门适用面三分：**代理增量 <50ms 适用**（WS 帧捕获进链后 listener 代理增量不得破坏——W3-TASK :14/:117 口径，本批加测 WS 握手+帧透传场景）；**新 API p95 <300ms 适用**（20 采样 × ≥3 新端点，W3-TASK :118 口径）；**首屏<3s/交互<1s 不适用本批**（无 UI 面——随 WSW-8 UI 批承接，如实登记）；虚拟滚动/分页强制对 API 面投影 = 帧列表/命中列表**必须游标分页禁 OFFSET**（W3-D3 协议）；③全部数字来自真实命令回显禁预填（PLAN :754） | W3-TASK :112 性能硬门四项中 UI 两项随 UI 批 = WSW-4 先例（纯引擎批无 UI 门）+ MA-4 D7 :22 推送面单独数字化先例同构；PLAN :753「不能在设计阶段预填目标值冒充实测结果」 |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A16）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 PLAN 两行与验收面 | WSW-5 行 :607（握手、Frame History、帧编辑和重放 / 不包含 QUIC·HTTP3）、WSW-6 行 :608（编解码、Diff、被动规则、候选发现 / 不包含第二套扫描引擎）；独立入口验收 = §十四 :797「WebSocket、Decoder、Comparer 和 Passive Audit 具有可用的独立入口」（§8.3 :583-594 是导出互操作非入口面）；WS 功能面 §4.5 :274-283（:283 不混实体）；Decoder §4.6 :285-294（:288 解压词表）；Comparer §4.7 :296-303（:298 四 Diff / :301 忽略面 / :303 长度≠内容）；Passive §4.8 :305-311（:307 不阻塞 / :308 五类 / :309 候选 / :310-311 Provider 复用）；候选/Finding 分建模 :504/:565；POC WS 面 :674-680（握手/文本/二进制/Ping/Pong/Close）；§十候选 :638 tokio-tungstenite / :640 chromiumoxide（浏览器候选——WSW-8 面，本批不进） | docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md:607,608,797,274-283,285-294,296-303,305-311,504,565,674-680,638,640 |
| A2 WS 传输挂点 | `impl WebSocketHandler for WswHandler` :2098 + `handle_message` :2100（Some 透传）+ ctx endpoint 提取 :2102-2106 + passthrough 日志 :2109「ws frame passthrough (mounted; workbench W5)」；Message import = :28 `use hudsucker::tokio_tungstenite::tungstenite::Message`；hudsucker 再导出 vendor/hudsucker/src/lib.rs:51 `pub use tokio_tungstenite` + WebSocketHandler trait :175（handle_websocket stream/sink 泵 :178-183）；Proxy 组装 listener.rs:538 WswHandler::new + :550-554 `Proxy::builder()…with_http_handler`；POC 先例 crates/wsw-poc/src/main.rs:131-140 同型透传 | crates/wsw-runtime/src/listener/handler.rs:28,2098,2100,2102-2106,2109；vendor/hudsucker/src/lib.rs:51,175,178-183；crates/wsw-runtime/src/listener.rs:538,550-554；crates/wsw-poc/src/main.rs:131-140 |
| A3 scope 闸面 | `scope::check` :296（ScopeDecision Allowed/Blocked）+ `check_redirect_hop` :395 + `audit_resolved` :423 + ScopeRules :139 / ScopeTarget :186 / 拒绝 code() :234；handler.rs 调用点 :1585（请求面）与 :2043-2044（Modified-Forward 二次校验面）；scope.rs **逐字节零 diff 禁区**（sender.rs 同） | crates/wsw-runtime/src/scope.rs:296,395,423,139,186,234；crates/wsw-runtime/src/listener/handler.rs:1585,2043-2044 |
| A4 024 拦截队列 | 024 三表 = wsw_listener :33 / wsw_session :45 / wsw_intercept :56（五态 state 词表 + event_id UNIQUE 幂等 + direction request/response）；persist.rs state_to_db :75-81 五态映射 + InterceptDirection :45；`expire_all_queued` 停机强终态（persist.rs:11 注释面）；**W5 帧不进拦截队列**（024 wsw_intercept 是 MITM HTTP 请求/响应面语义——WSW-4-TASK D7 :23「两词表不混」同型） | migrations/024_wsw_intercept.sql:33,45,56；crates/wsw-runtime/src/persist.rs:45,75-81,11；docs/rip3/wsw/WSW-4-TASK.md:23 |
| A5 025 exchange 与消息类型 | wsw_exchange :47（`protocol IN ('http','ws')` :56 注释「exchange.rs:90 词表」/ `source IN ('proxy_capture','repeater','fuzzer','import')` :54）+ wsw_repeater_tab :115 / tab_version :127 / send :141；exchange.rs MessageSource :47（ProxyCapture/Repeater/Fuzzer/Import——Import 注释「HAR 等，WSW-3/6」）/ protocol 字段 :90（小写 "http"/"ws"，PLAN :490）；insert_exchanges persist.rs:908 + source 校验 :824 + 词表注释 :674；链尾断言 persist.rs:893-900 `assert_watermark`（`v != 29` → Validation err——030 入链后改 30） | migrations/025_wsw_exchange.sql:47,54,56,115,127,141；crates/wsw-message/src/exchange.rs:47,90；crates/wsw-runtime/src/persist.rs:674,824,893-900,908 |
| A6 023 CAS 底座 | wsw_body_blob :33（sha256 小写 hex PK + uri CHECK 固定相对路径 `wsw/<前2>/<sha256>` + size_bytes + mime_type 非空**无 CHECK 词表** + first_seen_exchange_id + created_at）；cas.rs store_body :133（cap 帽 + 原始字节 sha256 登记）/ read_body :265；BodyRef :34（sha256/uri/size/fidelity）；GC 引用计数 delete_body_blobs_unreferenced（023 :55 注释面；执行位留 retention 批） | migrations/023_wsw_body_blob.sql:33-40,55；crates/wsw-message/src/cas.rs:34,133,265 |
| A7 diff 原语（Comparer 底座） | diff.rs 505 行纯 std 零依赖：DiffOp :25 / DiffHunk :38 / NormalizeRules :53（skip_line_substrings + normalize_keys 两维——子串/键值掩码形态**无 regex**）/ diff_lines :97（LCS） / similarity_ratio :271 / normalize_text :370 / diff_normalized :391；E.7 = WSW-4-BATCH-REPORT.md:248「diff 纯 std 面（无 regex）……W6 Comparer 若需正则忽略面须在其批内补（diff 原语保持零依赖红线）」 | crates/wsw-message/src/diff.rs:25,38,53,97,271,370,391；docs/rip3/wsw/WSW-4-BATCH-REPORT.md:248 |
| A8 mutator 变换链（Decoder 底座） | mutator.rs 412 行：EncodeKind :22 五值（Url/Base64/Base64Url/Hex/HtmlEntity——注释「PLAN :287 词表对齐」）/ Mutator :54 / Processor :74 / b64 字母表 :83-88（std 内嵌）/ url_encode :120 / html_entity_encode :134（五实体）/ 编码链 apply :155-157 / apply_mutators :226 / apply_processors :236；regex 转正 = wsw-runtime/Cargo.toml 末行 `regex = { version = "=1.13.1" }`（WSW-4 D4/D9）+ Cargo.lock :2231-2232 实值 1.13.1 | crates/wsw-runtime/src/fuzz/mutator.rs:22,54,74,83-88,120,134,155-157,226,236；crates/wsw-runtime/Cargo.toml（regex 行）；Cargo.lock:2231-2232 |
| A9 rp-wsw CLI 面 | 子命令分派 :815-830（listener/send/fuzz/ca{status,export,regenerate}/db{migrate}）+ usage :33 + parse_args :81；`"wsw-lis-repeater"` 常量先例（WSW-4-TASK D1 :18 引 rp-wsw.rs:505）；rp-wsw.rs:375/:524 = assert_watermark **调用点**非钉值（MA-4-TASK A16 两假阳性排除词形） | crates/wsw-runtime/src/bin/rp-wsw.rs:33,81,815-830,505,375,524 |
| A10 rp_server API 面 | History 游标分页 :3715 / 详情 :3833 / body 按需 :3873 / PATCH 写权 :3906 / status :3940；WSW-4 fuzz 四端点 :4410 起（spawn rp-wsw fuzz 子进程先例）；SSE 面 :47（MA-4 D1/D2/D3 事件推送，本批零改动）；既有 /api/wsw/* 词形 = history/ca/proxy/intercept/status/repeater/fuzz | apps/server/rp_server.py:47,3715,3833,3873,3906,3940,4410-4411 |
| A11 MA-4 SSE 裁决 | D1 :16「SSE 单面落地，WS 零实现」——WS 三否（stdlib 无 WS server / tokio-tungstenite 在 lock 但 Rust WS server = 第二进程第二端口第二事件面违红线 / 升 protocol_version 全端点语义变更）；D2 :17 事件源三形态 cursor（append-only rowid / updated_at 窗口 / 活跃态快照 diff）；D3 :18 订阅模型 + 词表 run_state/claim_state/message/guardrail/audit additive；D7 :22 实时性单独数字化「WSW-3 性能门管 UI 页面管不到流式推送」 | docs/phase0/MA-4-TASK.md:16,17,18,22 |
| A12 链尾硬钉与路由钉 | MA-4-TASK.md:43 A15 十六文件 29 钉位全清单（migration_idempotent.rs:87 / migrations_agent_data_plane.rs:83,100,109 / backup_restore.rs:51 / rp-mcp-parity caller_auth.rs:209 / plugin-runtime lifecycle_full_path.rs:604,610 / workflow-engine migration_replay.rs:148-149 / **wsw-message/src/store.rs:203** / wsw-message/tests/cas_e2e.rs:132,139,177,190 / **wsw-runtime/src/persist.rs:895-899** / wsw-runtime/tests/exchange_persist.rs:3,85 / intercept_queue.rs:47 / fuzz_job.rs:145 / migration_dev_copy_p7_3.rs:76,85,92）；030 立项 = 十六文件全随更（全词形扫描——WSW-4 E.2 :245 教训）；路由钉 web/tests/workspaces.test.ts:109 `toBe(22)`（W3-D8 20→22 设计内扩展现值） | docs/phase0/MA-4-TASK.md:43；crates/wsw-runtime/src/persist.rs:895-899；crates/wsw-message/src/store.rs:203；web/tests/workspaces.test.ts:109；docs/rip3/wsw/WSW-4-BATCH-REPORT.md:245 |
| A13 依赖面 | Cargo.lock 399（本会话 grep 实测）+ reqwest=0；tokio-tungstenite 0.30.0 :3008 / tungstenite :3218 / hudsucker 0.25.0 :1041 / hyper-tungstenite :1115——消费方五形（wsw-runtime/wsw-poc/hudsucker/hyper-tungstenite/tokio-tungstenite 自身）；**flate2/brotli/zstd lock+vendor 双零命中**（本会话实测——Gzip/Deflate/Brotli 裁剪依据）；wsw-message Cargo.toml sha2+base64 workspace 声明在位（WSW-1 转正先例）+ wsw-runtime path 依赖 wsw-message 在位（Cargo.toml 末行）；vendor/ = hudsucker/hyper-tungstenite/tokio-tungstenite/tungstenite 在库 | Cargo.lock:1041,1115,3008,3218；crates/wsw-message/Cargo.toml；crates/wsw-runtime/Cargo.toml；ls vendor/ |
| A14 迁移空位 | `ls migrations/030*` = No such file（本会话反证）；001-029 连续；028 立项先例 = WSW-4-TASK D2 :19（单文件单版本纯新增 + 链尾硬钉随更预登记词形）；PG 侧 V0008 不涉及（WS 帧/命中零 PG 面） | ls migrations/ 实测；docs/rip3/wsw/WSW-4-TASK.md:19 |
| A15 web/UI 面 | web/src/routes/ Wsw 页 4 个 = WswCertPage/WswHistoryPage/WswProxyPage/WswRepeaterPage（W2/W3 落点）；AI-IMPLEMENTATION-GUIDE.md = docs/ui-ux/ 743 行，grep `decoder|comparer|workbench` 零命中（本会话实测）；web/scripts/ e2e = rip3_wsw2_browser_e2e.mjs / rip3_wsw3_browser_e2e.mjs（WSW-4 无 UI 无 browser e2e——零 wsw4 脚本）；apps/server selftest = selftest_wsw4.sh 唯一 wsw 轨脚本（:24-27 BIN=rt3-target debug rp-wsw + python3 stdlib 靶机先例） | ls web/src/routes/ web/scripts/ apps/server/ 实测；docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md（743 行）；apps/server/selftest_wsw4.sh:24-27 |
| A16 S1 基线 | HEAD `aafb87e9`（DI-3/RT-4 收口 commit：29 份迁移 + Resolver 接线 + ffprobe 重钉，自述「workspace MA-3 以来首次零基线红」）；`git status --porcelain` = 0 行（本会话实测）；workspace 全量 `cargo test --offline --workspace --no-fail-fast` **exit 0 零红**（本会话实测）；Cargo.lock 399 恒定 | git log/status 实测；本会话后台全量任务 exit 0 回执 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `migrations/030_wsw_ws_frame.sql` | 新建 | D2：wsw_ws_frame + wsw_passive_hit 两表 + 索引 + schema_version(30) + `PRAGMA user_version=30`；纯新增零 ALTER | S2 |
| 链尾十六硬钉文件（A12 清单） | 修改 | D2：29→30 全词形随更（vec 链清单 / Some(30) / `001..030`·`010..030`·`013..030` 注释 / assert 30 / watermark 30 / `v != 30` 断言——全形态扫描禁抽样） | S2 |
| `crates/wsw-runtime/src/listener/handler.rs` | 修改 | D3：WswHandler WS 帧结构化捕获（opcode/payload/masked 提取 + 连接 endpoint 关联）→ capture 通道投递；handle_message 挂点扩展（Some 透传语义不变） | S3 |
| `crates/wsw-runtime/src/persist.rs` | 修改 | D2/D3：wsw_ws_frame 行读写面（insert/游标分页查询/按 connection 聚合）+ wsw_passive_hit 行写读面 + assert_watermark 29→30 + WS 握手行落库复用 insert_exchanges | S2/S3 |
| `crates/wsw-runtime/src/wsframe.rs` | 新建 | S3：帧类型面（opcode 词表 enum / WsFrameRecord / tungstenite Message ↔ 记录映射纯函数 + 单测） | S3 |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改 | D4：`ws` 子命令（connect 主动客户端 + replay 帧重放）+ `decode` 子命令 + `compare` 子命令（usage :33 同步） | S4/S5/S6 |
| `crates/wsw-runtime/src/decoder.rs` | 新建 | D5：Decoder 变换链（五变换双向 + Unicode escape + JWT 结构 + JSON 美化/压缩/校验 + sha2 + 多级链纯函数 + 单测含往返恒等负例） | S5 |
| `crates/wsw-runtime/src/comparer.rs` | 新建 | D6：Comparer 面（regex 忽略预处理 + 文本/JSON/Header/Hex 四 Diff + 相似度 + sha256 相同判定 + artifact 保存接线） | S6 |
| `crates/wsw-runtime/src/passive.rs` | 新建 | D7：内置规则注册表（五类起步词表 + 匹配纯函数 + rule_id/version）+ 逐行分析入口（exchange 行 → hit 行） | S7 |
| `crates/wsw-runtime/src/fuzz/persist.rs` 及 capture 链挂点（`listener/handler.rs` capture 通道） | 修改 | D7：Passive 分析旁路接线（异步 worker 侧逐行后处理，listener 转发路径零同步调用） | S7 |
| `crates/wsw-runtime/src/lib.rs` | 修改 | 模块接线（pub mod wsframe/decoder/comparer/passive——WSW-4 E.1 :242「追加声明属不可免接线」登记口径） | S3-S7 |
| `apps/server/rp_server.py` | 修改 | D4：七端点（ws/frames 游标分页 + ws/frames/{id} + ws/replay spawn + decode + compare + passive/hits 游标分页 + passive/rules）+ Bearer 鉴权同面 + SSE 面零改动 | S4-S7 |
| `apps/server/selftest_wsw56.sh` | 新建 | 判据收口脚本（selftest_wsw4.sh :1-27 形状先例：临时库 + 临时 RP_ARTIFACTS_DIR + python3 stdlib 靶机；**WS 靶机 = python3 手工 RFC6455 最小 echo server**（stdlib hashlib sha1 算 Sec-WebSocket-Accept + 掩码帧解析 ~80 行，零新依赖；Rust 集成测试面另用 tokio-tungstenite server 夹具承载协议全貌）+ V-9 守卫预挂） | S8 |
| `crates/wsw-runtime/tests/ws_frame.rs` | 新建 | S3：集成测试（tokio-tungstenite 内存 server/echo 夹具 + 透传捕获断言 + opcode 全词表 + close 观察面） | S3 |
| `crates/wsw-runtime/tests/decoder_comparer.rs`、`crates/wsw-runtime/tests/passive_audit.rs` | 新建 | S5/S7：往返恒等矩阵 / 差异归因矩阵 / 命中对账矩阵（判据 4/5/6 承载） | S5/S6/S7 |
| `docs/rip3/wsw/WSW-5-6-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次报告（:607/:608 交付列逐项对照 + 命令原文 + 判据对照 + 坏消息单列 + WSW-7/WSW-8 前置输出登记节）+ 索引登记 | S9 |

### 2.2 不修改

- `crates/wsw-runtime/src/sender.rs`、`crates/wsw-runtime/src/scope.rs` **逐字节零 diff**（发送引擎单点 + scope 闸词表延续——WSW-4-TASK D1 :18 / DI-3-RT-4-TASK A13 跨轨禁改延续；scope 调用零新增面即零改动，ws connect 子命令消费既有 ScopeRules::from_json :179）。
- `crates/wsw-message/` 全 crate 零 diff（diff.rs 原语 E.7 零依赖红线 + cas.rs/exchange.rs/parse.rs/serialize.rs 既有面纯消费；store.rs:203 链尾钉 29→30 属 A12 清单随更——该文件是唯一 wsw-message 落点例外，S2 全词形随更承载）。
- `crates/wsw-runtime/src/fuzz/`（mod/spec/mutator/analyzer/persist 五文件）零 diff（mutator.rs 纯函数**只读消费**——D5 复用面；fuzz/persist.rs 若 Passive 旁路需要接线按 §2.1 明列最小面走，analysis 挂点优先落 passive.rs 自持 + capture 链既有 worker 一处接线）。
- 拦截队列面零改动（024 wsw_intercept 词表/行语义/expire 全不动——帧不入拦截队列，A4）；025 全表零 diff（protocol='ws' 与 source='proxy_capture' 词形既有直用）。
- `crates/audit-engine/` 零触碰（W2-D9 词形延续）；`crates/tool-engine/`、`crates/agent-engine/`、`crates/workflow-engine/`、`crates/mcp-server/` 零 diff（MA 轨/RT 轨跨轨禁改）；`integrations/rtis/**` 零 diff。
- `web/**` 全目录零 diff（D8：路由钉 22 零随更 + 4 个 Wsw 页零触碰 + 零 npm 依赖）；rp_server SSE 面（rp_server.py:47 起 /api/events）零改动（D9）。
- `Cargo.lock`、workspace 根 `Cargo.toml`、两 Cargo.toml（wsw-runtime/wsw-message）**零 diff**（D10：regex 已转正零再动、tokio-tungstenite 走再导出、base64/sha2 走 wsw-message 既有声明）；`vendor/` 零 diff。
- `migrations/001-029` 零 diff（030 单文件纯新增；029:121 kind 词表冻结不扩——本批零 MA kind 面）；PG 迁移链 V0001-V0007 零触碰；Gzip/Deflate/Brotli 解压零 vendor 零实现（D5 裁剪）。
- `migrations/platform*.db` 生产库与 `platform-dryrun.db` 零触碰（只读指纹反证）；测试/selftest 一律临时库副本（dev-b1 → migrate 链先例 selftest_wsw4.sh:29-32）。
- HAR 导入导出（PLAN :587-594）零实现（W3-TASK :21「留后续批」词形延续；exchange.rs Import 词形已备但本批零导入面）；Hudsucker/chromiumoxide 候选零引入（PLAN §十候选未选，:640 归 WSW-8）。

## 3. 实施步骤（S1→S9 串行，每步过门再进）

1. **S1 门禁基线核验**：`grep -cE '^\[\[package\]\]' Cargo.lock` = 399 + `git status --porcelain` 干净 + HEAD = `aafb87e9`；`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` **现状全量一次基线**（预期零基线红——A16 实测复证；如有红先停批上报）；A1-A16 锚点函数名重定位复核；`grep -c 'name = "reqwest"' Cargo.lock` = 0 + flate2/brotli/zstd 零命中复证；rp-wsw debug 二进制在位复核（selftest 先例 :25）。
2. **S2 030 迁移面**：`migrations/030_wsw_ws_frame.sql` 落盘（D2 两表形状）+ `rp-wsw db migrate` 开发库副本推进 29→30 + 幂等重跑复测；**链尾十六硬钉全词形扫描随更**（A15/MA-4-TASK:43 清单 grep `\b29\b|Some\(29\)|1\.\.29|013\.\.029|010\.\.029|, 29,|== 29|!= 29` 全形态——WSW-4 E.2 :245 教训原文口径；persist.rs assert_watermark 词形 `v != 29`→`v != 30` + 消息串同步）；**门 = wsw-message/wsw-runtime/database 全部迁移测试绿 + user_version=30 断言绿**。
3. **S3 WS 帧捕获面（WSW-5 传输面①）**：wsframe.rs 类型面（opcode 词表 + WsFrameRecord + Message 映射纯函数）→ handler.rs 捕获扩展（:2100 挂点内 opcode/payload/masked 提取 + connection_exchange_id 关联 + WS 握手行 protocol='ws' 落 025 面复用 insert_exchanges）→ persist.rs wsw_ws_frame 写读面（Payload 经 cas::store_body 023 链 + 帧序 sequence_no 连续性）；**门 = `cargo test --offline -p wsw-runtime -- --test-threads=1` 绿（新增 ws_frame.rs 集成：tokio-tungstenite 内存 echo 夹具透传捕获断言 + opcode 全词表 + ping/pong/close 观察 + 帧序连续）**。
4. **S4 WS 重放与 CLI/API（WSW-5 工作台面）**：rp-wsw `ws` 子命令（connect 走 scope 闸前置 + replay 直连重发）→ rp_server 七端点之三（frames 游标分页/详情/replay spawn——W3-D3 协议 + :4410 spawn 先例）；**门 = selftest 节 PASS：经代理真实 WS 连接（python3 RFC6455 echo 靶机）→ 帧行落库对账 + replay 单帧靶机 delta=1 + API 401 未授权负例**。
5. **S5 Decoder 面（WSW-6①）**：decoder.rs 变换链（D5 词表 12 项）→ rp-wsw `decode` 子命令 → POST /api/wsw/decode；**门 = decoder_comparer.rs 往返恒等矩阵全绿（每变换 encode∘decode=identity 与 decode∘encode=identity 双向 + 非法输入负例：非 hex 奇长/非法 base64 字符/超界 Unicode escape → 结构化拒绝）**。
6. **S6 Comparer 面（WSW-6②）**：comparer.rs（regex 忽略预处理 + 四 Diff + 相似度）→ rp-wsw `compare` 子命令 → POST /api/wsw/compare；**门 = 差异归因矩阵全绿（增删改区间归因正确 / normalize_keys 掩码收敛 / regex 剔除后区间收敛 / JSON 键序不敏感 / Hex 首差异偏移 / 负例：长度相同内容不同 → 相同判定 false——PLAN :303）**。
7. **S7 Passive Audit 面（WSW-6③）**：passive.rs 规则注册表（五类起步）→ capture 链旁路接线（异步 worker 侧，listener 转发路径零同步调用）→ wsw_passive_hit 写面 → rp_server passive 两端点；**门 = passive_audit.rs 命中对账全绿（N exchange → 命中行数对账 + CAS/023 行数对账 + 不阻塞转发延迟面 + 候选态零 evidence 写入负例）**。
8. **S8 selftest_wsw56.sh 收口**：全节实跑（判据 1-10 数字来源；V-9 守卫预挂：rp-wsw bin / dev-b1 副本 / 靶机缺 → SKIP 计数 exit 0——0f2922cb 教训口径）；**门 = PASS/SKIP 分开计数 exit 0 + 既有 selftest_wsw4.sh 零改动回归绿**。
9. **S9 全量回归收口**：§5 全部命令实跑（workspace 全量一次——预期零基线红延续）；`git status --porcelain` 只含 §2.1 落点核对（web/** / vendor/ / Cargo.lock / sender.rs / scope.rs / wsw-message src 零 diff 实证）；批次报告落盘 + docs/README.md 登记（显式 add 本批清单，禁 add -A）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **WS 透传捕获对账**：经代理真实 WS 连接（python3 RFC6455 echo 靶机 + Rust tokio-tungstenite 夹具双面），帧行落库与线缆帧一一对应 | wsw_exchange 握手行 protocol='ws' = 1 + wsw_ws_frame 帧行数 = 靶机实发帧数（对账断言）+ opcode 词表逐帧正确（text/binary/ping/pong/close）+ 帧序 sequence_no 连续零缺口 + direction 归向正确 |
| 2 | **透传保真 + 编辑重放**：echo 往返字节一致；replay 编辑单帧对端实收一致 | Rust 夹具 payload 字节逐字节断言；`rp-wsw ws replay` 编辑帧 → 靶机 delta 计数 +1 且收到的 payload = 编辑值；未编辑 replay 原样 |
| 3 | **030 迁移与链尾钉**：单文件纯新增 + 十六文件全随更 | `sqlite3 file:dev.db?mode=ro "PRAGMA user_version"` = 30；幂等重跑零异常；A12 十六文件 grep 全词形 =30 零残留 29（除历史注释引用面）；`git diff --stat migrations/` 只 030 单文件 |
| 4 | **Decoder 往返恒等（负例硬门）**：五变换双向恒等 + 多级链 + JWT/JSON/Unicode/sha2 面 | decoder_comparer.rs 矩阵全绿：encode(decode(x))==x 与 decode(encode(x))==x 逐变换双断言 + 链序往返 + 负例（非法 hex 奇长/非法 base64/超界 escape）结构化拒绝；JWT 三段解析展示 + 不验签断言 |
| 5 | **Comparer 差异归因（负例硬门）**：四 Diff + 三维忽略面 | 差异归因矩阵全绿：文本增删改区间逐段断言 + normalize_keys 掩码收敛 + regex 剔除（skip_line_regexes/normalize_key_regexes）后区间收敛归因正确 + JSON 键序不敏感 + Header 逐键 + Hex 首差异 OFFSET + 相似度数值断言 + 负例「长度相同内容不同 → sha256 判相同=false」（PLAN :303） |
| 6 | **Passive 捕获对账 023（候选态负例硬门）**：命中行对账 + CAS 对账 + 零提升 | N exchange 捕获 → wsw_passive_hit 行数与规则预期对账 + wsw_body_blob 行数与 CAS 文件数对账（sha256 寻址幂等 INSERT OR IGNORE）+ 转发不阻塞（WS 透传延迟面复测）+ 负例：hit 行零 evidence_id 零 Finding 写面（表结构断言 + 全链 grep 无 evidence 写入） |
| 7 | **独立入口面**：CLI 子命令 + API 端点双面可用 | `rp-wsw ws --help` / `rp-wsw decode --help` / `rp-wsw compare --help` exit 0；七端点 Bearer 鉴权（401 负例）+ 游标分页零重零漏（1000 帧遍历断言——W3 判据同型）+ 过滤指纹守卫（filter_cursor_mismatch 400 负例） |
| 8 | **scope 闸延续**：WS 面零越闸 | ws connect 子命令 scope 外目标 → 拒绝（scope.rs:234 code() 词形）；透传捕获帧行 scope_decision 三态沿用 wsw_exchange 同列语义；scope.rs 零 diff 实证 |
| 9 | **性能门**：代理增量 <50ms + 新 API p95 <300ms | 直连 vs 经代理 WS 握手+帧透传场景 curl/计时中位数差 <50ms（repeat≥3）；GET /api/wsw/ws/frames（1000+ 帧首页）/ /api/wsw/decode / /api/wsw/passive/hits 20 采样 p95 <300ms——全部数字命令回显（PLAN :754） |
| 10 | **零依赖与跨轨零 diff** | Cargo.lock 399 恒定 + vendor 零 diff + 两 Cargo.toml 零 diff + reqwest=0 + web/** 零 diff + sender.rs/scope.rs/audit-engine/mcp-server/tool-engine 零 diff + 路由钉 22 零随更 + `git status --porcelain` 只含 §2.1 落点 |
| 11 | **全量回归** | `cargo test --offline --workspace --no-fail-fast` 全绿（零基线红延续）+ 既有 selftest_wsw4.sh 零改动绿 + clippy -D warnings 通过 + deny 双检通过 |
| 12 | **WSW-7/WSW-8 前置输出** | 批次报告专节：①passive rule_id 词表 + 规则注册表投影（WSW-7 Scanner 集成时被动/主动入口统一面输入）；②七端点响应形状词形（WSW-8 UI 批零后端改动输入——G.1 交接词形延续）+ GUIDE 零命中登记 + 四 UI 页面规格缺口清单 |

## 5. 测试面（收口命令）

```bash
# Rust 全量（2026-11-22 纪律：workspace 一次必须跑；本批基线 = DI-3/RT-4 收口零红态）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 定向：WSW 轨两 crate（串行口径；ws_frame 集成含真实 socket 面）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-message -- --test-threads=1
# 定向：database 迁移面（030 幂等 + 链尾钉）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p database -- --test-threads=1
# 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings
cargo fmt --all -- --check
# deny / lock / 红线现状
cargo deny check advisories && cargo deny check licenses
grep -cE '^\[\[package\]\]' Cargo.lock    # 399（HEAD aafb87e9 对比基准）
grep -c 'name = "reqwest"' Cargo.lock    # 0
grep -c "flate2\|brotli" Cargo.lock      # 0（解压裁剪依据反证）
# 迁移水位与链尾钉（S2/S9 双点）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw4.sh   # 既有零改动回归（其内 user_version 断言 28→随 S2 保留 wsw4 自有断言域不动）
ls migrations/030* 2>&1 | grep -vc "No such file"   # 1（030 落地后）
grep -rn "user_version=29\|, 29,|Some(29)" crates/wsw-runtime/src/ crates/wsw-message/src/ | grep -vc "030"   # 0（链尾钉随更后残留反证；全词形扫描在 S2 展开）
# 本批新增收口（V-9 守卫预挂；PASS/SKIP 分开计数）
bash apps/server/selftest_wsw56.sh
# 跨轨零 diff 反证（S9）
git status --porcelain | grep -c "web/\|vendor/\|sender.rs\|scope.rs"   # 0
git diff HEAD --stat -- Cargo.lock crates/wsw-runtime/Cargo.toml crates/wsw-message/Cargo.toml   # 空
```

基线记录（本任务书写作时点 2026-11-22 实测）：

- workspace 全量基线 = **exit 0 零红**（HEAD `aafb87e9` 干净树实测——DI-3/RT-4 收口后首次全绿态，本批红起点 = 零）。
- 030 空位在（`ls migrations/030*` No such file 反证）；028/029 双先例（WSW-4-TASK D2 + 029 入链 MA-3）。
- S2 与链尾随更同 commit 纪律（迁移与钉值随更不拆批，防中间态红窗口——WSW-4 E.2 教训词形）。

## 6. 红线（八条，违反任一即停批上报）

1. **Rust-First + 零第二 WS 栈**：WS 帧/Decoder/Comparer/Passive 引擎全 Rust（wsw-runtime/wsw-message）；rp_server 零 WS server 零新端口零第二事件面（MA-4 D1 :16 三否词形同门）；Python/Node 只允许 rp_server API 面与 e2e/selftest 骨架（既有模式）；RFC6455 靶机 = selftest 测试夹具面非生产实现。
2. **零新依赖 vendor 政策**：Cargo.lock 399 恒定 + vendor/ 零 diff + 两 Cargo.toml 零 diff（regex=1.13.1 WSW-4 已转正零再动；tokio-tungstenite 走 hudsucker 再导出 vendor lib.rs:51；base64/sha2 走 wsw-message 既有 workspace 声明）；Gzip/Deflate/Brotli 解压零 vendor 零实现（D5 裁剪登记——vendor 立项属后续批）；reqwest=0 保持（S1/S9 双点）。
3. **不建第二发送引擎 / 不建第二代理栈**：sender.rs 逐字节零 diff；HTTP 重放面唯一路径 = 既有 send 子命令；WS 重放面 = hudsucker sink 挂点单点（D3）；fuzz/sender/scope 三既有词表面零复制；拦截队列（024）面零改动零混用（WSW-4-TASK D7「两词表不混」延续）。
4. **迁移禁改 + 030 立项单文件 + 链尾钉全随更**：migrations/001-029 零 diff；030 单文件单版本纯新增零 ALTER；十六硬钉文件全词形扫描随更 29→30（A12 清单；禁抽样——WSW-4 E.2 :245 教训原文口径）；029:121 kind 词表冻结不扩；PG 链零触碰。
5. **生产库零触碰 + RTIS 零接触**：`migrations/platform*.db` 与 `platform-dryrun.db` 全程不写不读（只读指纹反证进报告）；`integrations/rtis/**` 零 diff；被动命中候选态零 Finding 提升零 evidence 写面（判据 6 负例承载）。
6. **性能门词表**：代理增量 <50ms（WS 场景在环实测）+ 新 API p95 <300ms（20 采样）为本批适用门；首屏<3s/交互<1s 随 WSW-8 UI 批（D10 如实登记不冒充适用）；全部数字真实命令回显禁预填（PLAN :754）；虚拟滚动/分页强制在 API 面 = 游标分页禁 OFFSET（W3-D3 协议）。
7. **边界纪律（观察池不进）**：DI-3 七缝主体（facade 审计锚/统一信封/内存域/媒体扩展/诚实分支）不进（E1-E5 观察池）；HAR 导入导出不进；JSON 规则文件加载面不进（观察池）；SSE additive kind 扩展不进（MA 轨词表域，D9）；chromiumoxide/浏览器面不进（WSW-8）；主动扫描 Provider 不进（WSW-7）；UI 四页不进（WSW-8）——做了即越界。
8. **测试离线 + --test-threads=1 + 全量一次 + 禁改既有测试钉值**：一律 `--offline --test-threads=1` + `CARGO_TARGET_DIR=/home/emo/rt3-target`；workspace 全量一次收口门（判据 11）；selftest_wsw4.sh 等既有脚本零改动回归；V-9 守卫预挂（bin/副本/靶机缺 → SKIP 计数 exit 0）；测试红只修代码不改钉值（REDLINES 红线 6；路由钉 22/链尾钉随更属设计内扩展同 W3 词形）；禁外联（靶机全 loopback；REDLINES 红线同 W2-TASK §五.10 口径）。

## 7. 回滚

- 单 commit 链交付（S2-S7 引擎面同批 commit 纪律；selftest/报告可分 commit）；`git revert` 整批撤销。
- 030 随 revert 回到 029 链尾（十六文件钉值复原 29；revert 说明登记 030 空位重开）；开发库副本/测试内存库零生产残留（revert 后无 DROP/清理步骤）。
- handler.rs 捕获扩展随 revert 回 :2100 透传现状（日志词形保留——透传挂载面 WSW-2 既有资产）；wsframe/decoder/comparer/passive 四新模块随 revert 零残留；lib.rs 模块声明行随 revert 清零。
- rp_server 七端点随 revert 消失（既有 /api/wsw/* 面零触碰）；selftest_wsw56.sh 随 revert 零残留。
- 生产库/RTIS/宿主软件零操作无生产回滚面；web/**/vendor/ 零 diff 无前端回滚面。

## 8. 交付物

1. **迁移面**：migrations/030_wsw_ws_frame.sql 单文件（wsw_ws_frame + wsw_passive_hit 两表 + 索引 + user_version=30）+ 链尾十六硬钉全词形随更——`git diff --stat migrations/` 单文件实证；001-029 零 diff。
2. **WSW-5 传输/工作台面**：handler.rs 帧结构化捕获 + wsframe.rs 类型面 + persist.rs 帧行读写 + rp-wsw ws 子命令（connect/replay）+ rp_server frames/详情/replay 三端点——握手行 protocol='ws' 对账 + 帧行对账 + 透传保真 + 编辑重放全断言（判据 1/2/3）。
3. **WSW-6 三工具面**：decoder.rs（12 项词表 + 往返恒等）+ comparer.rs（regex 忽略面 + 四 Diff）+ passive.rs（五类规则注册表 + 候选命中）+ rp-wsw decode/compare 子命令 + rp_server decode/compare/passive 四端点（判据 4/5/6）。
4. **测试面**：tests/ws_frame.rs + tests/decoder_comparer.rs + tests/passive_audit.rs 三集成测试 + selftest_wsw56.sh（V-9 预挂）+ 判据 1-12 全部数字命令回显入报告。
5. **文档面**：docs/rip3/wsw/WSW-5-6-BATCH-REPORT.md（:607/:608 交付列逐项对照 + 命令原文 + 坏消息单列——Gzip 裁剪/规则文件/SSE kind/UI 四页四项必列）+ docs/README.md 索引登记。
6. **WSW-7 前置输出登记**（批次报告专节）：①passive rule_id 词表 + 注册表只读投影（WSW-7 Scanner/Provider 统一入口 PLAN :30 的被动侧输入——主动 Provider 接线时禁第二规则注册表）；②七端点 Bearer 鉴权面与错误词形（WSW-7 Tool Contract 对接输入）。
7. **WSW-8 前置输出登记**（批次报告专节）：①七端点响应形状词形全清单（UI 批零后端改动输入——WSW-4 G.1 :278 交接词形延续）；②AI-IMPLEMENTATION-GUIDE 零命中登记 + 四 UI 页面规格缺口清单（WS 帧展示 :733 / Comparer :732 / Decoder / Passive——届时按 PLAN §5.2 :345-349 九入口 + yakit 参考图扩展设计）；③性能门交接（首屏<3s/交互<1s 在 UI 批适用词形 + 本批代理增量/p95 实测基线数）。
8. **下一批输入**：Gzip/Deflate/Brotli vendor 立项面（D5 裁剪遗留）+ JSON 规则文件加载面 + SSE additive kind 扩展面（MA 轨域）+ 帧级实时推送观察池——登记观察池，各按独立批立项（本批零实现）。
