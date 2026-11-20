# WSW-3 任务书 · HTTP History + Repeater

- 批次：WSW-3（PLAN §九 :605：History 列表详情、过滤搜索、请求编辑、重放、Tab 历史；**不包含大规模 Fuzz**）
- 日期：2026-11-20
- 上位：`docs/rip3/wsw/WSW-3-ADR.md`（本批八项裁决 W3-D1～D8，**表形状/分页协议/发送引擎/组件选型/Send 语义/写权分治以它为准**）、`docs/rip3/wsw/WSW-2-ADR.md`（W2-D4 exchange 表留 W3 / W2-D5 查点 B 帽接线留 W3 / W2-D9）、`docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md`（§1.2 Exchange 字段 / §3.2 表归属 / §3.4 retention 草案）、`docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`（八步循环 + 九条红线）、PLAN §4.2/§4.3/§5.3/§5.4/§6.3/§6.4/§7.2/§11.3/§11.6
- 状态：已实施收口（2026-11-20；主体经中断点快照 `9aff134a` 入库，续跑定向门禁全绿 + e2e 21/21 + 性能实测 + 批次报告 `docs/rip3/batches/WSW-3-BATCH-REPORT.md`）
- 硬验收口径声明：原手册 `/media/emo/code/pensting/WSW-CONTINUATION-WORKFLOW.md` 已丢失（`WSW-CONTINUATION-REDLINES.md:3-5` 固化结论），其"§4 W3 行"按 REDLINES 第 5 条从仓库内可考来源重建：PLAN §九 :605 + WSW-2-ADR W2-D4/D5/D9 + DESIGN §3.2 + 本任务书 §四。外部口径冲突以 PLAN / WSW-3-ADR / 本档为准。

---

## 一、目标（可验收定义）

1. **025 exchange 持久化**：`migrations/025_wsw_exchange.sql` 纯新增四表（wsw_exchange / wsw_repeater_tab / wsw_repeater_tab_version / wsw_repeater_send），链头 024→025 单文件推进；单行纪律（一 exchange 一行，响应未到 = 响应列 NULL + parse_status='partial'）；Body 大内容走 CAS 不入行（行内只落 sha256/uri/len/mime/fidelity 派生列）。
2. **capture→落库链路**：listener 捕获经消息模型解析后异步落 exchange 行——有界 mpsc + 单 worker + 批量事务；满载丢弃计数可见；**代理增量 <50ms 性能预算不得因落库破坏**（直连 vs 经代理 curl time_total 中位数差实测）；WAL 启用（`database::Pool::enable_wal`，`pool.rs:258`）。
3. **History API**：游标分页（(started_at, exchange_id) DESC + base64url cursor + 过滤指纹守卫，禁 OFFSET 禁整表）+ 过滤（域名/方法/路径/状态/MIME/时间区间/来源/标签）+ 全文搜索面（有界 search_text 投影 LIKE）+ Body 按需详情端点（单响应帽 8MiB）+ tags/notes 编辑；列表端点零 body 字节、禁选 headers/notes 大列。
4. **Repeater 发送引擎**：`wsw-runtime` 内 `sender.rs` + `rp-wsw send` 子命令——`wsw_message::serialize` 重放（Outbound 唯一事实，原始 h1 字节写）+ 重定向策略（none/follow max_redirects=10，逐跳 re-serialize + `scope::check_redirect_hop` 帽接线 = W2-G10 遗留收口 + 查点 C IP 审计）+ connect/read/total 超时 + SIGTERM 取消落 cancelled；发送历史 = wsw_repeater_send 行 + exchange(source=repeater) 行 + outbound CAS。
5. **History 页 UI**：虚拟滚动列表（自研窗口化，断言仅视口渲染）+ 过滤构建器（防抖 300ms）+ 详情 Raw/Pretty/Headers/Params/Body/Hex 六视图 + Send to Repeater 行操作。
6. **Repeater 页 UI**：多 Tab（新建/重命名/关闭/版本历史回退）+ 请求编辑器 Raw/结构化双模 + Send/Cancel（发送中态）+ 响应视图 + **Tab 修改历史 append-only 不覆盖**（wsw_repeater_tab_version 唯一 (tab_id, version_no)，禁 UPDATE）。
7. **e2e**：`web/scripts/rip3_wsw3_browser_e2e.mjs`（复制 `rip3_wsw2_browser_e2e.mjs` 模式）：≥1000 exchange 经代理 curl 循环生成 + 虚拟滚动仅视口渲染断言 + 过滤防抖断言 + Body 按需加载断言 + Send to Repeater 联动 + Repeater 发送往返 + 性能硬门（首屏<3s / 交互<1s / API p95<300ms / 代理增量<50ms），任一断言失败 exit 1。

非目标（本批不做，做了即越界）：Web Fuzzer/Intruder（W4，PLAN :606）；WebSocket Workbench（W5）；Decoder/Comparer/Passive Audit（W6）；HAR 导入导出（PLAN §8.3 留后续批）；retention 超帽 LRU 淘汰执行器与 blob GC 执行位（DESIGN §3.4 查询面落本批，执行器留 retention 批，如实登记遗留）；FTS5 搜索索引（W3-D7 显式缓议）；请求头区逐字节 wire tee（W3-D6 否决备选，fidelity 债如实登记）；CodeMirror/monaco/虚拟滚动库/npm 新依赖（W3-D5 零新依赖裁决）；透明代理/HTTP-3（PLAN :604/605 不包含列不变）。

## 二、文件清单草案

| 文件 | 动作 | 内容 |
|---|---|---|
| `migrations/025_wsw_exchange.sql` | 新建 | D1 四表 + 索引 + schema_version(25) + `PRAGMA user_version=25` 双写；纯新增零 ALTER；禁改历史迁移 |
| `crates/wsw-runtime/src/persist.rs` | 修改 | exchange/tab/version/send 四表 store 接口（insert_exchange 批量 / append_tab_version / record_send / update_tags_notes / 025 水位断言）+ WAL 启用接线 |
| `crates/wsw-runtime/src/listener/handler.rs` | 修改 | 请求入口 snapshot → 响应流末 join Exchange → 有界 channel（try_send 满载丢弃计数）→ 单 worker 批量落行；CaptureMeta 扩展（method/scheme/authority/path/头投影/sequence_no/started_at/remote_ip/scope_decision） |
| `crates/wsw-runtime/src/listener.rs` | 修改 | worker 生命周期挂 ListenerShared；sequence_no 原子计数器；persist pool enable_wal |
| `crates/wsw-runtime/src/sender.rs` | 新建 | 发送引擎：serialize_request→h1 字节写（Tcp/tokio-rustls）→parse_wire；重定向逐跳 scope 闸 + 帽；超时/取消；落 send 行 + exchange(source=repeater) + outbound CAS（W3-D4） |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改 | `send` 子命令（--db/--session-id/--tab-id/--send-id/--version-no/--request-json/--policy/--upstream-ca）+ SIGTERM graceful 落 cancelled |
| `crates/wsw-runtime/Cargo.toml` | 修改（如需） | 仅传递依赖转正直接声明 + 等号钉 lock 实值（tokio-rustls 已直依赖 `:66`；预计零新增行——若零新增则本文件不动）；Cargo.toml 头注释 "总数 400" 历史值勘误为 HEAD 对比口径 |
| `Cargo.lock` | 修改（仅依赖清单行） | **`[[package]]` 总数 399 不变（工作树与 HEAD 0339a1eb 实测基准；`git diff HEAD -- Cargo.lock` 仅 wsw-runtime 包依赖清单行）**；vendor 零变更 |
| `crates/wsw-runtime/tests/exchange_persist.rs` | 新建 | 025 水位/单行纪律/批量落行/背压丢弃/WAL 并发读写/tags-notes 编辑 |
| `crates/wsw-runtime/tests/sender_send.rs` | 新建 | 本地靶场发送往返/重定向帽越界拒/超时/取消终态/outbound CAS 逐字节回读/source=repeater 行断言 |
| `apps/server/rp_server.py` | 修改 | History API（GET history 游标分页 + 过滤 + q 搜索、GET history/{id}、GET history/{id}/body 按需、PATCH history/{id} tags/notes）+ Repeater API（tabs/tab 版本列表+追加/tab 关闭、POST send 子进程 spawn、GET send-status、POST cancel、GET sends?tab_id=）；读面 mode=ro、写面 busy_timeout；Bearer 鉴权同既有面 |
| `web/src/routes/WswHistoryPage.tsx` | 新建 | History 页（W3-D8.3）：过滤构建器 + 虚拟滚动 + 详情六视图 + Send to Repeater |
| `web/src/routes/WswRepeaterPage.tsx` | 新建 | Repeater 页：多 Tab + Raw/结构化编辑器 + Send/Cancel + 响应视图 + 版本历史回退 |
| `web/src/components/MessageViews.tsx` | 新建 | Raw/Pretty/Headers/Params/Body/Hex 详情视图组件（从 ResearchEvidencePage `<pre>` 雏形抽取，03 审计 :171 面） |
| `web/src/components/VirtualList.tsx` | 新建 | 自研窗口化列表（固定行高 + overscan，零新依赖） |
| `web/src/workspaces.ts` | 修改 | websec 工作区（order 12）两 tab 注册 |
| `web/src/App.tsx` | 修改 | /websec/history、/websec/repeater 路由 |
| `web/tests/workspaces.test.ts` | 修改 | 路由钉值 20→22（如实记录） |
| `web/scripts/rip3_wsw3_browser_e2e.mjs` | 新建 | §四.3 e2e 全集（W2 e2e 骨架复制改造：自包含起停 + exit 1 + ≥4 截图 `docs/rip3/batches/browser-e2e/wsw-3/`） |
| `docs/rip3/batches/browser-e2e/wsw-3/*.png` | 新建 | 实拍截图 ≥4 张 |
| `docs/rip3/batches/WSW-3-BATCH-REPORT.md` | 新建 | A-K 十一节批次报告（坏消息单列、数字实测带来源、fidelity 债与遗留如实登记） |

不动面：`crates/wsw-message/**`（只消费公开面：parse/serialize/cas/redact/exchange/ids/store）、`crates/wsw-poc/**`（D2 不转正）、`migrations/0{01..24}.sql` 历史（禁改）、`migrations/platform*.db`（零触碰）、`vendor/**`（零变更）、`crates/intel-db/**`（RTIS 零接触）、workspace `Cargo.toml`、既有 WSW-2 端点与两页语义（只增量）。

## 三、批内步骤（S1→S10，每步过门再进）

1. **S1 红线基线核实**：`grep -cE '^\[\[package\]\]' Cargo.lock` = 399 断言 + `git diff HEAD -- Cargo.lock` 空断言 + `grep -c 'name = "reqwest"' Cargo.lock` = 0 + `cargo check -p wsw-runtime --offline` 绿 + `cargo deny check licenses` 绿；开发库副本 user_version=24 水位确认（025 推进起点）；`grep -rn "1\.\.24" crates/ --include="*.rs"` 实勘迁移钉值清单（025 入链待跟面）。
2. **S2 025 迁移 + persist 扩展**：四表 SQL（D1 列 CHECK 逐列）+ persist store 接口（批量 insert / append-only version / send 行）；测试内存库 + dev-b1 临时副本双跑 user_version=25；源库 SHA-256 前后不变（WSW-2 S5 同范式）；迁移钉值 24→25 全跟上——钉值形态为 `vec![1,…,24]` 显式清单 + 链尾断言 `(24,…)→(25,…)`/`Some(24)→Some(25)` + `001..024` 注释 + user_version 断言（5d146e24 七文件先例；实勘扫描 = 链清单 "1, 2, …, 24" 模式 + `001..024`/chain tail 024/`must land 024`/`user_version must be 24` 消息模式，`crates/database/tests/` 与 `crates/agent-engine/tests/` 全扫，不预设文件数）。门：单测绿 + 水位断言。
3. **S3 capture→落库异步面**：CaptureMeta 扩展 + 请求入口 snapshot + 响应流末 join + mpsc(2048) try_send + 单 worker 批量事务 + 满载丢弃计数 + enable_wal；单测：批量落行/背压丢弃计数/partial 行（响应中断）/tags-notes 编辑/WAL 读写并发。门：`cargo test -p wsw-runtime --offline -- --test-threads=1` 全绿。
4. **S4 sender 引擎**：sender.rs（D4 全路径）+ rp-wsw send 子命令；单测 + 本地靶场集成（明文/HTTPS 经 --upstream-ca；重定向帽逐跳 + 越界拒；超时；SIGTERM → cancelled；outbound CAS 逐字节回读断言；scope default deny 拒发）。门：sender_send.rs 全绿。
5. **S5 History API**：rp_server 端点全集（D3/D7）；游标协议单测式断言走 e2e + Python 侧手测脚本（curl 序列：过滤指纹不匹配 400 / limit 帽 400 / 空游标末页）；写权分治落位（tags/notes/tab/tab_version Python 直写）。门：curl 实测回显进批次报告 E 节。
6. **S6 History 页 UI**：VirtualList + MessageViews + WswHistoryPage；`npm --prefix web run build` + `npm --prefix web run test` 绿；虚拟滚动断言面（本地 dev 起后端 + 手测脚本先验，e2e 收口）。
7. **S7 Repeater 页 UI**：WswRepeaterPage + 版本历史（append-only API 面：PUT tab request = 新 version 行断言，UPDATE/DELETE 端点不存在）。
8. **S8 e2e 编写与跑绿**：rip3_wsw3_browser_e2e.mjs（§四.3 全项），≥1000 种子 + 断言全集 + 性能硬门 + 截图 ≥4。
9. **S9 性能实测面**：代理增量 <50ms（直连 vs 经代理同请求中位数差 repeat≥3）；API p95（20 采样 × ≥3 新端点）；History 列表 1000+ 行首屏 <3s；落库吞吐（worker 批量行/秒实测）。全部数字来自真实命令回显，禁预填（PLAN :754）。
10. **S10 收口**：批次报告 A-K（fidelity 债/retention 遗留/FTS5 缓议如实登记）；任务书/ADR/DESIGN 一致性核对；REDLINES 八步循环收口（显式 add 本批清单，禁 add -A）。

## 四、完成标准（逐条可验收；验收 = 命令实跑回显）

### 4.1 定向门禁

```bash
cd /media/emo/code/pensting/research-platform
cargo test --offline -p wsw-runtime -- --test-threads=1          # 全绿（既有 76 项 + 本批新增全量）
cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings   # 绿
cargo deny check licenses                                        # 绿
grep -cE '^\[\[package\]\]' Cargo.lock                           # 399（零新外部包红线；HEAD 0339a1eb 对比基准）
git diff HEAD --stat -- Cargo.lock                               # 仅依赖清单行变化
grep -c 'name = "reqwest"' Cargo.lock                            # 0
python3 -c "import sqlite3,glob
for db in sorted(glob.glob('migrations/*.db')):
    if 'platform' in db.split('/')[-1]: continue
    print(db, sqlite3.connect('file:'+db+'?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])"
# → 至少 dev-b1 为 25；platform*.db 不出现在写入面（user_version 维持 1/1/1/0/0 反证）
git status --porcelain web | grep -q . && { npm --prefix web run build && npm --prefix web run test; }   # web touched 必跑
cd web && node scripts/rip3_wsw3_browser_e2e.mjs                 # web/ 内运行（W2 e2e 头注 :13-14 用法）；exit 0 = 全断言 PASS
```

### 4.2 功能断言矩阵（集成测试断言逐条落库，禁恒真断言）

| 用例 | 断言 |
|---|---|
| 025 单行纪律 | 同一请求+响应落一行；响应中断 → 响应列 NULL + parse_status='partial' |
| Body 不入行 | 行内 body 列仅 sha256/uri/len/mime/fidelity；CAS 文件字节与 sha256 读回一致 |
| 背压 | channel 满载注入 → 转发不阻塞（时延断言）+ persist_dropped_total 计数 + 日志可见 |
| 落库失败放行 | persist 注入故障 → 客户端响应不变 + 计数（W2 先例语义延续） |
| 游标稳定性 | 1000 行分页遍历零重零漏；过滤中变更过滤参数用旧 cursor → 400 filter_cursor_mismatch |
| 列表禁整表 | limit=500 强制；响应行数 ≤ limit；列表响应零 headers_json/body 字节字段 |
| Body 按需 | 列表/详情零 body 字节；/body 端点单次帽 8MiB 超帽 413 + x-wsw-body-sha256 校验头 |
| 发送往返 | OutboundRequest → 靶场响应；outbound CAS sha256 与实际写入字节一致；exchange(source=repeater) 行 + send 行终态 succeeded |
| 重定向帽 | follow 策略第 11 跳拒 + redirect_chain 可见；越界 Location → failed + scope_blocked；none 策略不跟随 |
| scope 拒发 | default deny 会话发送 → state=failed + error_code=scope_blocked + 记录可见（PLAN :528） |
| 超时/取消 | total 超时 → timed_out；SIGTERM → cancelled（非终态时）；六态词表无越界 |
| 版本不覆盖 | 连续 3 次编辑 → 3 行 version；restore N → 新行 origin='restore'；表上无 UPDATE/DELETE 路径 |
| Send to Repeater | History 行动作 → tab 版本 origin='from_history' + source_exchange_id 引用 + 编辑器内容含 method/path |
| 搜索面 | q=keyword 命中 url/头名/体前 4KiB；凭据头值（Authorization 值）不在 search_text |

### 4.3 e2e 验收面（rip3_wsw3_browser_e2e.mjs，exit 1 模式沿 W2 e2e `:60-65/:362-366`）

1. **种子**：rp_server + rp-wsw listener（loopback + scope include=127.0.0.1 + allow_private，W2 e2e `:48/:249-254` 同配置）起后，curl 循环经代理 ≥1000 请求（变体路径/方法/状态覆盖过滤维度）→ `GET /api/wsw/history` total ≥1000 断言。
2. **虚拟滚动**：History 页打开（种子 1000+）→ DOM 行节点数 ≤ 视口窗口+overscan 上界（如 <80）断言 + 滚动后行节点更换断言。
3. **过滤防抖**：过滤输入框 5 次快速键入 → 500ms 窗口内 /api/wsw/history 请求数 ≤2 断言（page.on('request') 计数）。
4. **Body 按需**：列表加载全程零 /body 请求断言；打开详情 Body 视图 → /body 请求恰一次断言。
5. **Send to Repeater 联动**：行操作 → 跳转 Repeater + 编辑器含 method/path + 版本行 origin='from_history'（API 核验）。
6. **Repeater 发送往返**：编辑目标为靶场端点 → Send → 响应视图含 marker + History 出现 source=repeater 行（API 核验）+ send 行终态 succeeded。
7. **性能硬门**：首屏 <3s（W2 e2e :289-293/:319-322 模式）；交互 <1s（:328-332 模式）；新 API p95 <300ms（20 采样 × ≥3 端点 :337-349 模式）；代理增量 <50ms（直连 vs 经代理 curl time_total 中位数差，repeat≥3）。
8. 截图 ≥4 张入库 `docs/rip3/batches/browser-e2e/wsw-3/`（文件存在且非空）；命令原文 + 关键回显进批次报告 E 节。

### 4.4 性能实测面

- 代理增量：直连靶场 vs 经代理同请求（curl `-w` time_total，repeat≥3 中位数差）<50ms——**落库链路在环**（capture→worker→025 行全部启用态测）。
- API p95：/api/wsw/history（1000+ 行游标首页）、/api/wsw/history/{id}/body、/api/wsw/repeater/send-status——20 采样 p95 <300ms。
- 落库吞吐：worker 批量行/秒实测登记（无预设门值，实测进报告 F 节）。
- 其余 PLAN §11.6 项（吞吐/并发/内存峰值）不可达面如实写缺测（:754 禁预填）。

## 五、红线清单（批内任何步骤不得越过）

1. **零 vendor 增量，lock 与 HEAD 对比**：Cargo.lock `[[package]]` = 399 不变（`git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'` 实测基准 399，0339a1eb；WSW-2 档 400 为 SEC-0 76c6feb6 前历史值——以 HEAD 对比为准，不硬编码陈旧数）；`git diff HEAD -- Cargo.lock` 仅 wsw-runtime 依赖清单行；vendor 零变更；新依赖只允许传递依赖转正直接声明 + 等号钉 lock 实值（预期零新增行）。
2. **Rust-First 禁 reqwest**：发送引擎全 Rust（sender.rs/hyper-rustls/tokio-rustls）；`grep -c 'name = "reqwest"' Cargo.lock` = 0 保持；Python/Node 只允许在 rp_server API 面与 e2e 骨架（既有模式）出现（REDLINES 红线 2、PLAN §6.1 :444、§十二 :760）。
3. **025 单行纪律**：一 exchange 一行；链头 024→025 单文件单版本推进；纯新增零 ALTER；禁改历史迁移文件；迁移钉值 24→25 全跟上（钉值形态见 S2：vec 链清单 + 链尾断言 + 注释 + user_version 断言四类，实勘扫描不预设文件数）；只在测试内存库与开发库副本执行。
4. **生产库零触碰**：`migrations/platform*.db` 不写（023 文件头 :30 纪律；水位反证进批次报告 D 节）；`rip-data/` 运行时数据不入库（.gitignore:47）。
5. **RTIS 零接触**：漏洞事实不读写（`crates/intel-db/src/lib.rs:10`）；RTIS 引用只走既有 evidence 链（REDLINES 红线 3）。
6. **测试纪律**：集成测试 `--test-threads=1` + timeout 保护；禁外联（靶场 local_range loopback，W2-TASK §五.10 口径）；测试红只修代码不改钉值（除非坐实过时并如实记录，含 workspaces.test.ts 路由钉值 20→22 这类设计内扩展）；同一失败方法最多两次（REDLINES 红线 7）。
7. **Rust-First 数据面**：exchange/intercept/send 行写权 Rust-only（W3-D7 分治表）；rp_server Python 只读 exchange 面 + 小控制行（tags/notes/tab/version）直写；禁 Python/Node 实现 HTTP 解析、序列化或发送。
8. **Scope 后端强制**：Repeater 发送前 scope 校验 + 重定向逐跳 check_redirect_hop + IP 审计；default deny；云元数据 deny-never-allow；越界拒绝记录可见（PLAN :523-528；W2-D5 词表全延续）。
9. **Outbound 不变式**：发送字节唯一来源 `wsw_message::serialize::serialize_request`（serialize.rs:219）；Display 不发送、hyper 写路径不采用（W3-D4.3 否决）；发送历史逐字节重发仅走 outbound CAS。
10. **敏感面**：凭据头值不进 search_text/日志/错误；展示投影挂 `wsw_message::redact`（redact.rs:55 唯一挂点）；原始行面与脱敏展示分离（PLAN :543-545；DESIGN §4.5）。
11. **不建第二套事实体系**：新表只在 025；六 ID/trace/artifact/evidence 引用走既有列；禁第二套事件流水（024 W2-D4 "行即事实" 延续）；audit-engine Emit 不接线（W2-D9 口径）。
12. **UI 纪律**：零新 npm 依赖（W3-D5）；不建平行设计系统（AI-GUIDE :28/:91）；深浅主题沿 RIP Token（PLAN §5.6 :430）；不用静态假数据伪造流量（PLAN :436）。
13. **收口纪律**：显式 `git add` 本批文件清单（禁 add -A；`.auto/` 不加）；commit/push 按 REDLINES 批次循环自走；CI 红不进下一批（REDLINES 红线 5）；宿主机零系统级安装（REDLINES 红线 8）。

## 六、退出条件与回退

- 定向门禁（§4.1）两轮修复仍红 → 批次挂起，改动留工作树不 commit，批次报告如实记录挂起点（不允许降标准换"通过"）。
- e2e 性能门连续两轮红：代理增量 >50ms → D2 背压/批量参数调优一轮；仍红 → 批次挂起如实记录（不得以"异步了应该快"冒充实测）。
- p95 >300ms 且瓶颈在 LIKE 搜索 → D7 FTS5 升级预案另立裁决（不批内私自换实现）。
- 025 若与 024 链冲突：迁移重新编号（026+），ADR D1 / DESIGN §3.2 同步改号，禁改历史迁移。
- fidelity 债（头区 degraded_no_wire / retention 执行器遗留 / FTS5 缓议）：批次报告 G 节单列，不冒充收口。
- 回退面：本批全部改动限于 025/wsw-runtime 三模块+bin/rp_server 端点/web 四文件+e2e+文档；`git revert` 对应 commit 即回；rip-data/ 运行时数据无回滚面。
