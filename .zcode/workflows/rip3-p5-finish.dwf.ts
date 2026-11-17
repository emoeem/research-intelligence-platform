/* zcode-workflow
name: rip3-p5-finish
description: "RIP3 Phase 5 续作收官：P5-8 收尾（hypothesis 实拍链 + 集成测试 + 批次报告）→ P5-9 三子批（Workflow 模板与画布 / Evidence 与 Coverage 页面 / 本地靶场实拍与端到端验收）→ Phase 5 RELEASE（编排面全量门禁复测 + RELEASE 报告 + HANDOVER 勾销）。每子批自动化入库（显式 add + 中文长描述 commit + push 重试 + CI watch），收口经编排面确定性门禁复测与 HEAD CI 确认，受阻处置站逐段内建。接手断点 docs/HANDOVER-P5-WIP.md（P5-8 收尾）。"
whenToUse: "P5-0 至 P5-7 已收口、P5-8 半成品已 WIP 入库后，驱动 Phase 5 剩余全部内容（P5-8 收尾 → P5-9 → RELEASE）的自主收官；仓库根 /media/emo/code/pensting/research-platform。取代 rip3-finish 工作流中 Phase 5 段的粗粒度 brief。"
scope: project
*/

interface BatchOutcome {
  /** 子批名（如 "P5-8 收尾"、"P5-9a Workflow 模板与画布"）。 */
  batch: string;
  /** 本批批次报告路径（docs/rip3/batches/…，RELEASE 批为 docs/releases/…）。 */
  batches: string[];
  /** 本批入库提交短哈希（按时间序）。 */
  commits: string[];
  /** 本批最后一次 push 对应 CI run 数据库 ID（取不到时如实写 "unconfirmed"）。 */
  ciRunId: string;
  /** 遗留问题（如实登记，不隐藏）。 */
  leftovers: string[];
  /** 本批收口时 HEAD 短哈希。 */
  head: string;
}

interface Finding {
  /** 位置（子批 / 文件路径）。 */
  where: string;
  /** 一句话说明发生了什么。 */
  what: string;
  /** 证据：命令与输出尾部。 */
  evidence: string;
  /** "verified" = 确定性门禁或独立确认过；"unconfirmed" = 未复核。 */
  status: "verified" | "unconfirmed";
  /** 影响量级；high 保留给数据损失或结果错误。 */
  severity: "low" | "medium" | "high";
}

interface GateResult {
  /** null = 门禁与 CI 全过；否则为受阻原因摘要。 */
  block: string | null;
  /** 本轮复测中发生并已修复的门禁/CI 事件。 */
  incidents: Finding[];
}

interface WorkflowReport {
  /** 两三句话回答「Phase 5 剩余全部内容完成了吗」。 */
  conclusion: string;
  findings: Finding[];
  /** 本运行实际执行过的校验（命令级）。 */
  verified: string[];
  /** 未覆盖面与原因。 */
  notCovered: string[];
}

const REPO = "/media/emo/code/pensting/research-platform";

/** 仓库根确定性命令面（bash -lc 定位仓库根；argv 携带运行时值）。 */
function repo(cmd: string, timeoutMs?: number) {
  return world.run("bash", ["-lc", `cd ${REPO} && ${cmd}`], {
    timeoutMs: timeoutMs ?? 1_800_000,
    output: { tail: 16384 },
  });
}

/** Phase 5 收口全量门禁全集（交接文档第四节第五条五道门禁 + selftest 全集 + RTIS 零改动）。静场串行，编排面统一复测。 */
const GATES: [string, number][] = [
  ["cargo test --offline --workspace", 1_800_000],
  ["cargo clippy --offline --workspace --all-targets -- -D warnings", 1_200_000],
  ["python3 -m unittest discover -s workers/python/tests", 900_000],
  ["npm --prefix web run build", 600_000],
  ["npm --prefix web run test", 900_000],
  [
    'for s in apps/server/selftest_phase*.sh apps/server/selftest_auth.sh; do bash "$s" || exit 1; done',
    1_800_000,
  ],
  ['test -z "$(git status --porcelain -- integrations/rtis)"', 60_000],
];

async function runGateSet(): Promise<string[]> {
  const failures: string[] = [];
  for (const [cmd, t] of GATES) {
    const r = await repo(cmd, t);
    if (r.exitCode !== 0) {
      failures.push(`$ ${cmd}\nexit=${r.exitCode}\n--- stdout tail ---\n${r.stdout}\n--- stderr tail ---\n${r.stderr}`);
    }
  }
  return failures.map((f) => f.slice(0, 4000));
}

/** HEAD 对应 CI run 的确定性确认：gh run watch --exit-status（gh EOF 间歇重试）。 */
async function ciWatch(): Promise<string | null> {
  const head = await repo("git rev-parse HEAD");
  const sha = head.stdout.trim().split("\n")[0] ?? "";
  if (!/^[0-9a-f]{40}$/.test(sha)) {
    return `无法取得 HEAD（${head.stdout.slice(0, 80)}）`;
  }
  let rid = "";
  for (let attempt = 1; attempt <= 5; attempt++) {
    const find = await repo(
      `gh run list --branch main --limit 10 --json databaseId,headSha -q '.[] | select(.headSha=="${sha}") | .databaseId' | head -1`,
    );
    rid = find.stdout.trim();
    if (/^[0-9]+$/.test(rid)) break;
    await repo("sleep 15");
  }
  if (!/^[0-9]+$/.test(rid)) {
    return `HEAD ${sha.slice(0, 8)} 无对应 CI run（未 push 或 gh 持续不可达）`;
  }
  const watch = await repo(`gh run watch ${rid} --exit-status >/dev/null 2>&1; echo CIV=$?`, 1_500_000);
  const civ = /CIV=(\d+)/.exec(watch.stdout)?.[1];
  if (watch.exitCode === 0 && civ === "0") return null;
  return `CI run ${rid} 未绿（watch exit=${watch.exitCode} CIV=${civ ?? "?"}）`;
}

/** 一轮收口 CI 确认：HEAD CI 确认 + 一轮修复重验（修复批交对应实施工程师）。 */
async function ciEnsure(impl: Agent, label: string): Promise<GateResult> {
  const incidents: Finding[] = [];
  const ci1 = await ciWatch();
  if (ci1 === null) return { block: null, incidents };
  log(`${label} HEAD CI 未绿（${ci1}），交实施工程师一轮修复后重验`);
  await impl.ask<string>(
    `HEAD CI 确认未绿（编排面复测）：${ci1}\n用 gh run view --log-failed 定位失败 job 与用例，修复并以独立修复批 commit + push（显式 git add 文件清单，禁 git add -A），push 后 gh run watch 至绿。`,
  );
  incidents.push({
    where: `${label} CI 确认`,
    what: "HEAD CI 未绿后实施工程师修复重推",
    evidence: ci1,
    status: "verified",
    severity: "medium",
  });
  const ci2 = await ciWatch();
  if (ci2 === null) return { block: null, incidents };
  return { block: `CI 两轮未绿：${ci2}`, incidents };
}

/** Phase 5 收口全量门禁复测：三轮修复回路（修复批交收口工程师），全过返回 null。 */
async function gateFixLoop(impl: Agent, label: string): Promise<GateResult> {
  const incidents: Finding[] = [];
  let failures: string[] = [];
  for (let round = 1; round <= 3; round++) {
    failures = await runGateSet();
    if (failures.length === 0) break;
    log(`${label} 全量门禁第 ${round}/3 轮未过（${failures.length} 项），交收口工程师修复`);
    await impl.ask<string>(
      `收口全量门禁复测失败（编排面静场串行复测，第 ${round}/3 轮）。失败项与输出尾部：\n${failures.join("\n\n")}\n修复并以独立修复批 commit + push（显式 git add 文件清单，禁 git add -A）。不要自行复跑全量门禁，编排面统一复测。`,
    );
    incidents.push({
      where: `${label} 收口全量门禁`,
      what: `第 ${round}/3 轮门禁失败（${failures.length} 项）后交收口工程师修复`,
      evidence: failures[0],
      status: "verified",
      severity: "medium",
    });
  }
  if (failures.length > 0) {
    return { block: `全量门禁三轮未过，首项：${failures[0]}`, incidents };
  }
  return { block: null, incidents };
}

const IMPL_PERSONA = `你是 RIP3 Phase 5 续作的资深实施工程师，独立负责一个子批的完整交付与自动入库。仓库根 = ${REPO}；所有文件工具与 shell 命令一律用绝对路径定位仓库内文件；开工先读仓库根 AGENTS.md、docs/HANDOVER-RIP3-CONTINUATION.md 与 docs/HANDOVER-P5-WIP.md。铁律：继承优先（trace 六 ID、Tool Contract 信封、workflow-engine、Context Broker、podman 执行面、audit correlation、source_sync、RTIS 接口八项复用不重建，禁建第二套 Workflow Runtime / Tool Registry / Findings 库）；RTIS 红线（读写只走 rt.py CLI / rtis MCP 工具面零直写零行为变更，POC 正文零复制，15155 对账零回归）；数据库链纪律（情报核新表走 migrations_pg，PG 链头 V0006；SQLite 仅运营面小迁移，链头 022，verification research 表沿 P5-0-B3 判定属运营面；演进链尾先按五形状全量定位九个钉值文件再改：vec![1, 2、vec![13, 14、(2X, \"_ 链尾名、uv1?、001.. 区间枚数）；五态纪律（Phase 5 计划 §5.7：工具退出码 0 不等于 CONFIRMED；失败/超时不等于 NOT_REPRODUCED；无命中不等于无漏洞；未测试不得计为通过；FALSE_POSITIVE 必须有明确依据；新代码不得绕过 ResultGuard 与 checked_normalized_doc）；授权审批节点不内置；宿主机零系统级安装（npm 只进 web/，Python 重依赖只进容器镜像，不用 sudo）；生产 migrations/platform*.db 零触碰；不 reset/clean/checkout/stash/rebase 改写历史；不删失败测试不降验收标准。工作树保护：仓库有 4 项用户未提交 WSW 素材（docs/README.md 修改、docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md 修改、docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md 与 docs/ui-ux/ 未跟踪）——绝不提交、不还原、不清理、不混入；commit 一律显式 git add 本批文件清单，禁 git add -A 与 git add .；每批 commit 后 git status 核对这 4 项仍在。环境事实：RP_DB_PATH 必须随每条 rp-verify CLI 命令（Bash 环境不跨调用持久，漏带落内存空库，症状 not_found）；podman shim 在 /tmp 会被清（mkdir -p /tmp/podman-shim && printf '#!/bin/sh\\nexec distrobox-host-exec podman \"$@\"\\n' > /tmp/podman-shim/podman && chmod +x /tmp/podman-shim/podman 后 export PATH）；端口 7800 被生产 rp_server 常驻占用勿动，测试 rp_server 用 7801+，vite dev 需 RP_API_TARGET=http://127.0.0.1:7801 且监听 [::1]（curl 用 localhost）；批量替换脚本逐对独立判定独立写（整文件 sub() 遇 MISS 跳过其余已配对）；ALTER 必须单行书写（migrate.rs 行守卫切断多行 ALTER）；web/tests/workspaces.test.ts 路由钉值 16；web/scripts/rip3_p5_browser_e2e.mjs 为可扩展浏览器验收模板；gh/api.github.com 间歇 EOF，重试 2 到 3 次仍败改走 commit checks 页面。批次纪律：每批按 Phase 5 计划 §13 A-J 十节出批次报告落 docs/rip3/batches/（无证据写未验证，坏消息单列，中文零 em/en dash，数字实测写来源）；批内只跑定向测试（收口全量门禁由编排面统一复测，你不要在收口时自行复跑）；commit 中文长描述 + push（SSH 断连重试 2 到 3 次）+ gh run watch 至绿，CI 红不进下一批；同一失败方法两次无新信息就换本质不同的方法。若门禁不可能通过、验收互相矛盾或遇到只有用户能定的事，escalate 并说明，不要绕过。`;

const P5_8_BRIEF = `收口 RIP3 P5-8（Coverage Matrix 与自适应研究）。接手断点：4b949614 WIP 已入库，当前 HEAD 树含全部半成品（022 迁移 / coverage.rs / 指纹 v2 / rp-verify 四子命令），clippy 零告警 + 定向测试全绿为交接记录口径，你先实测复核。

工作内容（按序）：
1. 前置核实：cargo test --offline -p verification-engine（预期 42 lib + 2 provider + 6 store 全过）与 cargo clippy --offline -p verification-engine --all-targets -- -D warnings（预期零告警）；红先修再继续。
2. hypothesis 实拍链重跑。上轮 not_found 根因 = RP_DB_PATH 未随命令携带（环境未挂非代码缺陷）；每条命令前 export RP_DB_PATH=/tmp/p5-8-e2e/verify.db。/tmp 被清则先重建场景（重建库、plan-create、item-add --dimension assets）再跑链。链路：hypothesis-add 正例（--plan vp-20261116-200640-186-006a --dimension assets --content/--source/--association/--stop 四必填齐备，可选 --preconditions/--expected/--negative-control/--suggested/--cost-risk）→ 负例一 --source 传空串（必填守卫拒，无证据不生成）→ 负例二 --round 3（该计划 budget max_rounds=2，轮次守卫拒）→ hypothesis-list --plan 确认在库 → hypothesis-status --id <正例回显 id> --status in_plan → --status validated → 终局负例 validated 后再 --status proposed（validated 终局不迁出，守卫拒）。七维词值：assets / api_endpoints / identity_permissions / input_points / business_flows / file_processing / data_flows。全部输出原文留档。
3. hypothesis 集成测试 2 条（crates/verification-engine/tests/store.rs 惯例：store() fixture + NewHypothesis）：生命周期正例（add → list/get → status 推进）与守卫负例（空 source / 超 max_rounds / validated 终局不迁出）。
4. 定向复测：cargo test --offline -p verification-engine 全过 + clippy 全 targets 零告警（如动 crates/domain 再跑 cargo test --offline -p domain）。
5. docs/rip3/batches/P5-8-BATCH-REPORT.md（A-J 十节）：E 节贴实拍原文；教训三条必写（迁移链尾钉值五形状定位法 / 批量替换 sub() 整文件跳过陷阱 / ALTER 必须单行书写）；G 节如实登记遗留；I 节引两组既有实拍（coverage-show 七维矩阵与盲区 PASS、指纹 v2 补段失效三键互异 PASS）加本批新实拍。
6. 收口：显式 git add 本批文件清单 → commit 中文长描述 → git push origin main（SSH 断连重试 2 到 3 次）→ gh run watch 至绿。
返回 BatchOutcome（batch="P5-8 收尾"，batches/commits/ciRunId/leftovers/head 全如实填写）。`;

const P5_9A_BRIEF = `实施 RIP3 P5-9 子批一（Workflow 模板与画布）。开工先读：docs/HANDOVER-P5-WIP.md 第四节、Phase 5 计划 docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md §6（Workflow 模板要求）与 §7 P5-9 行、workflows/ 目录既有 JSON 定义惯例（parse-pipeline-v1.json / review-card-v1.json）与 workflow-engine 018 控制面。

工作内容：
1. 通用渗透测试十三步 Workflow 模板（§6.1 十三步逐条）落 workflows/ 目录 JSON 定义惯例；
2. SRC 漏洞挖掘十三步模板（§6.2 十三步逐条）同上；Finding 与报告事实只经 RTIS 既有接口；
3. 节点 Schema 表单：先查 Node Registry 与既有 DSL 再决定新增节点类型；节点参数 JSON Schema 驱动，禁每页自建不一致的参数表单；循环节点带最大迭代次数、超时与预算上限；
4. 画布保存与版本：复用既有 XYFlow 画布与 workflow-engine 版本面。
硬性边界：两份模板十三步逐条映射到既有节点类型，缺失节点先 ADR 记录再最小新增；模板实例化测试（§7 P5-9「Workflow 可恢复且不重复运行已完成节点」语义由模板实例化与暂停恢复面共同覆盖，本批至少实例化两模板跑通非执行面验证）；禁建第二套 Workflow Runtime；禁接 ZAP；不动 runtime-gateway 内核。
收口：定向测试（cargo test --offline -p workflow-engine；web 面涉及则 cd web && npm run build && npm run test）+ A-J 报告 docs/rip3/batches/P5-9a-BATCH-REPORT.md + 显式 git add 文件清单 commit + push + gh run watch 至绿。
返回 BatchOutcome（batch="P5-9a Workflow 模板与画布"）。`;

const P5_9B_BRIEF = `实施 RIP3 P5-9 子批二（Evidence 与 Coverage 页面）。开工先读：docs/HANDOVER-P5-WIP.md、docs/rip3/batches/P5-9a-BATCH-REPORT.md 与近期 git log（接续子批一产物）、Phase 5 计划 §4（Web 工作台总体设计）与 §7 P5-9 行、docs/rip3/batches/P5-7-BATCH-REPORT.md G 节（Evidence rp_server 端点与 UI 页遗留）。

工作内容：
1. Evidence rp_server 检索端点（list/get，鉴权沿用既有 require_auth + AUTH_ENABLED 强制先例，形状与错误格式沿既有 API 惯例）；
2. UI Evidence 页：证据列表 + 追溯到来源 Task / ToolRun / Trace / Artifact 的跳转（Phase 5 计划 §7 P5-7 完成标准「Evidence 可以从 UI 跳转至来源任务和工具运行记录」）；
3. Coverage 矩阵页接 coverage-show 数据：七维五态分计数与盲区可见，未测试项与失败项分开显示（P5-8 完成标准沿用）；
4. 暂停/恢复/失败重试入口：复用 P5-6 plan-pause/plan-resume 跨进程面与 rp_server 控制端点（形状三），终态不渲染编造操作；
5. web/tests/workspaces.test.ts 路由钉值同步（当前 16，新增页记得同步）。
收口：cd web && npm run build && npm run test 全过 + cargo 定向面 + 真实浏览器逐功能实测截图入库 docs/rip3/batches/browser-e2e/（先例 web/scripts/rip3_p1_browser_e2e.mjs 与 p5_current_ui_shots.mjs；测试栈 rp_server 7801 + vite 5173，RP_API_TARGET=http://127.0.0.1:7801，vite 监听 [::1]，curl 用 localhost；token 走 localStorage rip_auth_token；浏览器文件上传用 setInputFiles）+ A-J 报告 docs/rip3/batches/P5-9b-BATCH-REPORT.md + 显式 git add 文件清单 commit + push + gh run watch 至绿。
返回 BatchOutcome（batch="P5-9b Evidence 与 Coverage 页面"）。`;

const P5_9C_BRIEF = `实施 RIP3 P5-9 子批三（本地靶场实拍与端到端验收）。开工先读：docs/HANDOVER-P5-WIP.md、docs/rip3/batches/P5-9a-BATCH-REPORT.md 与 P5-9b-BATCH-REPORT.md、Phase 5 计划 §14（最终验收场景）与 §7 P5-9 完成标准、docs/rip3/batches/P5-7-BATCH-REPORT.md G1 与 docs/rip3/batches/P5-5-BATCH-REPORT.md G1。

工作内容：
1. finding-link 正例 RTIS 写面实拍（P5-7 G1 遗留位）：本地靶场 = 本机 loopback 自建受控易受测面（只监听 127.0.0.1/localhost，低影响，不用 sudo，宿主机零安装，靶场脚本属批内产物落仓库）；跑出真实 CONFIRMED 后优先 rt.py poc add 新条目承载本次发现再 finding-link 关联（零触碰既有 POC 条目的 verified 状态）；若确需关联既有条目，先 rt.py poc get 核对漏洞类型真实对应并在报告写明依据；POC 正文零复制；调用只走 rt.py CLI。
2. egress_allowlist 派发面真实目标实拍（P5-5 G1 遗留）：本地靶场目标走 EgressAllowlist 链路（pasta -T + 代理 env + host 派生）实拍一次并留证。
3. 端到端浏览器验收：扩展 web/scripts/rip3_p5_browser_e2e.mjs 断言模板覆盖 P5-9 新页面与链路，截图入库 docs/rip3/batches/browser-e2e/。
4. Phase 5 计划 §14 六场景逐条演练留证据（A 从 Project 建研究 / B 知识生成验证计划 / C 本地靶场执行 / D 重复执行与缓存 / E 无命中补充研究 / F RTIS 与证据闭环），落 docs/rip3/batches/P5-9c-BATCH-REPORT.md，§7 P5-9 完成标准七条逐条对账（Project 建 ResearchTask → 知识检索生成计划 → 本地靶场验证 → Evidence 形成 → Finding 走 RTIS 接口 → Coverage 实际变化 → Workflow 可恢复且不重复运行已完成节点）。
收口：定向测试 + 显式 git add 文件清单 commit + push + gh run watch 至绿。
返回 BatchOutcome（batch="P5-9c 实拍与端到端验收"）。`;

/** 编排面门禁实录摘要（gateFixLoop 全过后传入 RELEASE brief；文本静态，事实由门禁 exit 0 背书）。 */
const gateSummary = [
  "cargo test --offline --workspace exit 0",
  "cargo clippy --offline --workspace --all-targets -- -D warnings exit 0",
  "python3 -m unittest discover -s workers/python/tests exit 0",
  "npm --prefix web run build exit 0",
  "npm --prefix web run test exit 0",
  "selftest_phase*.sh + selftest_auth.sh 全集 exit 0",
  "integrations/rtis 零改动检查 exit 0",
].join("\n");

const RELEASE_BRIEF = `收口 RIP3 Phase 5 RELEASE。开工先读：docs/HANDOVER-P5-WIP.md、Phase 5 计划 §17（完成定义）与 §14、docs/HANDOVER-RIP3-CONTINUATION.md。编排面已完成全量门禁静场串行复测与 CI 确认（结果见下），你不再自行复跑全量门禁。

工作内容：
1. docs/releases/RELEASE_2026-11-17_RIP3-PHASE5.md：Phase 5 计划 §17 完成定义逐条带证据（引用批次报告路径与实拍输出）+ §14 六场景对账（引用 P5-9c 演练证据）+ 遗留如实转登记（ZAP Adapter / 多模板批量与标签过滤 / P5-6 G1 G2 / egress 与 finding-link 实拍结论）+ RTIS 对账零回归声明（15155 口径 + integrations/rtis 零 diff，编排面门禁含此检查 exit 0）；
2. HANDOVER 勾销：docs/HANDOVER-RIP3-CONTINUATION.md 状态行与第三节 Phase 5 行更新为收口；docs/HANDOVER-P5-WIP.md 顶部标注收档（Phase 5 RELEASE 路径与最终 CI run）；
3. docs/README.md 的 Phase 5 行不动（该文件当前有用户未提交 WSW 改动，不入本批；在报告登记跳过原因）。
收口：显式 git add（RELEASE 报告 + 两份 HANDOVER）→ commit 中文长描述 → git push origin main（SSH 断连重试 2 到 3 次）→ gh run watch 至绿。
返回 BatchOutcome（batch="Phase 5 RELEASE"）。

编排面门禁实录（七项全过 exit 0）：
${gateSummary}`;

artifact.board("phases", {
  title: "RIP3 Phase 5 收官看板",
  key: "phase",
  status: "state",
  columns: ["实施中", "CI 确认", "已收口", "受阻"],
  cardTitle: "phase",
  detail: [{ field: "note", label: "备注" }],
});

const results: BatchOutcome[] = [];
const findings: Finding[] = [];

async function finishNow(done: BatchOutcome[], allFindings: Finding[], note: string): Promise<WorkflowReport> {
  const md: string[] = [
    "# RIP3 Phase 5 收官执行报告（中止）",
    "",
    `中止原因：${note}`,
    "",
    "## 已收口子批",
    "",
    "| 子批 | 批次报告 | 提交 |",
    "|---|---|---|---|",
    ...done.map((r) => `| ${r.batch} | ${r.batches.join(" / ")} | ${r.commits.join(" ")} |`),
    "",
    "继续方式：处置受阻原因后 ResumeWorkflowRun，或按 docs/HANDOVER-P5-WIP.md 第六节手动推进。",
  ];
  await artifact.markdown("report", md.join("\n"), {
    title: "RIP3 Phase 5 收官执行报告（中止）",
    description: note,
    primary: true,
  });
  return {
    conclusion: `Phase 5 收官中止：${note}。已收口子批 ${done.length} 个（明细见报告产物与 docs/rip3/batches/）。`,
    findings: allFindings,
    verified: [
      "已收口子批各自完成批内定向测试、A-J 批次报告、显式 add commit + push + CI watch（实施工程师，自动化批次入库纪律）",
      "已收口子批 HEAD 的 GitHub CI 经编排面 gh run watch --exit-status 确认全绿",
    ],
    notCovered: [
      "未开始的子批（见结论与 docs/HANDOVER-P5-WIP.md 第四、六节）",
      "WSW-0 与 Web Security Workbench 建设（Phase 5 收口后的下一阶段）",
      "实战校验三面（用户门控，独立待办，保持暂缓）",
    ],
  };
}

phase("环境预检与基线核对");
const preCmd =
  'git status -sb | head -8; git log --oneline -1; git rev-list --left-right --count origin/main...HEAD; mkdir -p /tmp/podman-shim && printf \'#!/bin/sh\\nexec distrobox-host-exec podman "$@"\\n\' > /tmp/podman-shim/podman && chmod +x /tmp/podman-shim/podman && PATH="/tmp/podman-shim:$PATH" podman --version; gh run list --limit 1 >/dev/null 2>&1; echo GH=$?; echo PRE_DONE';
const pre1 = await repo(preCmd);
log(`预检输出：\n${pre1.stdout.trim().slice(0, 800)}`);
const preOk = pre1.exitCode === 0 && /PRE_DONE/.test(pre1.stdout) && /podman version/.test(pre1.stdout);
if (!preOk) {
  report({ phase: "环境预检", state: "受阻", note: "git / podman shim / gh 基线未就绪" }, "phases");
  await hole<void>(
    "环境预检受阻处置",
    `环境预检未过（exit=${pre1.exitCode}）：\n${(pre1.stdout + "\n" + pre1.stderr).slice(0, 1000)}\n预检面：git 工作区可达（4 项 WSW 素材改动属预期不处理）、podman shim（/tmp/podman-shim 经 distrobox-host-exec 调宿主）、gh 可达（间歇 EOF 可重试）。请修复环境后返回；返回即视为环境就绪。`,
  );
  const pre2 = await repo(preCmd);
  if (!(pre2.exitCode === 0 && /PRE_DONE/.test(pre2.stdout))) {
    return await finishNow(results, findings, "环境预检处置后仍未就绪");
  }
}
report({ phase: "环境预检", state: "已收口", note: "git / podman shim / gh 就绪" }, "phases");

phase("P5-8 收尾实施");
report({ phase: "P5-8 收尾", state: "实施中", note: "" }, "phases");
const implA = agent("P5-8 收尾实施工程师", IMPL_PERSONA);
const p58 = await implA.ask<BatchOutcome>(P5_8_BRIEF);
log(`P5-8 收尾返回：${p58.commits.length} 个提交，报告=${p58.batches.join("/")}，CI=${p58.ciRunId}`);

phase("P5-8 收口 CI 确认");
report({ phase: "P5-8 收尾", state: "CI 确认", note: `HEAD ${p58.head}` }, "phases");
const gA = await ciEnsure(implA, "P5-8 收尾");
findings.push(...gA.incidents);
if (gA.block !== null) {
  report({ phase: "P5-8 收尾", state: "受阻", note: gA.block }, "phases");
  await hole<void>(
    "P5-8 收口受阻处置",
    `P5-8 收尾 CI 确认受阻：${gA.block}\n请处置（修复 / 显式豁免并写入批次报告）；返回即视为该子批可继续推进。`,
  );
  const gAre = await ciEnsure(implA, "P5-8 收尾");
  findings.push(...gAre.incidents);
  if (gAre.block !== null) {
    return await finishNow(results, findings, `P5-8 处置后 CI 仍未绿：${gAre.block}`);
  }
}
results.push(p58);
report({ phase: "P5-8 收尾", state: "已收口", note: `HEAD ${p58.head}，CI ${p58.ciRunId}` }, "phases");

phase("P5-9 批一实施：Workflow 模板与画布");
report({ phase: "P5-9 批一", state: "实施中", note: "" }, "phases");
const implB1 = agent("P5-9 批一实施工程师", IMPL_PERSONA);
const p59a = await implB1.ask<BatchOutcome>(P5_9A_BRIEF);
log(`P5-9a 返回：${p59a.commits.length} 个提交，报告=${p59a.batches.join("/")}`);

phase("P5-9 批一 CI 确认");
report({ phase: "P5-9 批一", state: "CI 确认", note: `HEAD ${p59a.head}` }, "phases");
const gB1 = await ciEnsure(implB1, "P5-9 批一");
findings.push(...gB1.incidents);
if (gB1.block !== null) {
  report({ phase: "P5-9 批一", state: "受阻", note: gB1.block }, "phases");
  await hole<void>(
    "P5-9 批一受阻处置",
    `P5-9a Workflow 模板与画布 CI 确认受阻：${gB1.block}\n请处置（修复 / 显式豁免并写入批次报告）；返回即视为该子批可继续推进。`,
  );
  const gB1re = await ciEnsure(implB1, "P5-9 批一");
  findings.push(...gB1re.incidents);
  if (gB1re.block !== null) {
    return await finishNow(results, findings, `P5-9a 处置后 CI 仍未绿：${gB1re.block}`);
  }
}
results.push(p59a);
report({ phase: "P5-9 批一", state: "已收口", note: `HEAD ${p59a.head}` }, "phases");

phase("P5-9 批二实施：Evidence 与 Coverage 页面");
report({ phase: "P5-9 批二", state: "实施中", note: "" }, "phases");
const implB2 = agent("P5-9 批二实施工程师", IMPL_PERSONA);
const p59b = await implB2.ask<BatchOutcome>(P5_9B_BRIEF);
log(`P5-9b 返回：${p59b.commits.length} 个提交，报告=${p59b.batches.join("/")}`);

phase("P5-9 批二 CI 确认");
report({ phase: "P5-9 批二", state: "CI 确认", note: `HEAD ${p59b.head}` }, "phases");
const gB2 = await ciEnsure(implB2, "P5-9 批二");
findings.push(...gB2.incidents);
if (gB2.block !== null) {
  report({ phase: "P5-9 批二", state: "受阻", note: gB2.block }, "phases");
  await hole<void>(
    "P5-9 批二受阻处置",
    `P5-9b Evidence 与 Coverage 页面 CI 确认受阻：${gB2.block}\n请处置（修复 / 显式豁免并写入批次报告）；返回即视为该子批可继续推进。`,
  );
  const gB2re = await ciEnsure(implB2, "P5-9 批二");
  findings.push(...gB2re.incidents);
  if (gB2re.block !== null) {
    return await finishNow(results, findings, `P5-9b 处置后 CI 仍未绿：${gB2re.block}`);
  }
}
results.push(p59b);
report({ phase: "P5-9 批二", state: "已收口", note: `HEAD ${p59b.head}` }, "phases");

phase("P5-9 批三实施：本地靶场实拍与端到端验收");
report({ phase: "P5-9 批三", state: "实施中", note: "" }, "phases");
const implB3 = agent("P5-9 批三实施工程师", IMPL_PERSONA);
const p59c = await implB3.ask<BatchOutcome>(P5_9C_BRIEF);
log(`P5-9c 返回：${p59c.commits.length} 个提交，报告=${p59c.batches.join("/")}`);

phase("P5-9 批三 CI 确认");
report({ phase: "P5-9 批三", state: "CI 确认", note: `HEAD ${p59c.head}` }, "phases");
const gB3 = await ciEnsure(implB3, "P5-9 批三");
findings.push(...gB3.incidents);
if (gB3.block !== null) {
  report({ phase: "P5-9 批三", state: "受阻", note: gB3.block }, "phases");
  await hole<void>(
    "P5-9 批三受阻处置",
    `P5-9c 实拍与端到端验收 CI 确认受阻：${gB3.block}\n请处置（修复 / 显式豁免并写入批次报告）；返回即视为该子批可继续推进。`,
  );
  const gB3re = await ciEnsure(implB3, "P5-9 批三");
  findings.push(...gB3re.incidents);
  if (gB3re.block !== null) {
    return await finishNow(results, findings, `P5-9c 处置后 CI 仍未绿：${gB3re.block}`);
  }
}
results.push(p59c);
report({ phase: "P5-9 批三", state: "已收口", note: `HEAD ${p59c.head}` }, "phases");

phase("Phase 5 收口全量门禁复测");
const implC = agent("Phase 5 收口工程师", IMPL_PERSONA);
const gAll = await gateFixLoop(implC, "Phase 5 收口");
findings.push(...gAll.incidents);
if (gAll.block !== null) {
  report({ phase: "收口门禁", state: "受阻", note: gAll.block }, "phases");
  await hole<void>(
    "收口门禁受阻处置",
    `Phase 5 收口全量门禁三轮未过：${gAll.block}\n请处置（修复 / 显式豁免并写入 RELEASE 报告遗留节）；返回即视为可继续 RELEASE。`,
  );
  const gAllre = await gateFixLoop(implC, "Phase 5 收口");
  findings.push(...gAllre.incidents);
  if (gAllre.block !== null) {
    return await finishNow(results, findings, `处置后全量门禁仍未过：${gAllre.block}`);
  }
}
report({ phase: "收口门禁", state: "已收口", note: "七项门禁静场串行全过（含 RTIS 零改动检查）" }, "phases");

phase("Phase 5 RELEASE 报告与 HANDOVER 勾销");
report({ phase: "Phase 5 RELEASE", state: "实施中", note: "" }, "phases");
const rel = await implC.ask<BatchOutcome>(RELEASE_BRIEF);
log(`RELEASE 返回：${rel.commits.length} 个提交，报告=${rel.batches.join("/")}`);

phase("Phase 5 RELEASE CI 确认");
const gRel = await ciEnsure(implC, "Phase 5 RELEASE");
findings.push(...gRel.incidents);
if (gRel.block !== null) {
  report({ phase: "Phase 5 RELEASE", state: "受阻", note: gRel.block }, "phases");
  await hole<void>(
    "RELEASE 收口受阻处置",
    `Phase 5 RELEASE CI 确认受阻：${gRel.block}\n请处置（修复 / 显式豁免并写入 RELEASE 报告）；返回即视为可继续收口确认。`,
  );
  const gRelre = await ciEnsure(implC, "Phase 5 RELEASE");
  findings.push(...gRelre.incidents);
  if (gRelre.block !== null) {
    return await finishNow(results, findings, `RELEASE 处置后 CI 仍未绿：${gRelre.block}`);
  }
}
results.push(rel);
report({ phase: "Phase 5 RELEASE", state: "已收口", note: `HEAD ${rel.head}，CI ${rel.ciRunId}` }, "phases");

const allLeftovers = results.flatMap((r) => r.leftovers.map((l) => `${r.batch}：${l}`));
const md: string[] = [
  "# RIP3 Phase 5 收官执行报告",
  "",
  `P5-8 收尾、P5-9 三子批与 Phase 5 RELEASE 全部收口（${results.length} 个子批）。每子批自动化入库（显式 add + commit + push + CI watch），收口经编排面 CI 确认与全量门禁静场串行复测（七项含 RTIS 零改动检查，exit 0）。`,
  "",
  "| 子批 | 批次报告 | 提交 | HEAD |",
  "|---|---|---|---|",
  ...results.map((r) => `| ${r.batch} | ${r.batches.join(" / ")} | ${r.commits.join(" ")} | ${r.head} |`),
  "",
  "## 遗留台账",
  "",
  ...(allLeftovers.length > 0 ? allLeftovers.map((l) => `- ${l}`) : ["- 无（各批次报告 G 节如实登记）"]),
  "",
  "## 下一阶段",
  "",
  "- WSW-0 只读审计与技术 POC（docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md；工作树 4 项 WSW 素材届时由用户决定入库）",
  "- 实战校验三面（用户门控，独立待办，保持暂缓）",
];
await artifact.markdown("report", md.join("\n"), {
  title: "RIP3 Phase 5 收官执行报告",
  description: "P5-8 收尾到 RELEASE 逐子批实施、门禁与收口账目（明细见 docs/rip3/batches/ 与 docs/releases/）",
  primary: true,
});

return {
  conclusion: `Phase 5 剩余全部内容完成：P5-8 收尾（hypothesis 实拍链与集成测试补齐）、P5-9 三子批（十三步模板与画布、Evidence 与 Coverage 页面、本地靶场实拍与 §14 六场景验收）与 Phase 5 RELEASE（全量门禁七项静场串行全过 + RELEASE 报告 + HANDOVER 勾销）共 ${results.length} 个子批收口，HEAD CI 全绿；遗留如实入台账。`,
  findings,
  verified: [
    "每子批收口 HEAD CI 经编排面 gh run watch --exit-status 确认全绿（gh EOF 间歇重试兜底）",
    "收口全量门禁由编排面静场串行确定性复测：cargo test --offline --workspace / clippy --all-targets -D warnings / python unittest / web build / web test / selftest_phase*.sh + auth / integrations/rtis 零改动，exit 0 才放行",
    "每子批自动化入库（显式 git add 文件清单 + 中文长描述 commit + push 重试），批次报告 A-J 落 docs/rip3/batches/，RELEASE 落 docs/releases/",
  ],
  notCovered: [
    "WSW-0 与 Web Security Workbench 建设（Phase 5 收口后的下一阶段，按任务书第四阶段另批推进）",
    "浏览器实测截图与视觉门明细（批次报告与 browser-e2e 目录承载，编排面不重复复核）",
    "实战校验三面（用户门控，独立待办，保持暂缓）",
    "RTIS 侧清账（rtis.py 自测与 license/maintenance 轴，RTIS 侧独立批次）",
  ],
};
