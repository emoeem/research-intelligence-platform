# Phase 3 阶段验收记录：首批 Tool Packs（P3-6 收口交付物）

- 日期：2026-11-10
- 范围：ROADMAP §4 验收命令全套执行与阶段收口；两包交付（Web Research、Core Utilities）+ 三包 planned 存根 + P3-7/P3-8 挂起承接（PHASE3-DESIGN §2.3 与 manifests/packs/planned/ 五存根对应）。
- 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §14 Phase 3（774 到 785 行，784 行原文为本阶段范围裁定依据）；docs/phase0/PHASE-ROADMAP.md §4.4（验收命令形式）；docs/phase0/TOOL-RUNTIME-ADR.md §3.2、§5、§7；docs/phase3/PHASE3-DESIGN.md。
- 红线遵守声明：本文为验收记录，内容全部来自本会话实际执行的命令与测试；撰写过程未在宿主机安装依赖（pacman -Q 快照 2415 行全程无变化，见 §4），未写 migrations/ 下任何 platform*.db，未启动任何虚拟机，未修改生产 RTIS 行为。
- 载具：crates/tool-engine/tests/web_research_e2e.rs（2 用例）与 core_utils_e2e.rs（3 用例），经 Gateway::dispatch（runtime-gateway lib.rs:196，register_node + CommandProbe::podman 真实探针）派发，不绕过 Gateway 直调 Adapter。

---

## 1. 计划 §14:784 五点可靠链路对照（两包交付口径）

计划 §14 Phase 3 验收口径原文：「不追求工具数量，优先保证安装、运行、结果 Schema、错误处理和证据归档可靠。」逐点对照：

| 可靠链路点 | 达成方式与证据 | 状态 |
|---|---|---|
| 安装 | 两包镜像构建全程容器内（FROM 与 rip-tool-base 同款 digest 固定 python:3.12-slim，镜像内零包安装，工具本体纯标准库）；宿主 pacman -Q 基线 2415 行全程无变化（§4） | 达成 |
| 运行 | 六工具（rip.http.inspect + 五 core 工具）各至少一次经 Gateway::dispatch 到 podman_host 真实容器 run SUCCEEDED（§2/§3 实录） | 达成 |
| 结果 Schema | 六 Manifest input/output schema 经 jsonschema_lite 在提交面与信封内双向生效（P3-3 POST 拒绝、P3-4 null 字段教训与修复）；TOOL-CONTRACT §9.1 形状对齐（method 枚举、save_body、body_artifact_id、status_code/final_url） | 达成 |
| 错误处理 | 坏输入 FAILED 且跃迁合法（[Queued,Preparing,Running,Failed]）；非白名单 CONNECT 代理 403 落结构化错误（P3-2 npmjs.com 重定向实证）；POST 与 yaml 在登记面拒绝；嵌套 json 转换面拒绝 | 达成 |
| 证据归档 | out/ 全量产物 sha256 登记 artifact 表（primary/attachment role 实测）；body.bin 与 sample.txt.gz 等落盘文件 sha256sum 双向对账一致（§2/§3） | 达成 |

数量裁定：交付 6 个已登记工具（Web Research 1 + Core Utilities 5），三包（Source Analysis、Document、Mobile Analysis）与 TPscan（P3-7）、Browser Automation（P3-8）为 planned 存根（manifests/packs/planned/，schemas/pack-plan/v1.json 形状，.venv-check/bin/python Draft202012Validator 校验通过），不进 Registry 不建镜像（裁定依据 PHASE3-DESIGN §1 两条原文）。

## 2. ROADMAP §4.4 验收命令（第 1、2 步实录）

第 1 步（派发 run；CLI 形态原文承认随 P1-5 定稿，实际派发入口为测试载具经 Gateway::dispatch，见 PHASE3-DESIGN §3.1 第 7 条与 §6 执行偏差记录）：

~~~bash
cargo test --offline -p tool-engine --test web_research_e2e
# test result: ok. 2 passed; 0 failed; 0 ignored（exit=0）
cargo test --offline -p tool-engine --test core_utils_e2e
# test result: ok. 3 passed; 0 failed; 0 ignored（exit=0）
~~~

第 2 步（Run 终态与 Artifact 只读对账，开发库副本；python3 -I + sqlite mode=ro）：

~~~bash
python3 -I -c "import sqlite3; c=sqlite3.connect('file:/tmp/rip-p33-devcopy/platform-devcopy.db?mode=ro',uri=True); print(c.execute('select run_id,status,tool_id from tool_run order by rowid').fetchall())"
# [('rn-p33-ok', 'SUCCEEDED', 'rip.http.inspect'), ('rn-p33-head', 'SUCCEEDED', 'rip.http.inspect'), ('rn-p33-nosave', 'SUCCEEDED', 'rip.http.inspect')]
python3 -I -c "import sqlite3,glob; p=sorted(glob.glob('/tmp/rip-p34-ok-*'))[-1]; c=sqlite3.connect(f'file:{p}/platform-devcopy.db?mode=ro',uri=True); print(c.execute('select status, tool_id from tool_run order by rowid').fetchall())"
# [('SUCCEEDED', 'rip.core.file_meta'), ('SUCCEEDED', 'rip.core.hash'), ('SUCCEEDED', 'rip.core.compress'), ('SUCCEEDED', 'rip.core.convert'), ('SUCCEEDED', 'rip.core.validate')]
~~~

Artifact 对账（sha256 双向一致；P3-4 载具 primary 取排序首位 result.json，工具产物为 attachment，与 P3-3 body.bin primary 并存为两种归档形态）：

~~~bash
# Web Research：body.bin（primary, 577 字节）
python3 -I 查 artifact 表 → art-rn-p33-ok-0-body.bin sha256=25ddf2c8...e484
sha256sum /tmp/rip-p33-devcopy/ws/rn-p33-ok/out/body.bin → 25ddf2c883e0d1958ea971d279a7e4f0fd446724ee3db7db19dadabd4a62e484（一致）
# Core Utilities（rn-p34-gz 全产物，attachment 行含工具产物）
artifact 表 → art-rn-p34-gz-0-result.json(primary) eb4a4c63...80b4 / art-rn-p34-gz-1-sample.txt.gz(attachment) cf93574d...b9c
sha256sum → eb4a4c639b49...80b4 / cf93574d7c3e...b9c（逐字一致）
~~~

## 3. ROADMAP §4.4 验收命令（第 3、4 步实录）

第 3 步（镜像 digest 固定复核）：

~~~bash
WD=$(grep -o 'sha256:[0-9a-f]*' containerfiles/rip-tool-webresearch/tool-manifest.json | head -1)
podman inspect "localhost/rip-tool-webresearch@$WD" --format '{{.Digest}}'
# sha256:bf910119b26426c107e9b1adfd722def3473fa1e260e0f447a9514e57f365f91（与 Manifest 双份、README 台账一致）
CD=$(grep -o 'sha256:[0-9a-f]*' containerfiles/rip-tool-coreutils/rip.core.hash.json | head -1)
podman inspect "localhost/rip-tool-coreutils@$CD" --format '{{.Digest}}'
# sha256:c90631b9269d42cb6a7f237dbbfd888f41df970152fd8c8121033089603b2eeb（与五 Manifest、README 台账一致）
~~~

第 4 步（契约测试，与 CI 同门）：

~~~bash
cargo test --offline --workspace
# workspace passed=207 failed=0（194 基线 + 13 新用例：egress 5 + web_research 2 + core_utils 3 + egress_proxy 单测 3）
cargo clippy --offline --workspace --all-targets -- -D warnings
# Finished dev profile（零告警，exit=0）
cargo fmt --all -- --check
# 通过（exit=0）
python3 -m unittest discover -s tests -v   # workers/python 目录下，CI 同命令
# Ran 125 tests OK (skipped=11)
~~~

## 4. 红线复核记录（收口时点实测）

- 宿主零安装：`pacman -Q | wc -l` = 2415，与 Phase 3 起点快照一致（P3-0 实测 2415），全程 diff 为空；镜像构建/拉取属运行时数据不属安装（ADR §3.2 口径）。
- 零生产库写入：`git status --short -- migrations/` 输出空（本阶段零新增迁移，PHASE3-DESIGN §2.4 零迁移声明兑现）；platform.db 与 platform-dryrun.db 零改动，库操作全部走开发库副本（/tmp/rip-p33-devcopy、/tmp/rip-p34-* 文件库）与测试内存库。
- 清理面：`podman ps -a --filter label=rip.runtime=rip-tool -q | wc -l` = 0；`podman images --filter dangling=true -q | wc -l` = 0（重建产生的旧 digest 悬空镜像已清）；rip-tool 镜像仅三个（rip-tool-base:smoke + 两 pack）；用户自用容器面未触碰；`podman system df` Images SIZE 2.775GB，低于 50 GiB 预算（ADR §7）。
- 文档长横线：`grep -rP '[\x{2014}\x{2013}]' docs/phase3/` 退出码 1（零命中）；DOC-STANDARD 提交前检查口径全 docs 复扫亦零命中。
- VM 面：本阶段未执行任何 VM 写操作，S1 到 S4 维持挂起（PHASE2-DESIGN §5.2.5 不变）；planned 存根承接 P3-7/P3-8 与 ADR §3.1 条件 1/2。

## 5. V 表与占位消解状态

- V-10（rip.http.inspect 真实镜像）：已关闭（P3-3 批次，PHASE-ROADMAP V 表行内标注）；TOOL-CONTRACT-DRAFT §11 待验证项 1 消解，`grep -c 'sha256:<digest>' docs/phase0/TOOL-CONTRACT-DRAFT.md` = 0（全部占位回填真实 digest）。
- V-5/V-6/V-7/V-8（VM 面）与 V-9（CI 无 podman）：维持待验证，容器端到端保持本地执行，容器用例 podman 不可用时自动跳过保持 CI 绿（各测试文件门禁段）。

## 6. 批次全景

| 批次 | 交付物 | 验证 |
|---|---|---|
| P3-0 | PHASE3-DESIGN.md（§24 十节，范围裁定与遗留承接表） | 文档自检（十节计数 10、零长横线）+ 质量门基线 194 |
| P3-1 | 两包镜像族（Containerfile + 纯标准库工具本体 + digest 台账） | 双镜像构建 exit=0，digest 三处一致，容器内功能验证（hash/roundtrip/GET 200/负向） |
| P3-2 | egress_proxy + network.mode 分派 + env 注入（podman_host_egress 5/5） | 正负契约测试全绿，pasta:-T 实测冻结，example.com 经代理 200 |
| P3-3 | rip.http.inspect 登记 + Gateway 全链端到端（web_research_e2e 2/2） | TOOL-CONTRACT §9.1 回填消解 §11.1 与 V-10，§4.3 三项验证落盘 |
| P3-4 | Core Utilities 五工具登记 + 端到端（core_utils_e2e 3/3） | 五 run SUCCEEDED 对账，坏输入/校验九条负例/FamilyLimiter 基线 |
| P3-5 | 三包 planned + P3-7/P3-8 存根（pack-plan v1 + 五 JSON） | Draft202012Validator 校验通过，image 字段零命中，containerfiles 三族 |
| P3-6（本批） | 本验收记录 + RELEASE_2026-11-10_PHASE3.md | §4.4 四步全套 + 红线复核 + 质量门 207 |

## 7. 遗留与后续（如实声明）

1. 三包 planned（Source Analysis/Document/Mobile Analysis）与 TPscan、Browser Automation：解锁条件随存根 exit_conditions，等 Phase 3 验收后的批次预算与 VM 通道解锁，本阶段不实现。
2. rip-gateway systemd 常驻编排与引擎层 submit_key 唯一性加固：Phase 2 遗留顺延项，本阶段未触碰（PHASE3-DESIGN §3.2 承接表）。
3. egress allowlist 为代理层策略执行（自有可信工具面），网络层硬隔离属 podman_vm 目标拓扑（ADR §8 已知代价，P3-2 已声明）。
4. npmjs.com 重定向依赖（P3-2 负向用例触发面）与 example.com 外网可达性：公共端点行为变化时需按测试头注更换触发源。
