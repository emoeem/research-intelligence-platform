# WSW-8 批次收口报告 · Browser Evidence / Integration（页面截图 + DOM Artifact + ResearchTask/Workflow/Evidence 关联，chromiumoxide vendor 基线跃迁批）

- 批次：WSW-8（[WSW-8-TASK.md](WSW-8-TASK.md) 全文照办：§0 D1-D8、§1 A1-A15、§2 范围、§3 S1-S9、§4 判据 13 条、§6 八红线）
- 日期：2026-11-23（实施会话；基线 HEAD `f8d38107` 工作树干净 + WSW-8-TASK.md 未跟踪在位）
- 收口态：**vendor 基线跃迁面/捕获引擎面/登记派发面/入口面/测试面/文档面全落 + workspace 全量零红（S9 终值见 §2）**；S9 未 commit（禁 git add/commit/push——工作树留主会话审查收口，REDLINES 红线 9）

---

## 1. 改动文件清单（路径 + 行数；wc -l / git diff --stat 实测）

| 路径 | 类型 | 行数 | 内容 |
|---|---|---|---|
| `crates/wsw-runtime/Cargo.toml` | 修改 | +16 | D1：chromiumoxide `=0.9.1` default-features=false + features=["bytes"]（fetcher/native-tls/rustls/zip0/zip8 全禁）+ 头注新基线声明两行（[[package]] 399→417 + reqwest 双检词形） |
| `Cargo.lock` | 修改 | +249/-4 | D1：基线跃迁入库——[[package]] 399→417（18 新块 = 16 新名 + tungstenite 0.28.0/sha1 0.10.7 双版本行；零删除零既有包版本 bump；wsw-runtime 依赖块 +1 行 chromiumoxide） |
| `vendor/`（18 新目录） | 新增 | 9.3MB | D1：`cargo vendor` 新包源码全量入库（chromiumoxide_cdp 6.5M 主导；.cargo/config.toml 零改动，`cargo build --offline` 全程可证；tracked 零 diff——18 `??` 待显式 add） |
| `crates/wsw-runtime/src/browser.rs` | 新建 | 858 | D2/D3/D4：统一入口（probe_chromium 五级探针链唯一实现 + scope_gate 子进程同链复用 + BrowserCaptureParams 词表校验 + open_engine_with_manifest Engine 装配 query-first + capture 统一入口 submit_correlated→NativeRustCall→终局 evidence 'screenshot' 直 INSERT + artifact_rows 回读）+ 单测 12（探针链矩阵/词表负例/clip 域/帽词形） |
| `crates/wsw-runtime/src/bin/rp-wsw-browser.rs` | 新建 | 648 | D2/D4：native_rust 派发目标子进程（argv `capture <out_dir>` + stdin D8.1 一行请求 + chromiumoxide 会话：BrowserConfig headless+no_sandbox+一次性 udir+XDG/TMP 压短注入 → 四模式截图 → DOMSnapshot captureSnapshot（computedStyles=["display"] 必填白名单）→ innerText → Network 域摘要 → out/ 五文件（帽 + capture_fidelity）→ meta 九字段 → D-TC-03 result 帧；Browser::close→wait→udir 清理） |
| `crates/wsw-runtime/src/lib.rs` | 修改 | +1 | `pub mod browser` 接线（WSW-5+6/WSW-7 追加声明先例） |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改 | +104/-1 | D8：`browser capture` 子命令（第十面分派 + run_browser_capture + usage 同步十面 + Args 五旗标 --selector/--clip-json/--settle-ms/--scope-rules-json 复用 --mode/--timeout-s/--task-id/--trace-id/--run-id） |
| `manifests/tools/rip.wsw.browser.json` | 新建 | 223 | D3：native_rust + binary=/home/emo/rt3-target/release/rp-wsw-browser + binary_sha256=a226fb56…（0.1.4 终钉）+ subcommand capture + argv_template ["{{out_dir}}"] + input 词表（url/mode 四态/selector/clip/settle_ms/scope_rules/timeout_s/browser_bin/trace_id）+ output（status ok + 九投影字段 + capture_fidelity）+ resources timeout 120s/8MiB/memory 128Gi（RLIMIT_AS 阶梯实测——坏消息 3）+ egress [80,443] + family browser_automation（词表硬门——坏消息 5）+ T1 + idempotent false + provenance（构建产物钉值 + RLIMIT_AS 实测 + 已知限制五条） |
| `manifests/tools/rip.wsw.browser.yaml` | 新建 | 122 | D3 JSON 孪生（digest 双文件一致实测） |
| `crates/tool-engine/tests/policy_resolver_manifests.rs` | 修改 | +17/-17 | D3：期望表 30→31（[29,0,0,1]→[30,0,0,1]、checked 29→30、快照 dump 31 份——设计内扩展非钉值改动，照 WSW-7 先例只改预期值） |
| `crates/context-broker/tests/catalog_sources.rs` | 修改 | +16/-16 | 登记面计数 35→36（5 断言 + coverage 注记 +browser——**同类计数面随更落点例外**，WSW-7 catalog_sources 先例；零红硬门强制） |
| `crates/context-broker/tests/acceptance_no_stuffing.rs` | 修改 | +2/-2 | 登记面计数 35→36（no-stuffing 不变式 36 工具面实测仍成立） |
| `apps/server/rp_server.py` | 修改 | +96 | D8：POST /api/wsw/browser/capture（`_wsw_browser_capture`：mode/scheme/selector/clip 词表 400 快拒 + rp-wsw browser capture 子进程委派 :4766/:4998 先例同型 + timeout_api_s 5..600 帽 + 504 rp_wsw_timeout/503 rp_wsw_bin_missing 词形透出 + 终态非 SUCCEEDED 结构化 capture_error 回传）+ `_wsw_post_route_wsw56` browser/capture 分支；SSE 面与既有 /api/wsw/* 全族零触碰 |
| `apps/server/selftest_wsw8.sh` | 新建 | 215 | S8：V-9 五项守卫 + Chromium 探针链分面 + 真实 capture 全链对账（PNG magic/sha256/dom strings/六 ID/015 视图）+ file:// 与 scope 元数据负例 + reqwest 双检 + lock 基线钉 417 + vendor 完整性 + deny 双检 + 生产库指纹 + 跨轨零 diff 反证；**PASS/SKIP 分开计数 exit 0** |
| `crates/wsw-runtime/tests/browser_adapter.rs` | 新建 | 709 | S7：e2e 8 测（四模式真捕获 + 六 ID/015 视图对账 + meta 九字段 + 截断帽 fidelity 负例 + TIMED_OUT 看门狗负例 + scope 元数据拒负例 + selector_miss 结构化失败负例 + 词表零派发负例 + V-9 探针 fallthrough/SKIP 词形 + udir 残留=0） |
| `docs/README.md` | 修改 | +1 | 本报告索引登记（§9） |
| `docs/rip3/wsw/WSW-8-BATCH-REPORT.md` | 新建 | 本档 | 批次报告 |

未触碰实证（S9 反证 grep = 0）：web/**、sender.rs、scope.rs、listener/**、persist.rs、wsframe.rs、decoder.rs、comparer.rs、passive.rs、fuzz/**、scanner.rs、authority.rs、tool-engine/src、mcp-server/src、tool-registry/src、plugin-registry/、plugin-runtime/、verification-engine/、agent-engine/、wsw-message/**、migrations/、integrations/rtis/**、deny.toml、.cargo/config.toml——selftest_wsw8.sh §5 实测回显 = 0（§2 ⑧）。

## 2. §5 收口命令实测（exit code + 关键输出行）

```bash
# ① S1 基线（f8d38107 干净树）：grep -cE '^\[\[package\]\]' Cargo.lock = 399；grep -c 'name = "reqwest"' = 0
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# → EXIT=0；1315 passed / 0 failed（基线零红口径延续）；/usr/sbin/chromium --version = Chromium 158.0.8127.36 Arch Linux（探针链一级固定路径命中）

# ② S2 基线跃迁（--config 覆盖 vendored 源走 rsproxy-sparse，仓库 .cargo/config.toml 零改动）
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' update -w    # → Adding 18 块；EXIT=0
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' vendor vendor --locked    # → EXIT=0；18 新目录
grep -cE '^\[\[package\]\]' Cargo.lock        # → 417（预判精确命中；偏差 = async-tungstenite 0.32.1 vs A3 预判 0.32.0——坏消息 2）
grep -c 'name = "reqwest"' Cargo.lock         # → 1
grep -rn '^reqwest' crates/*/Cargo.toml | wc -l    # → 0
cargo deny check licenses && cargo deny check advisories    # → licenses ok / advisories ok（双 EXIT=0）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --offline -p wsw-runtime    # → Finished dev 44.10s EXIT=0
git diff HEAD -- Cargo.lock | grep -c "^-name\|^-version"    # → 0（零删除行零 bump）
du -sb vendor/<18 新目录> 合计                   # → 9,767,832 B（9.3MB；A3 预判 11MB——du 口径差，坏消息 2）
ls vendor | wc -l                              # → 392（374+18）

# ③ S3/S5 定向测试
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime -- --test-threads=1
# → EXIT=0；220 passed / 0 failed（lib 100 含 browser.rs 单测 12 + integration 120 含 browser_adapter 8）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p tool-engine -- --test-threads=1    # → 全绿（期望表 31 断言过）

# ④ S4 登记面
CARGO_TARGET_DIR=/home/emo/rt3-target cargo run --offline -p tool-registry --bin rp-tool -- validate --manifest manifests/tools/rip.wsw.browser.json
# → {"command":"validate","content_hash":"5d3d6fb2…","ok":true,"tool_id":"rip.wsw.browser","version":"0.1.4"} EXIT=0
CARGO_TARGET_DIR=/home/emo/rt3-target cargo run --offline -p tool-registry --bin rp-tool -- register --manifest manifests/tools/rip.wsw.browser.json --db migrations/dev-b1.db
# → {"command":"register",…,"ok":true,"superseded_previous":true,"version":"0.1.4"} EXIT=0
# 重跑（同版本）→ exit 1 {"error":{"code":"registration_failed","message":"…rule 2 (VersionComparable)…not greater than existing…"}}（幂等拒绝词形）
grep -rn "rip.wsw.browser" manifests/tools/ | wc -l    # → 2（json+yaml 孪生）
sha256sum /home/emo/rt3-target/release/rp-wsw-browser    # → a226fb56c8061eb16bf739f3213705fd8e2b32d4977c11b7ee97e8274f0a564e（= manifest binary_sha256，PIN_MATCH）
# Resolver assign 快照（RT3_SNAPSHOT_DUMP=1）：
# rip.wsw.browser declared=native_rust tiers=[low=assign/native_rust/- unknown=inconsistent/podman_host/- high=reject/null/RT3_VM_NOT_READY]
#   verdict=assign cross_check.consistent=true R6=绝对路径+64hex 钉值+允许面+预算+egress 面 通过

# ⑤ 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p wsw-runtime --all-targets -- -D warnings    # → EXIT=0
rustfmt --edition 2021 --check crates/wsw-runtime/src/browser.rs crates/wsw-runtime/src/bin/rp-wsw-browser.rs crates/wsw-runtime/tests/browser_adapter.rs    # → 本批三新文件 FMT_CLEAN（rp-wsw.rs 存量红按 WSW-5+6 §E.5 口径保留）

# ⑥ S7 e2e
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime --test browser_adapter -- --test-threads=1
# → 8 passed / 0 failed（156s；四模式真捕获 + 六 ID 对账 + 负例矩阵 + V-9 PASS 词形——本机探针链命中无 SKIP 计数）

# ⑦ S8 收口
RP_BROWSER_BIN=${RP_BROWSER_BIN:-} bash apps/server/selftest_wsw8.sh
# → PASS=28 SKIP=0 FAIL=0 exit 0（capture 实拍对账：run=rn-wsw8-… png=12052B strings=26 net_rows=4 六ID 同链）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw56.sh    # → PASS=42 SKIP=0 FAIL=0 exit 0（零改动回归绿）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh     # → PASS=30 SKIP=4 FAIL=4 exit 1（4 FAIL 全部 = D1 声明的基线跃迁面——坏消息 4；4 SKIP = 镜像 digest 漂移实录——坏消息 6）

# ⑧ 跨轨零 diff 反证（S9）
git status --porcelain | grep -c "web/\|sender.rs\|scope.rs\|listener\|persist\|fuzz/\|tool-engine/src\|mcp-server/src\|migrations/\|integrations/rtis\|deny.toml\|.cargo/config"    # → 0
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done
# → 1 / 1 / 1 / 0 / 0（只读指纹；selftest §5 同值复测零触碰）

# ⑨ S9 全量（新 lock 基线 417 全量门；首跑实录 1334 passed / 3 failed = context-broker 计数面两处随更前态 + browser_adapter 并行 udir 竞态——三处修复后终跑）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --workspace --no-fail-fast   # （默认并行线程——串行锁修复后零旗标绿）
# → EXIT=0；185 个 test binary；1337 passed / 0 failed（1315 基线 + browser.rs lib 单测 14 + browser_adapter e2e 8 = +22；/tmp/wsw8_s9_final.log）
```

## 3. 判据逐条状态（§4 判据表 13 条）

| # | 判据 | 状态 | 实测锚 |
|---|---|---|---|
| 1 | 真实截图对账（禁模拟冒充） | **PASS** | selftest §4：tool_run SUCCEEDED + screenshot.png artifact 行 sha256/size 与 `sha256sum`/stat 实测一致 + 文件头 `89504e47`（xxd 实拍 `8950 4e47 0d0a 1a0a`）+ meta.json 九字段逐词在场（current_url/final_url/title/captured_at/viewport/dpr/browser_version/load_status/trace_id） |
| 2 | DOM 结构化提取对账 | **PASS** | dom.json = DOMSnapshot.captureSnapshot 原始 JSON（strings/documents 两键在场，strings=26、documents=1）+ strings 含 fixture marker（`wsw8-selftest-marker-4e7c`）+ page.txt 与 fixture 正文一致 + network.json 摘要行数与 fixture 请求数对账（4 行：html/pixel.png/app.js/favicon，url/method/status/resource_type/size 词面齐） |
| 3 | evidence 链六 ID 对账 | **PASS** | evidence 行 evidence_type='screenshot'（001:252 词表预埋面）+ content_hash = artifact.sha256（`hash_same=1`）+ storage_ref = artifact uri（`ref_same=1`）+ source_pk = run_id + task_ref = task_id + trace_id 三列同值（tool_run/artifact/evidence = tr-wsw8-cli-3）+ v_trace_chain 视图按 trace_id 汇出 tool_run/evidence/artifact≥5 行实测 |
| 4 | run 归属反证 | **PASS** | 派发唯一路径 = capture()（browser.rs）Engine submit_correlated + NativeRustCall；browser.rs 源面 grep 零 `Command::new("chromium")`（probe_chromium 只做 fs metadata 检查）+ 零 wsw_fuzz_job 词形 + evidence 写面唯一（insert_screenshot_evidence 单函数 INSERT 001 表） |
| 5 | 截图四模式 | **PASS** | viewport/full_page/element/region 四次真捕获全 SUCCEEDED（e2e 四模式循环 + CLI region `--clip-json` 实跑 SUCCEEDED + evidence ev-rn-wsw8-1795374702223130）+ 非法 mode 词表拒绝负例（validation 词表外）+ selector 无命中结构化失败负例（FAILED + error_json 含 `selector_not_found`） |
| 6 | 预算与隔离 | **PASS** | /stall + timeout_s=5 → 看门狗 TIMED_OUT（tool_run 状态 + e2e 断言）+ 一次性 user-data-dir 终局清理实测（udir_count 前后相等 = 0 残留；close→wait→remove 修复实录见坏消息 7）+ 超帽截断 → capture_fidelity 词形负例（/big 页 ~2.5MiB → page.txt 落帽 1MiB + meta.capture_fidelity[{artifact:"page.txt",reason:"cap_exceeded",cap_bytes:1048576,…}]） |
| 7 | scope 与边界负例 | **PASS** | file:// scheme 词表拒绝（validation + selftest CLI 负例）+ 元数据 host `169.254.169.254` scope_rules 传入时 check(:296) 拒绝（scope_blocked + metadata_endpoint 词形，零 run 行——闸在 submit 前）+ scope.rs 文件零 diff（`git status --porcelain | grep scope.rs` = 0） |
| 8 | 登记面（Resolver assign 硬门） | **PASS** | 期望表 31 份 [30,0,0,1] 断言绿（5 passed）+ rp-tool validate exit 0 + register dev 副本 exit 0（重跑 rule 2 fail-closed 拒绝词形如实登记——无 skipped 词形，同 WSW-7 坏消息 3 口径）+ /api/tools 露出（tool 表注册行，handle_tools aux/dev 副本查询面）+ binary_sha256 manifest = release 实测（PIN_MATCH） |
| 9 | V-9 守卫 | **PASS** | 探针链五级任一命中 PASS（selftest：命中 /usr/sbin/chromium）+ 全缺 → chromium_missing SKIP exit 0（脚本 GUARD_MISS/CHROME_OK 分面形状 + e2e chromium_skip_guard 词形）+ RP_BROWSER_BIN 指向不存在路径负例实测（e2e browser_v9_probe_chain_fallthrough：链继续落到固定路径级）+ bin/副本缺 SKIP 计数（selftest_wsw7.sh :23-38 形状复用） |
| 10 | 独立入口面 | **PASS** | `rp-wsw browser --help` exit 0（main 全 argv --help 短路 → usage stdout）+ POST /api/wsw/browser/capture 400 校验负例矩阵（bad_url_scheme/bad_mode/bad_selector/bad_clip 直调实测全中）+ 503 rp_wsw_bin_missing + 504 rp_wsw_timeout（直调实测）+ 401 Bearer 同面（do_POST 顶部 require_auth 既有承载——:6028 词形零触碰）+ 既有 /api/wsw/* 端点零触碰反证（rp_server diff 仅 +96 追加面） |
| 11 | vendor 基线跃迁与零外溢 | **PASS** | [[package]] = 417（S2 精确命中）+ Cargo.toml 直接依赖 reqwest = 0 + lock 内 reqwest = 1 行（chromiumoxide 0.13.5 传递，default-features=false 零 TLS——lock 依赖面实读：base64/bytes/http/hyper/web-sys 面零 native-tls/rustls/openssl）+ deny 双检 exit 0 + `cargo build --offline -p wsw-runtime` 过 + vendor 完整性门（复跑 tracked diff = 0）+ web/**/migrations/tool-engine src/mcp-server src/integrations/rtis/deny.toml/.cargo 零 diff |
| 12 | 全量回归 | **PASS** | workspace 全量零红——EXIT=0，185 binary，1337 passed / 0 failed（1315 基线 +22 = browser.rs lib 单测 14 + e2e 8）；clippy -D warnings EXIT=0（本批 crate）+ selftest_wsw56.sh 零改动回归绿（42/0/0）+ selftest_wsw7.sh 零改动回归（30 PASS + 4 SKIP + 4 FAIL 全部 = D1 声明面——坏消息 4/6 单列）；首跑 3 失败三根因与修复实录见坏消息 9 |
| 13 | WSW-9/RELEASE 前置输出 | **PASS** | 批次报告 §7 专节四项全登记（capture 冷/热双口径基线 + Chromium 生命周期/并发/超时恢复/隔离评估 + 跨平台探针词表 + vendor 新基线台账 + reqwest 双检门词形 + WSW-7 §6 承接态） |

## 4. vendor 跃迁实测台账

### 4.1 lock 对账（`git diff HEAD -- Cargo.lock` 逐行核对）

- `[[package]]`：**399 → 417**（+18 块，A3 预判精确命中）。
- +18 新块：chromiumoxide 0.9.1 / chromiumoxide_cdp 0.9.1 / chromiumoxide_pdl 0.9.1 / chromiumoxide_types 0.9.1 / async-tungstenite **0.32.1** / reqwest 0.13.5 / sync_wrapper 1.0.2 / tower 0.5.3 / tower-http 0.6.11 / tower-layer 0.3.3 / which 8.0.6 / utf-8 0.7.6 / ipnet 2.12.2 / futures-timer 3.0.4 / wasm-bindgen-futures 0.4.80 / windows-registry 0.6.1（16 新名）+ tungstenite 0.28.0（与 hudsucker 栈 0.30.0 并存）+ sha1 0.10.7（与既有 0.11.0 并存）。
- **零删除行、零既有包版本 bump**（A3 预判 9 个 bump 未发生——既有钉值原样吸收，比预判更干净；唯一偏差 = async-tungstenite 0.32.1 vs probe 预判 0.32.0——坏消息 2）。
- wsw-runtime 依赖块：+1 行 `"chromiumoxide"`。

### 4.2 16 新包许可台账（deny 双检 exit 0，deny.toml 零改动）

| 包 | 版本 | license | 用途 |
|---|---|---|---|
| chromiumoxide | 0.9.1 | MIT OR Apache-2.0 | Chromium 控制引擎（D3 派发目标子进程内 CDP 会话） |
| chromiumoxide_cdp | 0.9.1 | MIT OR Apache-2.0 | CDP 协议绑定（build 期 pdl 生成，6.5MB 主导 vendor 增量） |
| chromiumoxide_pdl | 0.9.1 | MIT OR Apache-2.0 | CDP PDL 解析器（cdp build 依赖） |
| chromiumoxide_types | 0.9.1 | MIT OR Apache-2.0 | CDP 消息类型面 |
| async-tungstenite | 0.32.1 | MIT | CDP WebSocket 连接（tokio-runtime feature 链） |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | **chromiumoxide 硬传递（default-features=false 零 TLS 零 openssl 零 rustls——lock 依赖面实读：base64/bytes/http/hyper-util/web-sys 面）** |
| sync_wrapper | 1.0.2 | Apache-2.0 | reqwest 传递 |
| tower | 0.5.3 | MIT | reqwest 传递（Service 面） |
| tower-http | 0.6.11 | MIT | reqwest 传递 |
| tower-layer | 0.3.3 | MIT | tower 传递 |
| which | 8.0.6 | MIT | chromiumoxide 浏览器探测（fetcher 兄弟面——本批禁用其下载面，仅 which 系统查找进树） |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 | async-tungstenite 传递（WebSocket UTF-8 校验） |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | reqwest 传递 |
| futures-timer | 3.0.4 | MIT/Apache-2.0（斜杠形 deny 归一 OR） | chromiumoxide 超时面 |
| wasm-bindgen-futures | 0.4.80 | MIT OR Apache-2.0 | reqwest wasm 备选面传递（cfg(wasm) 未编译路径） |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 | chromiumoxide Windows 探测面传递（cfg(windows) 未编译路径） |

双版本目录：`vendor/sha1-0.10.7/`（MIT OR Apache-2.0）、`vendor/tungstenite-0.28.0/`（MIT OR Apache-2.0）。
vendor 增量实测 9.3MB（du -sb 合计 9,767,832 B）；`ls vendor | wc -l` = 392（374+18）；`cargo vendor vendor --locked` 复跑 tracked diff = 0。

### 4.3 reqwest 双检词形（D1⑤ 基线声明）

```
grep -rn '^reqwest' crates/*/Cargo.toml | wc -l     # → 0（workspace 直接依赖 = 0 硬门）
grep -c 'name = "reqwest"' Cargo.lock               # → 1（chromiumoxide 传递行——基线钉）
```

后续批门禁写法：`[[package]]` 以 HEAD 对比不硬编码陈旧数（`git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'`——WSW-3-TASK :124 词形）；reqwest 双检两行命令原样入 selftest_wsw8.sh §1。

## 5. 真实截图实测数字（判据 1-3 载体）

- S3 冒烟首拍（RP_BROWSER_BIN=/usr/sbin/chromium 直跑）：viewport elapsed_ms=837、screenshot_bytes=14292、dom_strings=26、network_rows=4；PNG magic `89504e47`；meta 九字段齐（browser_version = HeadlessChrome/158.0.0.0——宿主 158.0.8127.36）。
- CLI 全链（rp-wsw browser capture 经 Engine submit_correlated + native_rust 派发）：viewport SUCCEEDED + evidence ev-…79；region `--clip-json` SUCCEEDED + evidence ev-…30。
- selftest_wsw8 实拍对账行：`capture 实拍对账：run=rn-wsw8-1795374924999325 png=12052B strings=26 net_rows=4 六ID 同链`。
- e2e 四模式：8 passed / 156s（每模式一次独立 Chromium 进程 + 一次性 udir；udir 残留 = 0）。
- capture 冷/热双口径（D7① 基线数字，release 二进制直跑三连测，loopback fixture viewport+settle 300ms）：冷首跑 wall=1372ms/elapsed_ms=1359，热跑 wall=1188/1173ms、1218/1199ms（详细口径见 §7.1）。

## 6. 坏消息单列（十项）

1. **UI 批词形移交去向（D8① 如实登记，不冒充兑现）**：Scanner 工作台 UI（providers 投影卡片 + 扫描发起表单）与 Evidence 页截图查看 UI（PLAN :329 四页面）双双移交后续 UI 批观察池；web/** 全目录零 diff 实证（S9 反证 grep = 0）。后端零改动即接：providers 投影既有 GET /api/wsw/scan/providers + 本批 POST /api/wsw/browser/capture + evidence 行 015 视图汇出面。
2. **lock 对账偏差**：async-tungstenite 实解析 0.32.1（A3 probe 预判 0.32.0——probe 时点与 S2 实解析相隔索引更新）；A3 预判的 9 个既有包版本 bump 全部未发生（lock diff 零 `-name` 行，比预判干净）；vendor 增量实测 9.3MB vs 预判 11MB（du 块大小口径差）。[[package]] 417 预判精确命中。
3. **RLIMIT_AS 与 Chromium 结构性不兼容（本批新实测发现）**：Manifest memory 初值 512Mi 下 prlimit `--as=512Mi` FATAL（V8 虚拟地址空间预留——`Socket path too long` 之外的第二拦路面：`prlimit --as` 64Gi 仍 FATAL、96Gi OK）；阶梯实测（2026-11-23，prlimit --as：FAIL@512Mi/2Gi/8Gi/16Gi/64Gi、OK@96Gi/128Gi/256Gi）→ 1.3× 余量钉 **128Gi**（DI-2 ADR D5 #5 ffprobe 1536Mi 阶梯先例同款）；RLIMIT_AS 为 VA 上限非物理内存帽，实质资源承载 = RLIMIT_CPU + 墙钟看门狗（timeout_seconds → TIMED_OUT killpg）。systemd-run MemoryMax 路径不受影响（MemoryMax=512Mi 实测 OK——env_clear 白名单下 systemd-run --user 探测恒败落 prlimit 兜底，native_rust.rs:964 实录词形）。
4. **selftest_wsw7.sh 零改动回归 = 4 FAIL（全部为 D1 声明的基线跃迁面）**：`Cargo.lock diff = wsw-runtime 依赖块 3 行`（现 = 228 行 18 新块 + 1 依赖行）、`[[package]] 399 恒定`（现 417）、`跨轨零 diff vendor/`（现 18 新目录）、`Cargo.lock diff 无其它行`——四钉均为 D1 明文声明的新基线词形覆盖（selftest_wsw4.sh 28 陈旧钉已知红同款词形：**测试钉值零改动维持，禁顺手改**——收口后 417 为新基线，wsw7 四钉随下一链尾演进批随更，观察池登记）；wsw7 其余 30 项 PASS 全绿（登记/投影/CLI 负例/RTIS 委派/rp_server 面）。
5. **family 词表修正**：任务书 §2.1 词形「family `browser`」在 rp-tool validate 13-family 词表（validate.rs:26 `browser_automation`）外——validate 硬门实测拒绝，登记用 `browser_automation`（词表内唯一 browser 族）。偏差如实登记，非钉值改动。
6. **镜像 digest 漂移实录（WSW-7 坏消息 5 词形再实证）**：selftest_wsw7 收口时点 localhost/rip-nuclei 镜像经外部重建（podman images 显示 3h 前 latest 6692d845d13d），钉值 digest 9ff41ce9… 不再命中 → wsw7 判据 1/2 e2e 分面 4 SKIP（D6 分面词形）。非本批引入（vendor/lock 面与本批零交集）；恢复路径 = 镜像重建后 `podman images --digests` 实测回钉 manifests/tools/rip.security.nuclei.json / rip.security.zap.json（与本批 binary_sha256 回钉同款维护面）。
7. **Chromium 子进程环境两处实测修复（S3 冒烟后坐实）**：① Adapter env_clear 白名单 TMPDIR = 任务深层 tmp → Chromium process_singleton SingletonSocket 超 Unix socket 108 字符帽 FATAL（`Socket path too long`，e2e 实录）——修复 = 一次性 user-data-dir 锚系统 /tmp（非 std::env::temp_dir()——TMPDIR env 感染面）+ Chromium 子进程 env 注入 XDG_RUNTIME_DIR/TMPDIR 压短（BrowserConfig process_envs 既有面）；② close→wait（Browser::wait 既有面）后清理 user-data-dir，消除 chromium 收尾写盘与 remove 竞态（udir 残留 2 → 0）。两修复均在 rp-wsw-browser 子进程内承载，tool-engine/adapter 零触碰。
8. **binary_sha256 维护面（任务书必列项；WSW-7 坏消息 5 同款词形）**：本批实施期 release 二进制随源码修复迭代重建四次（80da8bb5→1c526d5b→034ec92c→**a226fb56 终钉**），manifest version 0.1.0→0.1.1→0.1.2→0.1.3→0.1.4 演进（rule 2 fail-closed 强制——同版本重登记被拒）；终态 = 0.1.4 + a226fb56 双文件一致（PIN_MATCH 实测）+ dev-b1.db 注册行 0.1.4 superseded_previous=true。重建后 sha256sum 实测回钉 manifest json/yaml 两处一致 + rp-tool register 一次。
9. **workspace 首跑 3 失败实录与修复（判据 12 门过程面）**：①context-broker catalog_sources.rs / acceptance_no_stuffing.rs 计数 35 vs 36（随更前态）——计数面随更后单跑复绿；②browser_capture_four_modes udir 残留断言 `10 != 8`——workspace 默认并行线程下多 browser 测试并发互扰（e2e 文件默认并行，selftest/定向纪律 --test-threads=1 不覆盖 workspace 门）——修复 = 测试内全局串行锁（BROWSER_TEST_LOCK 毒化免疫；`--test-threads=4` 并行复跑 8/8 绿坐实）。修复后 workspace 终跑零旗标全绿。
10. **TIMED_OUT 路径 udir 残留（判据 6 已知边界）**：killpg 看门狗击杀覆盖 rp-wsw-browser 子进程自身——TIMED_OUT 终态 run 的一次性 user-data-dir 残留 /tmp（正常完成路径残留 = 0 已实测，e2e 断言 + selftest 面）；固定前缀 `rp-wsw-browser-udir-` 可被宿主 janitor 面（WSW-9 生命周期评估输入）定期清扫，观察池登记不在本批实现。
   - **capture 截断帽实测值（任务书必列项）**：dom.json 16MiB / page.txt 1MiB / network.json 1MiB（超帽字节级截断 + capture_fidelity 登记——e2e /big 页 ~2.5MiB 正文实测 page.txt 落帽 1MiB 且 fidelity 登记 `{"artifact":"page.txt","reason":"cap_exceeded","cap_bytes":1048576,"raw_bytes":2631616,"written_bytes":1048576}` 词形）；screenshot 不设帽（PNG 自身体积面，8MiB 为结果帧帽）。
   - **catalog 计数面随更落点例外**：context-broker catalog_sources.rs + acceptance_no_stuffing.rs 计数 35→36（任务书 §2.1 落点清单未列——workspace 全量零红硬门强制随更，WSW-7 catalog_sources 34→35 先例同款，如实登记）。

## 7. WSW-9 + RELEASE 前置输出登记（交付物 7 专节）

### 7.1 capture 冷/热双口径时长基线（PLAN :754 实测禁预填）

| 口径 | 实测（2026-11-23，release rp-wsw-browser 直跑，loopback fixture viewport settle=300ms） |
|---|---|
| 冷（进程组首跑，浏览器进程启动 + 会话 + 捕获全链） | wall 1372ms（elapsed_ms=1359） |
| 热（连续第二/三跑） | wall 1188ms / 1218ms（elapsed_ms 1173 / 1199） |

结论：浏览器进程启动 ~0.7-0.9s 主导冷跑，热跑稳态 ~1.2s；capture 本体（导航+截图+DOMSnapshot+网络摘要）在 837ms 冒烟面已含。**基线移交 WSW-9 性能/可靠性验收批**（§九 :611），本批零门（D7①）。无并发池/复用实现——并发面 = 每 run 独立进程天然隔离，WSW-9 并发验收按此基线起测。

### 7.2 Chromium 生命周期/并发/超时恢复/浏览器隔离评估结论（PLAN :331 逐词）

- **生命周期**：每 capture run 一次 `Browser::launch` → CDP 会话 → `Browser::close` → `Browser::wait`（子进程完全退出）→ udir 删除；进程组 = Adapter process_group(0)，killpg TERM→KILL 覆盖 Chromium 及孙进程（看门狗 TIMED_OUT 实测路径）。
- **并发**：本批零池化（观察池）；每 run 独立浏览器进程 + 独立 udir 天然无共享态，并发上限 = 宿主进程/内存预算（WSW-9 承接）。
- **超时恢复**：双层——Adapter 看门狗预算点 killpg（TIMED_OUT 语义承载）+ 子进程内部死线 = 预算 + 30s 兜底自清理（capture_timeout 词形，仅看门狗失效时可达）；CDP 层 REQUEST_TIMEOUT 30s 单调用帽（chromiumoxide 既有）。
- **浏览器隔离**：cookie/storage = 一次性 user-data-dir（run 终局删除，e2e 残留=0 实测）；进程 = 独立进程组 + no_sandbox（容器/无显示环境必需）；复用浏览器实例面零实现（观察池）。

### 7.3 Chromium 探针链跨平台路径词表（WSW-9 跨平台矩阵输入）

| 平台 | 本批实现（browser.rs probe_chromium 唯一实现） | chromiumoxide detection.rs 既有词形（对照面，:91-150） |
|---|---|---|
| Linux（本机实测） | RP_BROWSER_BIN env → PW_CHROMIUM env → ~/.cache/ms-playwright/chromium-*/chrome-linux*/chromium glob → /usr/sbin/chromium → /usr/bin/chromium | /opt/chromium.org/chromium、/opt/google/chrome、/tmp/aws/lib（lambda）+ PATH 名称面 |
| Windows（登记未测） | 同链（env 两级 → ms-playwright glob 路径分隔面 → 无固定路径级——**词表待 WSW-9 补 Windows 固定路径级**） | CHROME env 变量 + PATH（chrome.exe/msedge.exe）+ 注册表（LOCAL_MACHINE/CURRENT_USER——windows-registry crate）+ C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe |
| macOS（登记未测） | 同链（glob ~/.cache/ms-playwright + /usr/sbin/chromium 无效面） | /Applications/Google Chrome.app/Contents/MacOS/Google Chrome（+Beta/Dev/Canary）、/Applications/Chromium.app/Contents/MacOS/Chromium、/Applications/Microsoft Edge.app/… |

### 7.4 vendor 新基线台账 + reqwest 双检门词形

见 §4.2（16 新包名/版本/许可/用途一张表）与 §4.3（双检命令原样）。后续批门禁：lock 对比以 HEAD 为准（`git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'`），reqwest 双检两行原样，deny 双检 exit 0。

### 7.5 WSW-7 §6 前置输出承接态

- **providers 响应词形消费去向**：WSW-7 §6.1 三 provider 投影词形（provider_id/runtime/family/permission_level/idempotent/version/image）本批零改动消费——`rp-wsw scan providers` 与 GET /api/wsw/scan/providers 全族零触碰（S9 反证 = 0）。
- **Scanner UI 缺口清单移交**：WSW-7 §6.2 清单 + 本批 Evidence 页截图查看 UI 双双并入后续 UI 批观察池（§6 坏消息 1 词形去向）；Scanner 发起表单可按 POST /api/wsw/browser/capture（400 词表负例词形齐）与 POST /api/wsw/scan 两端点直接接线，后端零改动。
- **GUIDE scan/plugin 零命中实测承接**：本批零新 GUIDE 词面（browser capture 不入 GUIDE——UI 批词形），零命中态延续。

## 8. 交付物对照（§8 八项）

1. vendor 基线跃迁面 ✅（Cargo.toml 声明 + 头注 + lock 417 对账台账 + vendor 18 目录 9.3MB + deny 双检 + offline 构建 + reqwest 双检声明；`git diff HEAD --stat -- Cargo.lock vendor/` = +249 行 + 18 目录）。
2. 捕获引擎面 ✅（rp-wsw-browser bin 648 行 + browser.rs 858 行）。
3. 登记派发面 ✅（manifest 孪生 0.1.4 + 期望表 31 份 + validate/register 实拍 + catalog 计数面 36）。
4. 入口面 ✅（rp-wsw browser capture 第十面 + POST /api/wsw/browser/capture；SSE 面与既有 /api/wsw/* 全族零触碰）。
5. 测试面 ✅（tests/browser_adapter.rs 8 测 + selftest_wsw8.sh 28 PASS + 判据 1-13 实测锚 §3）。
6. 文档面 ✅（本档 + docs/README.md 登记；坏消息十项含任务书必列四项）。
7. WSW-9 + RELEASE 前置输出 ✅（§7 专节四项）。
8. 下一批输入（观察池登记）✅（§6 坏消息 1 + §2.2 观察池词形随任务书；scanner UI/evidence UI/MHTML/实例复用池/计划级批量/导出渲染/BodyArtifactRegistrar 桥接/Listener 集成增量/wsw4 28 陈旧钉 + wsw7 四基线钉随更）。
