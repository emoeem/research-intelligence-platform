# WSW-7 批次收口报告 · Plugin / Scanner Integration（Tool/Plugin Registry 接入 + Nuclei/ZAP/RTIS 三 Adapter）

- 批次：WSW-7（[WSW-7-TASK.md](WSW-7-TASK.md) 全文照办：§0 D1-D8、§1 A1-A14、§2 范围、§3 S1-S9、§4 判据 12 条、§6 八红线）
- 日期：2026-11-23（实施会话；基线 HEAD `d62e8b87` 工作树干净）
- 收口态：**登记面/代码面/测试面全落 + workspace 全量零红（1315 passed）**；ZAP 镜像面按 **D1 选项 B** 收口（构建期外联传输坐死实录——坏消息单列）；S9 未 commit（禁 git add/commit/push——工作树留主会话审查收口）

---

## 1. 改动文件清单（路径 + 行数；git diff --numstat / wc -l 实测）

| 路径 | 类型 | 行数 | 内容 |
|---|---|---|---|
| `manifests/tools/rip.security.zap.json` | 新建 | 157 | D2：podman_host + image digest 占位 64 零（S3 选项 B——实拉后回钉）+ input（url/target/scan_mode enum[baseline]/timeout_seconds/alert_budget）+ output（matched_count/alerts_excerpt/zap_version）+ resources timeout 600 + 8MiB + egress ports [80,443,8080,8443] + T1 + **idempotent false** + family network + provenance（ghcr 源迁移注记 + known_limits） |
| `manifests/tools/rip.security.zap.yaml` | 新建 | 115 | D2 JSON 孪生（nuclei :19 孪生先例形状） |
| `containerfiles/rip-zap/Containerfile` | 新建 | 41 | D1②：FROM ghcr.io/zaproxy/zaproxy@sha256（digest 占位——选项 B 状态头注实录：源迁移 + 传输坐死 + 恢复路径）+ HOME/XDG 收敛 /task/out + ENTRYPOINT wrapper |
| `containerfiles/rip-zap/wrapper.sh` | 新建 | 87 | D1②：D-TC-03（zap.sh 内联 -cmd -quickurl/-quickout + set -- 数组拼装零注入面 + proxy env → connection.proxy.chain.* 显式旗标 + 镜像内 python3 解析 JSON 报告 → stdout 第一行 result 帧 + zap.log 静默面） |
| `crates/verification-engine/src/provider.rs` | 修改 | +229/-0 | D1②/D3：ZAP consts（ZAP_IMAGE/ZAP_ENTRYPOINT/ZAP_FAMILY/ZAP_TOOL_ID/ZAP_EGRESS_PORTS，:195-201 同型）+ ZapProvider（capability + build_input clamp 双向）+ execute_zap（execute_nuclei :241 同型：submit_correlated → ContainerCall → execute_container_via_engine → 信封映射 → ProviderOutcome validate）+ 单测 2（capability 契约 + input 形状/manifest 同源断言）；**execute_nuclei/RTIS 面零改动**（模块头注 Provider 面三实现补记） |
| `crates/wsw-runtime/Cargo.toml` | 修改 | +13 | D4：verification-engine path 依赖 + tool-engine/tool-registry workspace 依赖（三依赖全 workspace 内部 path——[[package]] 399 恒定；D4 单行词形的同族扩展注记见 §坏消息） |
| `crates/wsw-runtime/src/scanner.rs` | 新建 | 544 | D1/D3/D7/D8：三 provider 统一入口（scan_nuclei=execute_nuclei 消费/PlanItemRecord 合成、scan_zap=execute_zap/ZapScanParams、scan_rtis=RtisPocProvider poc get+use 委派 + **id 元数据锚投影正文零透出**）+ ScanError 词值面 + 五态投影（evaluator::evaluate 复用零第二判定）+ providers_projection（manifest 直读 + plugin catalog 只读计数）+ 单测 4（stub rt.py 矩阵/五态 INCONCLUSIVE/投影对账/预算形状） |
| `crates/wsw-runtime/src/lib.rs` | 修改 | +1 | `pub mod scanner` 接线（WSW-5+6 追加声明先例） |
| `crates/wsw-runtime/src/bin/rp-wsw.rs` | 修改 | +291/-4 | D7①：`scan` 子命令分派（nuclei/zap/rtis/providers 四面）+ run_scan_* 三 runner + scan_engine（文件库 query-first 幂等登记——rp-verify engine_with_nuclei 先例）+ `scan providers` 投影载体 + usage 同步 + P9-3 --help/-h → stdout exit 0 统一口径（判据 9） |
| `crates/tool-engine/tests/policy_resolver_manifests.rs` | 修改 | +18/-17 | D2：期望表 29→30（`[28,0,0,1]→[29,0,0,1]`、fn 更名 `manifests_expectation_table_30`、快照 dump 30 份、constraints checked 29——**tasks 文件计数面随更，照 DI-3/RT-4 先例只改预期值**） |
| `crates/context-broker/tests/catalog_sources.rs` | 修改 | +8/-8 | 登记面计数 34→35（7 处断言 + coverage 注记 +zap——**同类计数面随更，零红硬门强制**；WSW-5+6 三工具入面同款先例） |
| `crates/context-broker/tests/acceptance_no_stuffing.rs` | 修改 | +2/-2 | 登记面计数 34→35（no-stuffing 不变式不变——used<=alloc 在 35 工具面实测仍成立） |
| `apps/server/rp_server.py` | 修改 | +76 | D7②：POST /api/wsw/scan（`_wsw_scan`：provider 词表 400 闸 + rp-wsw scan 子进程委派 :4766 先例同型 + timeout_api_s 5..900 帽 + 504 rp_wsw_timeout 透出）+ GET /api/wsw/scan/providers（`_wsw_scan_providers`：rp-wsw scan providers 子进程委派——投影单源 Rust）+ API_ROUTES 一行 + `_wsw_post_route_wsw56` scan 分支；SSE 面与既有 /api/wsw/* 七端点零触碰 |
| `apps/server/selftest_wsw7.sh` | 新建 | 301 | S8：V-9 五项守卫预挂（硬守卫 exit 0 + podman/镜像/rt.py 分面 SKIP 计数）+ 登记面（validate json+yaml/register 副本/rule 2 重跑零残留）+ providers 投影逐字段对账 + CLI 负例/help 八连 + RTIS 真实委派 + Nuclei/ZAP 真实扫描（分面）+ rp_server 五查 + 生产库指纹 + 零 diff 反证 |
| `Cargo.lock` | 修改 | +3 | wsw-runtime 依赖块三行（tool-engine/tool-registry/verification-engine——D4 path 依赖强制写入面；`[[package]]` 399 恒定零新包，判据 10 字面冲突如实走坏消息） |
| `docs/README.md` | 修改 | +1 | 本报告索引登记（§9） |
| `docs/rip3/wsw/WSW-7-BATCH-REPORT.md` | 新建 | 本档 | 批次报告 |
| `containerfiles/README.md` | — | 零改动 | 选项 B：镜像未建成无台账行（台账 = 建成镜像 digest 实录；建成后的回钉流程见 §坏消息恢复路径） |

未触碰实证：`git status --porcelain` 只含上表落点（web/**、vendor/、sender.rs、scope.rs、listener/**、persist.rs、wsframe.rs、decoder.rs、comparer.rs、passive.rs、fuzz/**、tool-engine/src、mcp-server/src、plugin-registry/、plugin-runtime/、migrations/、integrations/rtis/**、wsw-message/** 全零 diff——selftest §8 反证 grep = 0）。

## 2. §5 收口命令实测（exit code + 关键输出行）

```bash
# ① S1 基线（d62e8b87 干净树）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# → EXIT=0；181 test result: ok；1301 passed（基线零红延续）

# ② 定向三 crate（收口态复跑）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p verification-engine -- --test-threads=1
# → ok. 43 passed (lib) + ok. 2 passed (provider e2e V-9 SKIP 面)
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p wsw-runtime -- --test-threads=1
# → ok. 86 passed (lib) + ok. 9 passed (tests/scanner_adapter.rs) + 其余 suite 全绿
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p tool-engine -- --test-threads=1
# → ok. 5 passed（policy_resolver_manifests：manifests_expectation_table_30 等）+ 全 suite 绿

# ③ S9 全量（终跑）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# → EXIT=0；182 test result: ok；1315 passed；零 FAILED（基线 1301 + 本批 +14）

# ④ 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p verification-engine -p wsw-runtime --all-targets -- -D warnings
# → EXIT=0（初跑三处 lint：unused import/too many arguments 8>7/redundant closure——修后归零）
rustfmt --edition 2021 --check crates/wsw-runtime/src/scanner.rs crates/wsw-runtime/tests/scanner_adapter.rs \
  crates/wsw-runtime/src/bin/rp-wsw.rs crates/tool-engine/tests/policy_resolver_manifests.rs
# → 全 clean（provider.rs 的 import 排序红为 HEAD 存量——A12「全仓存量红约 120 文件」口径，本批文件单独 rustfmt 达标）

# ⑤ deny / lock
cargo deny check advisories && cargo deny check licenses
# → advisories ok / licenses ok
grep -cE '^\[\[package\]\]' Cargo.lock    # → 399
grep -c 'name = "reqwest"' Cargo.lock    # → 0

# ⑥ 登记面（S2/S9 双点）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo run --offline -p tool-registry --bin rp-tool -- validate --manifest manifests/tools/rip.security.zap.json
# → {"ok":true,"tool_id":"rip.security.zap","version":"0.1.0","content_hash":"4f514254…"} EXIT=0
#   yaml 孪生 validate 同 EXIT=0
/home/emo/rt3-target/debug/rp-tool register --manifest manifests/tools/rip.security.zap.json --db migrations/dev-b1.db
# → {"ok":true,"superseded_previous":false,...} EXIT=0（首次落库；重跑 rule 2 fail-closed 拒绝见坏消息）
grep -rn "rip.security.zap" manifests/tools/ | wc -l   # → 2（json + yaml 孪生）
grep -c "9ff41ce9\|rip-zap@sha256" manifests/tools/rip.security.zap.json   # → 1

# ⑦ 镜像面
podman image exists localhost/rip-nuclei@sha256:9ff41ce98d4279ed528819ebc42cb32fe5b98916e8fb9a8633110fad83354cb8
# → 非零（镜像缺位——S1 实测与 A14 一致；本机重建 digest 4d0b47ef…≠钉值，见坏消息）
podman image exists localhost/rip-zap@sha256:0000…0   # → 非零（选项 B——镜像未建成）

# ⑧ 本批收口
bash apps/server/selftest_wsw7.sh
# → PASS=34 SKIP=4 FAIL=0；SELFTEST_WSW7: ALL GREEN（exit 0；SKIP=分面守卫
#   与实运行行各一对：nuclei 钉值镜像缺位守卫+e2e ×2、zap 镜像未建成守卫+e2e ×2）

# ⑨ 既有回归（零改动）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw56.sh
# → SELFTEST_WSW56: ALL GREEN；EXIT=0；42 [ok]（42/0 收口口径延续）

# ⑩ RTIS 零接触与生产库反证
git status --porcelain -- integrations/rtis/ | wc -l   # → 0
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db \
         migrations/platform-governance.db migrations/platform-search.db; do \
  sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done
# → 1 / 1 / 1 / 0 / 0（与 selftest_wsw56 收口时点指纹一致——只读未触碰）

# ⑪ 跨轨零 diff 反证
git status --porcelain | grep -c "web/\|vendor/\|sender.rs\|scope.rs\|tool-engine/src\|mcp-server/src\|migrations/\|plugin-registry/\|plugin-runtime/"
# → 0（Cargo.lock 面 = D4 三行依赖块，见坏消息——[[package]] 399 恒定实证）
```

## 3. 判据逐条状态（§4 判据表 12 条）

| # | 判据 | 状态 | 实测 |
|---|---|---|---|
| 1 | Nuclei 真实扫描对账 | **SKIP（V-9）** | 钉值镜像 `9ff41ce9…` 不在本机（S1 实测）；本机重建 digest `4d0b47ef…` ≠ 钉值（mtime 非确定性——两次构建必然异 digest，P5-9c「三建三钉」先例同因），且 nuclei manifest/provider 钉值属本批禁改面（§2.2）→ e2e 按 D6 预授权 SKIP。代码面就绪：`tests/scanner_adapter.rs::nuclei_real_scan_local_range_end_to_end`（守卫+对账+五态断言全写就，镜像在位即跑）+ selftest §5 同面。**镜像建成即自动出数字，禁模拟冒充面零妥协** |
| 2 | ZAP 真实扫描对账 | **SKIP（选项 B）** | 镜像未建成（传输坐死——坏消息 §1）。选项 B 验收路径（任务书判据 2 原文「选项 B 时按登记面验收并坏消息单列」）：登记面/代码面判据 4/5 全过（本表 4/5）+ selftest §6 分面 SKIP 计数 |
| 3 | RTIS 委派对账（零接触负例硬门） | **PASS** | stub rt.py fixture 矩阵全绿（`rtis_stub_delegation_matrix`：poc get 元数据锚 + poc use 回填 + exit 非零结构化透出 category=tool_unavailable）；真实 poc get V-9 守卫下回读（`rtis_real_poc_get_metadata_readback`：poc 900008 id 断言）+ 真实全链委派 `rp-wsw scan rtis --poc-id 900008` exit 0（metadata id=900008/content_omitted=true + use_recorded used_on 回填 hit_count 递增 5→8 实录）；**零 poc add 词形反证**（`rtis_zero_touch_source_grep`：scanner.rs 禁 `"poc", "add"`/`"poc", "update"`/knowledge.db 词形 + provider.rs record_verification 唯一在位面断言）；`git status --porcelain -- integrations/rtis/` = 0 行 |
| 4 | 登记面（Resolver assign 硬门） | **PASS** | 期望表 30 份 `[29,0,0,1]` 断言绿（`manifests_expectation_table_30` ok）；`rp-tool validate` json/yaml 双 exit 0；register 副本落库 + 重跑 rule 2 拒绝零副作用（zap tool_version 行数=1——「skipped 词形」任务书词形与 rp-tool 实测语义不一致，坏消息 §3）；GET /api/wsw/scan/providers 投影逐字段一致（selftest §2 python 逐字段 assert + 三 provider + plugin catalog 3）；/api/tools 露出 zap 行（selftest §7：`?tool_id=rip.security.zap` → available=true/source=aux/VALIDATED/T1/0.1.0 current——P6-9 aux 回退词形，dev-b1 本体为 first_batch_b1.rs 每跑重建的一次性副本，登记面持久证在 selftest 副本流程） |
| 5 | D-TC-03 帧协议与看门狗 | **PASS（代码面）** | wrapper stdout 第一行 `{"status":"result","data":{…}}` 逐字断言写就（zap e2e 测试 + selftest §6 双面，镜像在位即跑）；out/ 产物 artifact 收集（collect_out_artifacts 既有面 + partial 标注词形——podman_host.rs :1081 partial=true FAILED 路径既有）；超预算 TIMED_OUT 负例（`zap_watchdog_timeout_negative`：budget.timeout_seconds=1s → TIMED_OUT + INCONCLUSIVE + timeout 分类——镜像在位时跑；引擎面 601 拒绝负例 `budget_negative_*` 常跑 PASS）；cancel 路径零残留容器（--rm 兜底 :20 词形 podman_host 既有零改动） |
| 6 | 预算双闸与族帽 | **PASS** | max_requests 1001 → `input fails input_schema` 结构化拒绝（scan_nuclei face exit 1 实测）；budget.timeout_seconds 601>600 / 301>300 → `budget exceeds manifest cap` 引擎 submit 拒绝（`budget_negative_max_requests_and_timeout` 双 tool 断言）；provider 装配面 clamp 裁剪闸（601→600 断言）；FamilyLimiter scanner 族并发 = 1（`scanner_family_concurrency_default_is_one`：default=1 + 同族第二次 try_acquire None + 帽 2 两占满拒 + Drop 回占——podman_host.rs :172 面实测）；per-caller 帽词形零改动（tool-engine src 零 diff） |
| 7 | scope/egress 延续 | **PASS** | 三 adapter 派发全经 Engine submit + Adapter 链（scanner.rs 源面零 `Command::new("podman")` 词形——容器面唯一入口 = execute_container_via_engine；rtis = Command::new(python3) 子进程既有面）；egress_allowlist host 从 input.url 派生（scan_nuclei/scan_zap 恒 NetworkPlan::EgressAllowlist + NUCLEI/ZAP_EGRESS_PORTS）；非 allowlist 端口/域拒绝 = EgressProxy CONNECT 白名单既有面（ports 词表 [80,443,8080,8443] manifest 同源断言）；scope.rs/sender.rs 零 diff |
| 8 | 五态纪律与 RTIS 零直写 | **PASS** | 扫描结果零 Finding 写面（scanner.rs/provider.rs 源面零 evidence/finding 写词形；persist 面零触碰）；RTIS 回填仅 poc use 词形（判据 3 grep 断言）；五态判定唯一路径 = evaluator::evaluate（project_normalized 单入口；`project_normalized_hit_is_inconclusive_not_confirmed`：命中→INCONCLUSIVE/零命中→NOT_REPRODUCED/超时→INCONCLUSIVE+timeout 三面断言）；命中面 INCONCLUSIVE 投影（scanner 单测 + selftest 双面） |
| 9 | 独立入口面 | **PASS** | `rp-wsw scan --help` / `scan nuclei --help` / `scan zap --help` / `scan rtis --help` 全 exit 0（P9-3 stdout 统一口径新增——rp-tool 先例）；POST /api/wsw/scan 401 未授权 + 未知 provider 400 + spawn timeout 透出词形（504 rp_wsw_timeout——`_wsw_rp_wsw_json` TimeoutExpired 既有映射）；GET providers 401（selftest §7 四查 401/401/400/200 全过） |
| 10 | 零依赖与跨轨零 diff | **PASS（一处字面偏差如实）** | Cargo.lock 399 恒定 + reqwest=0 + vendor 零 diff + web/** 零 diff + integrations/rtis 零 diff + tool-engine/mcp-server/plugin-registry/plugin-runtime src 零 diff + migrations 001-030 零 diff + 路由钉 22 零随更（web/ 零 diff）+ `git status --porcelain` 只含 §2.1 落点；**Cargo.lock 零 diff 字面与 D4 path 依赖强制写入 3 行内在冲突**（依赖块三行，[[package]] 不变——坏消息 §2） |
| 11 | 全量回归 | **PASS** | workspace 全量 EXIT=0、1315 passed 零红（终跑）+ clippy -D warnings 通过 + deny 双检通过 + selftest_wsw56.sh 零改动回归绿（42/0） |
| 12 | WSW-8 前置输出 | **PASS** | 本报告 §6 专节（providers 响应形状词形 + UI 缺口清单 + ZAP 两核实项结论/选项记录） |

真实扫描实测数字汇总：**RTIS 真实库委派数字在场**（poc 900008：metadata id 回读 + use_recorded used_on=http://127.0.0.1:8080 回填 + hit_count 5→8 跨四次委派实录）；Nuclei/ZAP 容器真实扫描数字随镜像面（判据 1 SKIP/判据 2 选项 B——镜像建成即跑，测试与 selftest 数字面已写就，禁模拟冒充零妥协）。

## 4. 坏消息单列（四项必列 + 两项新增）

1. **ZAP 镜像未建成（D1 选项 B 触发——两核实项结论）**：①任务书原文 `docker.io/owasp/zap2docker-weekly` **已下架**（hub.docker.com API `object not found` + registry 匿名拉取 401 双证；`library/alpine`/`projectdiscovery/nuclei` 同刻 200 对照）——官方源迁移 **ghcr.io/zaproxy/zaproxy**（OWASP 官方同管线，manifest 可读，amd64 平台 manifest digest `sha256:20c11203…` 实读）；②大层传输坐死：624MB 层三次尝试（podman pull ×2 约 1.5h + curl `-C -` 续传脚本）速率 21-30KB/s 且反复停滞（curl `--speed-limit 10000 --speed-time 90` abort 循环实录；同刻小 blob 28MB 秒下——**限速只打大传输**），估算 8h+ 不可达批次窗口。**结论：触发选项 B（镜像不可得——传输面坐死）**；选项 C 不触发（拉取面部分可用非"双死"，代理键未实测因镜像缺位，loopback 面可达）。②之核实项 2（`zap.sh -help` 出站代理注入键实拍）**未实测**——wrapper 按公开 baseline.py 词形写就（`-cmd/-dir/-quickurl/-quickout` + `connection.proxy.chain.enabled/serverName/port` 上游代理链），词形待镜像落位后实拍核销。**恢复路径（主会话续作）**：`/tmp/zap_fetch_blobs.sh`（curl -C - 可续传，`/tmp/zap_fetch.log` 台账， blobs 落 `/tmp/zap-oci/blobs/sha256/`）→ 全 blob 到位后组 OCI layout（index.json + oci-layout）→ `podman pull oci:/tmp/zap-oci:weekly` → `podman build -t localhost/rip-zap:latest -f containerfiles/rip-zap/Containerfile containerfiles/rip-zap/` → digest 实取回钉四处（manifest json/yaml image+image_digest、provider.rs ZAP_IMAGE、Containerfile FROM）→ `podman run --rm localhost/rip-zap@sha256:<钉值> -version` 冒烟 → selftest §6 真实扫描数字面自动点亮。
2. **Cargo.lock「零 diff」字面与 D4 内在冲突**：D4 裁决 wsw-runtime 新增 workspace 内部 path 依赖——依赖解析**必然**把 tool-engine/tool-registry/verification-engine 三行写进 Cargo.lock 的 wsw-runtime 依赖块（git diff 实测恰 3 行 `+ "tool-engine",` / `+ "tool-registry",` / `+ "verification-engine",`，无其它行）；`[[package]]` 计数 399 恒定、零新包、vendor 零 diff（判据 10 的可检面全过）。判据 10「Cargo.lock 零 diff」按字面执行则 D4 不可实施——按 D4 优先落依赖、字面偏差如实登记本节（selftest §8 对账：LOCK_DIFF=3 + LOCK_DIFF_ALL=3 双断言钉住「只有这三行」）。
3. **rp-tool register 幂等词形与判据 4 不一致**：判据 4 原文「`rp-tool register` dev 副本幂等（重跑 skipped 词形）」——rp-tool 实测**无 skipped 词形**：同 (id, version) 重登记被 rule 2 VersionComparable fail-closed 拒绝（`version 0.1.0 is not greater than existing version 0.1.0`，exit 1，零副作用——tool_version 行数恒 1 实测）。幂等面 = 拒绝式（重复登记不可达），selftest §1 按实测语义断言并注记。另：dev-b1.db 本体为 **first_batch_b1.rs `reset_dev_copy()` 每跑重建的一次性副本**（本轮 workspace 回归即重建一次、S2 落库行被清、收口时已重新 register 回写）——登记持久证以 selftest 副本流程为准，dev-b1 行态为收口时点快照。
4. **selftest_wsw4.sh 28 陈旧钉维持已知红**（D5 红线 4：本批零链尾演进禁顺手改测试钉值——WSW-5+6 §E.6 登记口径不变；随下一链尾演进批随更，观察池延续）。
5. **Nuclei 镜像重建 digest 非确定性（判据 1 SKIP 根因）**：rip-nuclei 本机重建实测 digest `4d0b47ef…` ≠ 钉值 `9ff41ce9…`（COPY 层 tar 含 mtime——跨 checkout 重建必然异 digest）；钉值回取唯一路径 = 找回当日构建产物（不存在于任何 registry）或 nuclei manifest/provider 钉值随更（本批禁改面）。已重建镜像 `localhost/rip-nuclei:latest`（4d0b47ef）留本地 storage 未删（digest 派发面不引用，零行为影响）—— nuclei e2e 数字面待钉值镜像或钉值随更批次。
6. **本批 Cargo.toml 落地为三依赖非 D4 单行**：scanner 统一入口签名面需要 Engine/PodmanHostAdapter（tool-engine）与 Registry::open 装配（tool-registry）——verification-engine 不 re-export 上述类型（lib.rs 属禁改面不可加 re-export），rp-wsw scan_engine 装配（rp-verify engine_with_nuclei :181-189 同型）必需三面。三依赖全 workspace 内部 path、[[package]] 399 恒定、reqwest=0——D4 实质（零外部包）全过，单行词形扩展如实登记。

## 5. 未解决项

- ZAP 镜像构建与 digest 回钉（§坏消息 1 恢复路径；/tmp 续传脚本与 blob 半成品仍在后台续传——完成前登记面/代码面收口态不受影响）。
- Nuclei e2e 数字面（钉值镜像不可回取——随下一 nuclei 钉值演进批或手工恢复当日镜像后自动点亮）。
- ZAP 核实项 2（zap.sh -help 代理注入键实拍）与 wrapper 旗标词形核销（镜像建成时随 S3 回钉同批完成）。

## 6. WSW-8 前置输出登记（交付物 7 专节）

### 6.1 GET /api/wsw/scan/providers 响应形状词形全清单（UI 批零后端改动输入）

```json
{
  "providers": [
    {
      "provider_id": "nuclei",
      "tool_id": "rip.security.nuclei",
      "name": "Nuclei Template Scanner",
      "runtime": "podman_host",
      "family": "network",
      "permission_level": "T1",
      "idempotent": true,
      "network_mode": "egress_allowlist",
      "egress_ports": [80, 443, 8080, 8443],
      "image": "localhost/rip-nuclei@sha256:9ff41ce9…",
      "version": "0.1.0",
      "adapter_note": "execute_nuclei 既有链直接编排（podman_host 容器派发；模板经 template_ref/文件面）",
      "manifest": "manifests/tools/rip.security.nuclei.json"
    },
    { "provider_id": "zap",   "tool_id": "rip.security.zap",  …同字段词形（idempotent=false、image 占位钉值待回钉） },
    { "provider_id": "rtis",  "tool_id": "rip.kb.rtis",       …同字段词形（runtime=python_worker、family=knowledge、permission_level=T0、image=null、egress_ports=[]） }
  ],
  "plugins": {
    "catalog_entry_count": 3,
    "plugin_ids": ["knowledge.awesome-hacking", "knowledge.secskills", "knowledge.tongui-course-notes"],
    "source": "manifests/plugins/catalog.json",
    "note": "plugin-registry catalog 只读投影（D8 零新插件包零登记面）"
  }
}
```

POST /api/wsw/scan 请求词形：`{"provider":"nuclei|zap|rtis","url":"…","template_ref":"rtis-poc:N","template_file":"…","rate":N,"max_requests":N,"timeout_s":S,"alert_budget":N,"poc_id":N,"target":"…","task_id":"…","trace_id":"…","run_id":"…","timeout_api_s":120}`；响应 = `{"provider":…,"scan":{rp-wsw scan stdout JSON 帧全量——status_word/tool_run_id/trace_id/result/normalized{result_status,result_reason,error_category}/duration_ms…}}`；错误词形：400 bad_provider / bad_url / bad_poc_id / bad_target / bad_timeout、504 rp_wsw_timeout、503 rp_wsw_bin_missing、400 rp_wsw_failed（stderr 尾行透出）。

### 6.2 Scanner 工作台 UI 缺口清单（PLAN §5.2 :345-349 + Plugin Center :31）

- Scanner 入口面：providers 投影卡片（三 adapter 契约面 6.1 词形）+ 扫描发起表单（POST /api/wsw/scan 词形全字段）+ 扫描结果视图（normalized 五态徽标 + result 帧 + tool_run/artifact 引用跳转 /api/runs）——后端零改动即接。
- Plugin Center 面：plugins 投影（catalog_entry_count + plugin_ids 只读清单）——插件详情/信任面复用既有 /api/plugins + /api/plugins/trust-policy 读面。
- GUIDE grep `scan|plugin` 命中情况实测（2026-11-23）：docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md **scan 0 / plugin 0 命中**——GUIDE 现行稿零 Scanner/Plugin 工作台面，WSW-8 UI 批按 PLAN §5.2 :345-349 九入口面扩展设计稿后实施（本批后端零改动输入 = §6.1 词形）。
- 端点面缺口：GET /api/wsw/scan/{run_id} 扫描运行详情投影（现走 /api/runs 通用读面——够用；专用投影待 UI 批裁决）+ POST scan 异步化（同步委派 timeout_api_s 帽 900——长扫描 UI 面需 SSE/轮询语义裁决）。

### 6.3 三 adapter 性能基线数（UI 批回归对照基准）

- RTIS 委派（rp-wsw scan rtis，真实库 poc 900008）：端到端 <2s（rt.py subprocess poc get+use 两往返；selftest 实测 exit 0 即回）。
- GET /api/wsw/scan/providers（rp-wsw scan providers 子进程委派）：<1s（manifest 三份直读 + catalog 解析；401 投影同面）。
- Nuclei/ZAP 容器派发首拍：待镜像面（判据 1/2 SKIP——建成后的首次端到端耗时进 selftest 数字面补录）。

## 7. 回滚面

- 工作树未 commit——`git checkout -- <路径>` / 未跟踪文件删除即整批撤销（主会话审查后按批 commit 纪律落链）。
- manifest 双文件 + 期望表 + context-broker 计数随 revert 清零回 29/34；dev-b1 副本 zap 行 = 一次性副本零生产面（first_batch_b1 重建先例承载）；containerfiles/rip-zap 随删零残留。
- provider.rs ZAP 段 / scanner.rs / rp-wsw scan 面 / rp_server 两端点随 revert 消失（execute_nuclei/RTIS 既有面零触碰无回滚面）；wsw-runtime Cargo.toml 三依赖行 + Cargo.lock 三行随 revert 清零。
- 生产库/RTIS 目录/宿主软件零操作无生产回滚面；web/**/vendor/**/migrations 零 diff 无回滚面。

## 8. S9 遗留说明

- 本报告与 docs/README.md 登记在工作树落盘，**未 git add / commit**（环境纪律禁 commit/push——显式 add 本批清单由主会话执行）。
- /tmp 侧续传面（zap blobs + fetch 脚本 + 各次命令日志 /tmp/wsw7_*.log）为会话临时数据，重建步骤已固化进 §坏消息 1 恢复路径。
