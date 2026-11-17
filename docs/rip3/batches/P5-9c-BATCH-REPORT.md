# P5-9c 批次报告：本地靶场实拍与端到端验收（egress 链路贯通 + RTIS Finding 闭环正例 + §14 六场景演练）

> 批次：P5-9 子批三（本地靶场实拍与端到端验收）。规格源 = Phase 5 计划 §14（六场景）/ §7 P5-9 完成标准七条 + HANDOVER-P5-WIP.md 第四节 + rip3-p5-finish P5_9C_BRIEF；接续 = P5-9a（dd0860ae/942d453c 双绿）+ P5-9b（90311c1b/6f636123 双绿）。

## A. 本批完成内容

1. **本地靶场落仓库**：scripts/local_range.py（纯标准库 http.server，硬编码只绑 127.0.0.1:8443 不提供 --host 覆盖；/p59c-marker 返回固定 marker RIP3-P59C-LOCAL-RANGE-MARKER）+ scripts/local_range_template.yaml（检测模板，info 四字段齐全，word match 锚）。低影响原则：信息面特征零真实漏洞语义、loopback 强制、用完即停。
2. **egress 链路真实贯通（P5-5 G1 遗留落地，三层缺陷修复实录见 B 节）**：execute-plan --network egress 全链（host 派生 → pasta:-T 代理端口转发 → EgressProxy 白名单 → nuclei -proxy 显式代理 → 靶场 marker 命中 → matched_count 1）。镜像重建一次换钉（wrapper 两处修复），manifests/README 台账/provider 常量五处 digest 同步。
3. **RTIS Finding 闭环正例（P5-7 G1 遗留落地）**：rt.py poc add 新条目 900064（元数据面，POC 正文零复制）→ item-add --poc-id 900064 → execute-plan 命中 → evaluator INCONCLUSIVE（命中不等于 CONFIRMED，五态纪律语义正确呈现）→ 人工审证（provider_result 帧 matched_count 1 + hash 71ca2897… + trace tr-exec-…）→ result-set CONFIRMED（审证依据写入 reason）→ finding-link → rt.py poc update --verified sanctioned → poc 900064 verified=1 + verified_note 落库。零触碰既有 POC 条目。
4. **§14 六场景 CLI 面演练留证（E3 节）**：A（任务镜像 + 两模板入库）/ B（plan-build 双渠道 match + 逐条排除原因 + 预算面）/ C（本地靶场执行 + Evidence 双帧 + 五态归一）/ D（指纹识别 + profile-version 失效面）/ E（盲区假设登记 + 无证据 Finding 守卫拒）/ F（RTIS 闭环 + 六 ID 回溯）。
5. **浏览器端到端**：web/scripts/rip3_p5_9c_browser_e2e.mjs 11/11 断言全过 + 6 截图入库（I 节）；测试栈 rp_server 7801（RP_DB_PATH=/tmp/p5-9c-e2e/verify.db）+ vite 5173。
6. **定向测试**：verification-engine 全量 53 过 + clippy 零告警 + tool-engine 全套件串行 16 套件全绿（含真容器 podman_host_e2e/egress/budget 面）。

## B. 架构变化和复用说明

### egress 链路三层缺陷修复实录（坏消息单列）

首跑四项 INCONCLUSIVE 层层剥离，逐层根因与修复：

1. **nuclei 不读 env proxy（策略缺口，核心修复）**：podman_host 注入 http(s)_proxy env 指向 run 生命周期 EgressProxy（白名单代理），但 nuclei 完全无视 env proxy——实测死端口代理（127.0.0.1:9999）下仍"成功"零命中且零请求落目标面。更实质的后果：pasta 默认出站未网络层硬隔离（egress_proxy.rs 边界声明原文），nuclei 直连可绕过白名单，**白名单约束此前对 nuclei 实际失效**。修复 = wrapper.sh 在 `http_proxy` env 在场时显式 `set -- -proxy "$http_proxy" "$@"`（nuclei 旗标名是 `-proxy`，首发误用 `-http-proxy` exit 2 用法错误即时暴露即修）；无 env（network none 本机面）零旗标零行为变化。修复后请求经代理落靶场（靶场日志 05:35:01 GET /p59c-marker 200）。
2. **D-TC-03 帧协议 stdout 第一行不变量被命中行破坏**：nuclei 命中行打 stdout（`-o` 只落 JSONL 文件），有匹配时结果帧不再是 stdout 第一行 → 信封解析 `container frame status unexpected: <missing>`。零命中场景（P5-5/P5-6/P5-7 历史实拍）不触发。修复 = nuclei stdout 静默（`>/dev/null`，诊断走 stderr 照旧），命中详情以 result.jsonl artifact 承载，结果帧独占 stdout。
3. **egress host 派生要求 url 带 scheme**：`host_from_url` 对 "127.0.0.1:9443" 报 `url missing scheme`（派生面有意严格不猜测），靶场目标改 `http://127.0.0.1:8443` 即过。另发现靶场端口须在 Manifest egress.ports 白名单内（80/443/8080/8443），9443 不在内——靶场迁 8443，零代码改动走既有约束面。

镜像换钉流程（README digest 固定规则）：wrapper 修改 → podman build → podman inspect 实取 digest → 五处同步（manifests json ×2 / yaml ×2 / provider.rs NUCLEI_IMAGE 常量 / README 台账行），替换逐文件独立判定（9b K 节教训沿用）。最终 digest sha256:9ff41ce9…（三建三钉：ef2de221 → 1ecf3797 → 9ff41ce9，前两值为中间态见 git 历史）。

### 复用矩阵

| 能力 | 复用物 | 增量 |
| --- | --- | --- |
| egress 白名单面 | EgressProxy + host_from_url + Manifest egress.ports（P3-2/P5-5） | 零（约束面照常生效） |
| 容器派发链 | submit_correlated + podman_host Adapter（015/P5-5） | 零 |
| 五态归一化 | evaluator + ResultGuard（P5-6/P5-7） | 零（INCONCLUSIVE 语义首次真实呈现） |
| finding-link | rt.py poc update --verified sanctioned 既有接口（P5-7） | 零（正例首拍） |
| 指纹/假设/守卫 | fingerprint v2 + hypothesis-add + finding-link 守卫（P5-8/P5-7） | 零（场景 D/E 演练） |
| 浏览器验收 | rip3_p5_browser_e2e.mjs 骨架 | 新脚本同骨架 |

## C. 修改文件清单

| 文件 | 变化 |
| --- | --- |
| containerfiles/rip-nuclei/wrapper.sh | `-proxy` env 代理承载 + nuclei stdout 静默（帧协议不变量） |
| containerfiles/README.md | digest 台账 rip-nuclei 行（新建，含换钉实录与原值引用） |
| manifests/tools/rip.security.nuclei.json / .yaml | image 与 image_digest 换钉 sha256:9ff41ce9…（各 2 处） |
| crates/verification-engine/src/provider.rs | NUCLEI_IMAGE 常量同步 |
| scripts/local_range.py | 新文件（本地靶场，loopback 硬编码） |
| scripts/local_range_template.yaml | 新文件（检测模板） |
| web/scripts/rip3_p5_9c_browser_e2e.mjs | 新文件（浏览器端到端 11 断言） |
| docs/rip3/batches/browser-e2e/p5-9c/*.png | 6 张截图入库 |
| docs/rip3/batches/P5-9c-BATCH-REPORT.md | 新文件（本报告） |

工作树用户 WSW 素材 4 项不提交不还原；commit 显式 add 本批文件清单。

## D. 数据库迁移、Schema 和兼容性变化

零迁移（SQLite 链头 022 / PG 链头 V0006 零推进）。本批 schema 面 = Manifest runtime.image digest 换钉（登记面非 schema 面；Registry 九条校验照常通过，tool-engine contract 套件全绿）。镜像重建属运行时数据（README 缓存策略：镜像不入库，digest 登记入库）。RTIS 侧新增 poc 条目 900064（rt.py poc add 既有接口，非平台库）。

## E. 测试命令及真实结果（2026-11-18 实测）

### E1 定向测试

```
$ cargo test --offline -p verification-engine
lib 42 / provider 2 / store 9 全过；clippy --all-targets -D warnings 零告警
$ cargo test --offline -p tool-engine -- --test-threads=1（串行防容器派发挂死）
16 套件全绿 exit 0（contract 37 / podman_host_e2e 8 / egress 3 / budget 2 / python_worker 6 / … 真容器面在内）
$ cargo clippy --offline -p tool-engine --all-targets -- -D warnings 零告警
```

### E2 egress 链路实拍（靶场 127.0.0.1:8443，计划 vp-20261117-213601-413-01b0）

```
$ rp-verify plan-create --target http://127.0.0.1:8443 … → vp-…-01b0
$ rp-verify item-add --plan vp-…-01b0 --provider nuclei --target http://127.0.0.1:8443 --dimension input_points --poc-id 900064 …
$ rp-verify execute-plan --plan vp-…-01b0 --template scripts/local_range_template.yaml --network egress
vi-…-581-03ae → INCONCLUSIVE | template matched 1 finding(s): evidence review required before any conclusion
$ rp-verify evidence-list --plan vp-…-01b0
re-20261117-213602-738-0090 provider_result matched:1 hash 71ca2897… trace tr-exec-20261117-213601-…
$ 容器 result.jsonl（artifact）："matcher-status":true，"matched-at":"http://127.0.0.1:8443/p59c-marker"，响应体含 RIP3-P59C-LOCAL-RANGE-MARKER
$ 靶场日志：[05:35:01] 127.0.0.1 "GET /p59c-marker HTTP/1.1" 200
```

### E3 RTIS Finding 闭环正例（P5-7 G1 落地）

```
$ python3 integrations/rtis/rt.py poc add --stdin（JSON 元数据面）→ {"id":900064,"code":"local-range-marker-p59c","created":true,"verified":0}
$ rp-verify result-set --item vi-…-581-03ae --status CONFIRMED --reason 'evidence review: provider_result re-…-0090 (hash 71ca2897…) matched_count=1 via egress allowlist chain…'
→ result: CONFIRMED
$ rp-verify finding-link --item vi-…-581-03ae --note 'P5-9c e2e: CONFIRMED after evidence review…'
→ {"linked":true,"rtis_poc_id":900064,"rtis_response":{"updated":["verified","verified_note"]}}
$ rt.py poc get --id 900064 → verified:1 + verified_note 落库
$ git status integrations/rtis/ → 零 diff（POC 库非跟踪态）
```

### E4 §14 六场景演练证据

| 场景 | 演练面 | 证据 |
| --- | --- | --- |
| A 从 Project 建研究 | Research 任务镜像真实行 + 两模板入库经 Workflow Studio 装载 | 浏览器 01/02 截图 + rp-workflow put ×2 PASSED + version-chip-1 |
| B 知识生成计划 | plan-build 双渠道（pg-intel rp-kc exit 1 如实报错不猜测；rtis-poc 82ms 命中 2）+ 逐条排除原因（900064 "no component anchor"、POC 14 "component not in target profile"）+ 预算面（cap 40 / concurrency 1 / max_rounds 3） | match 原文（E5 补记）+ 浏览器 03 截图 |
| C 本地靶场执行 | egress 全链真实命中 + Evidence 双帧 + 五态归一 INCONCLUSIVE | E2 原文 + 浏览器 04 截图 |
| D 重复执行与缓存 | fingerprint 同输入两次同指纹 dfca48bb…（识别相同检测条件）+ profile-version v1/v2 互异（画像补段失效面）+ execute-plan 幂等恢复沿用 P5-6 实拍（resumed_skipped=1） | fingerprint 原文 ×4 |
| E 无命中补充研究 | coverage-show 六维盲区 → hypothesis-add（hi-…-7 proposed，来源依据/停止条件内置）→ finding-link 非 CONFIRMED 拒绝（finding_link_requires_confirmed）| coverage-show + hypothesis-add + 守卫拒原文 |
| F RTIS 与证据闭环 | CONFIRMED → finding-link → RTIS verified=1 + Evidence 六 ID（trace/tool_run/plan 三锚）+ RTIS 对账零回归 | E3 原文 + 浏览器 05/06 截图 |

### E5 浏览器端到端

web/scripts/rip3_p5_9c_browser_e2e.mjs **11/11 断言全过**（详见 I 节）。

### E6 CI

本批 commit push 后 `gh run watch` 收口，红则修复批再进 RELEASE；终态记入 HANDOVER。

## F. 性能数据

本批无新增基准项。egress 链路单次派发 829ms（provider_result 帧 duration_ms）至命中收敛；P10 不回归门（assemble/dispatch）不涉及本批改动面。

## G. 未解决问题与风险

1. **pg-intel 渠道 rp-kc search exit 1**（plan-build/match 双渠道之一）：本次演练环境 PG 检索面不可用，渠道错误如实透出（不静默不猜测），rtis-poc 渠道独立可用；PG 面随 Phase 8 基准批统一核查（rp-kc 依赖面自查）。
2. **场景 A"创建 ResearchTask"编排容器未建**（ResearchTasksPage 创建/暂停/恢复/取消编排面注明随验证契约批次，现状只读镜像 + 模板选择面）：P5 验收以模板入库 + 计划链（plan-create/plan-build CLI 面）承载，UI 创建编排如实登记为后续批次。
3. **场景 D"UI 显示跳过原因"**：execute-plan 幂等恢复（resumed_skipped）是 CLI 面（P5-6 实拍）；plans 页展示五态计数与状态，跳过原因明细属 Activity/详情页族后续。
4. **egress 白名单约束边界**（egress_proxy.rs 边界声明沿用）：代理层策略执行只对遵守代理的工具有效；nuclei 经本批 wrapper `-proxy` 修复后纳入约束面，其它第三方工具逐个接入时须同口径核查（登记为接入纪律）。
5. agent_run/artifact 两锚无实体沿用 P5-7 G2 登记（Evidence 档承载 Artifact 语义）。
6. 本地靶场端口白名单耦合 Manifest egress.ports：靶场换端口须在 80/443/8080/8443 内（本批 9443→8443 实录）。

## H. 下一批依赖和明确的非目标

Phase 5 RELEASE 收口依赖：本批六场景证据 + 三子批报告链（9a/9b/9c）+ 全量门禁静场串行复测。非目标：不接 ZAP、不做 Report Intelligence 与知识回流、不做 Proxy/MITM（WSW 扩展另行开工）、场景 A 创建编排容器不在本批扩面。

## I. UI 截图与浏览器验收证据

脚本 **11/11 断言全过**，截图入库 docs/rip3/batches/browser-e2e/p5-9c/：

| 截图 | 场景 | 断言 |
| --- | --- | --- |
| 01-sceneA-tasks.png | A | Research 任务镜像真实行 |
| 02-sceneA-templates.png | A | 渗透模板入库可选（version-chip-1 装载） |
| 03-sceneB-plans-confirmed.png | B | plan-build 计划行 + CONFIRMED 五态徽标 |
| 04-sceneC-evidence-marker.png | C/F | Evidence 详情六 ID 锚 + matched 1 + RTIS 引用 |
| 05-sceneE-coverage-blindspots.png | E | 盲区 uncovered 维度在列 |
| 06-sceneF-traces.png | F | Trace 六 ID 关联面（tr-exec 锚） |

### §7 P5-9 完成标准七条对账（全部带证据）

| 完成标准 | 状态 | 证据 |
| --- | --- | --- |
| 从 Project 创建 ResearchTask | 部分达成 | 任务镜像 + 模板选择面（浏览器 01/02）；UI 创建编排容器未建如实登记（G2），计划链由 plan-create/plan-build CLI 面承载 |
| 检索知识并生成计划 | 达成 | plan-build 双渠道 + 排除原因 + plan 落库（E4-B） |
| 运行本地靶场验证 | 达成 | egress 链真实命中（E2） |
| 形成 Evidence | 达成 | 双帧落档 hash/trace 三面贯通（E2） |
| Finding 走 RTIS 接口 | 达成 | finding-link 正例 → RTIS verified=1（E3） |
| Coverage 有实际变化 | 达成 | input_points 维 CONFIRMED 计数 + 六维盲区可见（coverage-show + 浏览器 05） |
| Workflow 可恢复且不重复运行已完成节点 | 达成 | execute-plan 幂等恢复语义（P5-6 实拍 resumed_skipped=1 沿用 + run_execution.rs 集成测试 9 过） |

## J. RTIS 对账和数据安全回归结果

finding-link 正例只走 rt.py CLI（poc add / poc get / 内部 poc update --verified sanctioned），零直写零行为变更；新条目 900064 元数据面承载（POC 正文零复制：description 只写锚引用与元数据，模板正文在仓库 scripts/local_range_template.yaml 不进 RTIS）；零触碰既有 POC 条目 verified 状态；commit 前 `git status integrations/rtis/` 零 diff。靶场 loopback 硬编码零外联；生产 migrations/platform*.db 零触碰（实拍库 /tmp/p5-9c-e2e/verify.db）。

## K. 本批教训实录

1. **第三方工具不读 env proxy 是白名单约束的静默失效点**：平台注入 http(s)_proxy env 只约束"遵守代理的工具"（document_fetch Transport 等自有面）；nuclei 实测完全无视 env（死端口代理仍零命中零请求）。第三方工具接入 egress 面时必须实测代理遵守性，不遵守者在 wrapper/调用面显式承载（本批 `-proxy` 旗标）。
2. **stdout 帧协议与第三方工具输出争抢**：D-TC-03"结果帧 = stdout 第一行"不变量会被工具自身 stdout 输出（nuclei 命中行）破坏；零命中历史实拍不暴露该类缺陷，首次真实命中才现形。wrapper 面 stdout 静默是通用解（帧独占 stdout，详情走 artifact 文件）。
3. **nuclei 旗标名以容器内 `-h` 实测为准**：代理旗标是 `-proxy`（`-http-proxy` 不存在，exit 2 即时暴露）；容器内 `-h | grep` 一次定位，不凭记忆写旗标。
4. **pasta:-T 只转发显式列出的端口**：容器内 127.0.0.1:<port> 仅在 pasta:-T,<port> 列出的端口上等于宿主 loopback 同端口；目标端口的实际连通靠白名单代理在宿主侧发起（代理上游连接不经 pasta）。
5. **pkill 自匹配误杀**：pkill -f 模式含在调用 shell 自身命令行时自杀（本批靶场重启实录）；用 ^python3 锚定可执行名规避。
