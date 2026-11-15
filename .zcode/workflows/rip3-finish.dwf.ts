/* zcode-workflow
name: rip3-finish
description: "RIP3 收官：Phase 3 知识治理 → Phase 4 混合检索 → Phase 5 工作流与安全研究 → Phase 6 报告智能 → Phase 7 知识回流 → Phase 8 规模测试 → §17/D8 收官自查，逐阶段实施（批次自动入库 commit+push+CI 硬门槛）+ 编排面确定性门禁复测 + 受阻处置站。接手入口 docs/HANDOVER-RIP3-CONTINUATION.md 队列。"
whenToUse: "RIP3 Phase 2 已收口后驱动剩余全部阶段的自动化收官；仓库根 /media/emo/code/pensting/research-platform。"
scope: project
*/

interface PhaseOutcome {
  /** 阶段名（如 "Phase 3 知识治理"）。 */
  phase: string;
  /** 本阶段批次报告路径（docs/rip3/batches/…）。 */
  batches: string[];
  /** 阶段 RELEASE 路径（docs/releases/…）。 */
  release: string;
  /** 本阶段入库提交短哈希（按时间序）。 */
  commits: string[];
  /** 遗留问题（如实登记，不隐藏）。 */
  leftovers: string[];
  /** 阶段收口时 HEAD 短哈希。 */
  head: string;
}

interface Finding {
  /** 位置（阶段 / 文件路径）。 */
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
  /** 两三句话回答「剩下的全部内容完成了吗」。 */
  conclusion: string;
  findings: Finding[];
  /** 本运行实际执行过的校验（命令级）。 */
  verified: string[];
  /** 未覆盖面与原因。 */
  notCovered: string[];
}

interface PhaseCard {
  /** 阶段名。 */
  phase: string;
  /** 实施中 | 门禁复测 | 已收口 | 受阻。 */
  state: string;
  /** 备注（HEAD / 受阻原因）。 */
  note: string;
}

const REPO = "/media/emo/code/pensting/research-platform";

/** 仓库根确定性命令面（bash -lc 定位仓库根；argv 携带运行时值）。 */
function repo(cmd: string, timeoutMs?: number) {
  return world.run("bash", ["-lc", `cd ${REPO} && ${cmd}`], {
    timeoutMs: timeoutMs ?? 1_800_000,
    output: { tail: 16384 },
  });
}

/** 收口门禁全集（R2 §15.1 + 交接文档第四节第五条：五道门禁 + 各阶段 selftest 全集）。 */
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

/** HEAD 对应 CI run 的确定性确认：gh run watch --exit-status。 */
async function ciWatch(): Promise<string | null> {
  const head = await repo("git rev-parse HEAD");
  const sha = head.stdout.trim().split("\n")[0] ?? "";
  if (!/^[0-9a-f]{40}$/.test(sha)) {
    return `无法取得 HEAD（${head.stdout.slice(0, 80)}）`;
  }
  let rid = "";
  for (let attempt = 1; attempt <= 3; attempt++) {
    const find = await repo(
      `gh run list --branch main --limit 10 --json databaseId,headSha -q '.[] | select(.headSha=="${sha}") | .databaseId' | head -1`,
    );
    rid = find.stdout.trim();
    if (/^[0-9]+$/.test(rid)) break;
    await repo("sleep 15");
  }
  if (!/^[0-9]+$/.test(rid)) {
    return `HEAD ${sha.slice(0, 8)} 无对应 CI run（未 push 或 gh 不可达）`;
  }
  const watch = await repo(`gh run watch ${rid} --exit-status >/dev/null 2>&1; echo CIV=$?`, 1_500_000);
  const civ = /CIV=(\d+)/.exec(watch.stdout)?.[1];
  if (watch.exitCode === 0 && civ === "0") return null;
  return `CI run ${rid} 未绿（watch exit=${watch.exitCode} CIV=${civ ?? "?"}）`;
}

/** 一轮收口复测：五道门禁全集（修复回路 3 轮，实施工程师修复）+ HEAD CI 确认（一轮修复重验）。 */
async function gateAndCi(impl: Agent, label: string): Promise<GateResult> {
  const incidents: Finding[] = [];
  let failures: string[] = [];
  for (let round = 1; round <= 3; round++) {
    failures = await runGateSet();
    if (failures.length === 0) break;
    log(`${label} 门禁第 ${round}/3 轮未过（${failures.length} 项），交实施工程师修复`);
    await impl.ask<string>(
      `收口门禁复测失败（编排面复测，第 ${round}/3 轮）。失败项与输出尾部：\n${failures.join("\n\n")}\n修复并以独立修复批 commit + push（不要自行复跑全量门禁，编排面统一复测）。`,
    );
    incidents.push({
      where: `${label} 收口门禁`,
      what: `第 ${round}/3 轮门禁失败（${failures.length} 项）后交实施工程师修复`,
      evidence: failures[0],
      status: "verified",
      severity: "medium",
    });
  }
  if (failures.length > 0) {
    return { block: `门禁三轮未过，首项：${failures[0]}`, incidents };
  }
  const ci1 = await ciWatch();
  if (ci1 === null) return { block: null, incidents };
  log(`${label} CI 未绿（${ci1}），一轮修复后重验`);
  await impl.ask<string>(
    `push 后 CI 红：${ci1}\n用 gh run view --log-failed 定位失败 job 与用例，修复并以独立修复批 commit + push。`,
  );
  incidents.push({
    where: `${label} CI 硬门槛`,
    what: "HEAD CI 未绿后实施工程师修复重推",
    evidence: ci1,
    status: "verified",
    severity: "medium",
  });
  const ci2 = await ciWatch();
  if (ci2 === null) return { block: null, incidents };
  return { block: `CI 两轮未绿：${ci2}`, incidents };
}

const IMPL_PERSONA = `你是 RIP3 平台架构升级的资深实施工程师，独立负责一个阶段的完整交付与自动入库。仓库根 = ${REPO}；会话工作区在仓库外，所有文件工具与 shell 命令一律用绝对路径定位仓库内文件；开工先读仓库根 AGENTS.md 与 docs/HANDOVER-RIP3-CONTINUATION.md。铁律：继承优先（trace 六 ID、Tool Contract 信封、workflow-engine、Context Broker、podman 执行面、audit correlation、source_sync、RTIS 接口八项复用不重建）；RTIS 红线（integrations/rtis 零修改，15155 对账零回归，漏洞细节不进 3.0 库与公开仓库）；新增域永不落 SQLite（情报核新表走 migrations_pg V 链，refinery 每文件事务 + checksum，测试副本先验证）；SQLite 020+ 仅运营面小迁移；成熟库优先（Rust 新依赖增量 vendor：移开 .cargo/config.toml 联网 cargo fetch 后手工增量 vendor，清理新目录嵌套 .gitignore 并同步 checksum 三方一致，否则 CI 校验必炸）；宿主机零系统级安装（Python 重依赖只进容器镜像，npm 只进 web/，不用 sudo）；中文文档禁 em/en dash；生产 migrations/platform*.db 零触碰；不 reset/clean/checkout 改写历史；环境事实：podman 直连宿主、rip-pg17 容器（RP_PG_DSN 缺省 127.0.0.1:5432/postgres）、端口 7800 被常驻服务占用（测试 rp_server 用 7801+）、pypi/hf-mirror/pytorch-cpu/debian 源可达、镜像 digest 变更即版本变更并三处同步台账（manifest yaml/json + containerfiles/README.md）、迁移链推进时同步 apps/server/selftest_phase1_pg.sh 钉值（先例 V0003 2→3）。工作树若出现非本批改动，先 git status 核对并如实上报。批次纪律：每批按 R2 §17.6 A-H 出批次报告落 docs/rip3/batches/，批内 commit（中文长描述，仓库惯例）+ push（SSH 断连重试 2 到 3 次，失败走 HTTPS+gh token）+ gh run watch 至绿（CI 红不进下一批，同一失败两次无新信息即 escalate）。最终收口五道门禁与 CI 确认由编排面统一复测——你只跑开发期定向测试与每批 push 后的 CI watch，不要在收口时重复全量门禁。同一失败方法两次无新信息就换本质不同的方法。若门禁不可能通过、验收互相矛盾或遇到只有用户能定的事，escalate 并说明，不要绕过。`;

const P3_BRIEF = `交付 RIP3 Phase 3（知识治理）。

按序读（仓库内绝对路径）：docs/HANDOVER-RIP3-CONTINUATION.md（第三节队列第 2 行、第四节贯穿纪律、第五节环境事实）、docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md（第五部分知识治理 + 第十四部分 Phase 3 行 + §17.6 批次格式）、docs/rip3/03-DOMAIN-MODEL.md（1.4 到 1.8 实体）、docs/rip3/decisions/ 全部 ADR、docs/releases/RELEASE_2026-11-15_RIP3-PHASE2.md（当前基线与先例）。

验收基准（队列原文）：D1 sha256 去重；D2 规范化哈希；D3 datasketch MinHash+LSH worker（容器内，宿主机零安装）；D4 语义候选（pgvector，禁自动合并）；D5 CONTRADICTS 进审核；KnowledgeRelation 10 型；质量分六权重与七类风险标记；审核工作流（复用 018 控制面与 Human 节点）；RTIS 对账零回归（15155 口径）。

实施要点：情报核新表走 migrations_pg（V0004 起）；向量面按 03 号冻结物 1.7（knowledge_chunk_vectors + VectorBackend trait）；审核状态机复用 workflow-engine 018 Human 面；datasketch 只进容器镜像（pip 受控拉取，运行期 network=none）。返回 PhaseOutcome（phase="Phase 3 知识治理"，batches/release/commits/leftovers/head 全如实填写）。`;

const P4_BRIEF = `交付 RIP3 Phase 4（混合检索）。先读 docs/HANDOVER-RIP3-CONTINUATION.md（队列第 3 行）与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第六部分、docs/rip3/00-ARCHITECTURE-AUDIT.md 第一节 1.1（向量未接入实证）与 docs/rip3/12-STORAGE-AND-MIGRATION.md。

验收基准（队列原文）：统一 Search API（PG 面：pg_search BM25 + pgvector + JSONB 过滤）；RRF/MMR 融合在 Rust 检索层（跨 SQLite 知识面与 PG 文档面）；reranker worker（bge-reranker ONNX）；tokenizers 接入替换 est_tokens；缓存升级；Benchmark（RTIS 语料播种，Recall@K/MRR/NDCG）。

实施要点：pg_search BM25 索引建演练随本批（P1 遗留 2 清账）；CI PG service job 评估随本批（P1 遗留 5）；RTIS 语料播种只走 rt.py 只读面（零直写）；权限前置过滤谓词在 SQL 内（13 号冻结物第五节）；bge 模型只进容器镜像（离线推理，D7 本地优先）。返回 PhaseOutcome（phase="Phase 4 混合检索"）。`;

const P5_BRIEF = `交付 RIP3 Phase 5（工作流与安全研究）。先读 docs/HANDOVER-RIP3-CONTINUATION.md（队列第 4 行）与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第七部分。

验收基准（队列原文）：workflow 模板机制（现无模板能力，审计实证）；渗透/SRC 十三步模板；自适应研究引擎（覆盖矩阵七维 / 假设生成 / 五态纪律 CONFIRMED/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE/FALSE_POSITIVE，禁止把未测试或未复现描述为没有漏洞）；画布增强；Findings 经 RTIS 接口落库。

实施要点：授权确认节点不内置（2026-11-14 用户裁决，R2 7.3）；模板实例化测试；UI 批次必须真实浏览器逐功能实测并截图入库 docs/rip3/batches/browser-e2e/（playwright 先例 web/scripts/rip3_p1_browser_e2e.mjs，浏览器文件上传用 setInputFiles）；Findings 落 RTIS 走 rt.py CLI / rtis MCP 面（零直写）；五态纪律为硬性。返回 PhaseOutcome（phase="Phase 5 工作流与安全研究"）。`;

const P6_BRIEF = `交付 RIP3 Phase 6（报告智能）。先读 docs/HANDOVER-RIP3-CONTINUATION.md（队列第 5 行）与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第八部分。

验收基准（队列原文）：Report Domain/IR（事实引用 RTIS）；typst PDF 渲染（字体烘焙离线）；DOCX/HTML/MD/JSON 渲染器（统一 Report IR 单源，禁止每种格式重新调用 AI 生成）；Diagram Engine（Mermaid + typst cetz/fletcher）；通俗+技术双解释（七个问题）；导出质检。

实施要点：typst 0.13 选型已登记 ADR-0002；typst 与字体只进容器镜像（离线渲染）；报告事实数据引用 RTIS（零复制漏洞细节进 3.0 库与公开仓库）；UI 批次浏览器实测截图入库 docs/rip3/batches/browser-e2e/。返回 PhaseOutcome（phase="Phase 6 报告智能"）。`;

const P7_BRIEF = `交付 RIP3 Phase 7（知识回流）。先读 docs/HANDOVER-RIP3-CONTINUATION.md（队列第 6 行）与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第十四部分 Phase 7 行。

验收基准（队列原文）：Finding 到候选 KnowledgeCard 流水线（复用 k_proposal 模式）；Evidence 溯源（六 ID）；上游更新监测（复用 source_sync）。

实施要点：k_proposal 语义对齐 RTIS k_proposal 表（from_run/type/title/claim/sources_json/evidence_level/status）；回流候选进审核而非直接入库；上游监测泛化 source_sync（Git/GitHub/URL 通道 + commit 钉死 + checksum）不重建；Evidence 六 ID 关联复用 015 面。返回 PhaseOutcome（phase="Phase 7 知识回流"）。`;

const P8_BRIEF = `交付 RIP3 Phase 8（规模测试）。先读 docs/HANDOVER-RIP3-CONTINUATION.md（队列第 7 行）与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第十五部分 15.4。

验收基准（队列原文）：1k/10k/100k 文档 + 百万 chunk 合成语料；导入/索引/检索/内存/存储/并发/迁移/重建/备份基准报告；决定运营核是否移植 PG。

实施要点：合成语料生成脚本属批内产物，语料本体为 gitignore 运行时数据（rip-data/ 规格内）；基准数字落 docs/rip3/batches/ 批次报告（数字带实测来源）；运营核移植出数据支撑的裁决建议，最终拍板留用户——在批次报告与 leftovers 注明「待用户确认」；对照 autoresearch 计量先例（docs/phase10/PERF-BASELINE.md）。返回 PhaseOutcome（phase="Phase 8 规模测试"）。`;

const CLOSE_BRIEF = `收官自查（RIP3 全部 Phase 后）。先读 docs/HANDOVER-RIP3-CONTINUATION.md 第八节与 docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md 第十七、十八部分。

交付：R2 §17.1-17.6 执行规则与第十八部分最终验收场景 A-E 的全绿自查（先例 = 2.0 的 docs/phase10/PLAN-17-SELFCHECK.md；落 docs/rip3/PLAN-17-SELFCHECK.md，判据逐条带命令与回显；有红项修复后循环到全绿）；D8 版本命名决策评估输入（2.x Phase 11+ 或 3.0 的兼容性评估，落 docs/rip3/D8-VERSION-ASSESSMENT.md，决策本身留用户，leftovers 注明「待用户确认」）；完成自查收口批次（commit+push+CI）。

返回 PhaseOutcome（phase="收官自查与版本评估"）。`;

artifact.board("phases", {
  title: "RIP3 收官阶段看板",
  key: "phase",
  status: "state",
  columns: ["实施中", "门禁复测", "已收口", "受阻"],
  cardTitle: "phase",
  detail: [{ field: "note", label: "备注" }],
});

const results: PhaseOutcome[] = [];
const findings: Finding[] = [];

async function finishNow(resultsSoFar: PhaseOutcome[], allFindings: Finding[], note: string): Promise<WorkflowReport> {
  const md: string[] = [
    "# RIP3 收官执行报告（中止）",
    "",
    `中止原因：${note}`,
    "",
    "## 已收口阶段",
    "",
    "| 阶段 | 批次报告 | RELEASE | 提交 |",
    "|---|---|---|---|",
    ...resultsSoFar.map(
      (r) => `| ${r.phase} | ${r.batches.length} 份 | ${r.release} | ${r.commits.join(" ")} |`,
    ),
    "",
    "继续方式：处置受阻原因后 ResumeWorkflowRun 或按 HANDOVER 第四节手动推进。",
  ];
  await artifact.markdown("report", md.join("\n"), {
    title: "RIP3 收官执行报告（中止）",
    description: note,
    primary: true,
  });
  return {
    conclusion: `收官中止：${note}。已收口阶段 ${resultsSoFar.length} 个（明细见报告产物与 docs/rip3/batches/）。`,
    findings: allFindings,
    verified: [
      "已收口阶段各自完成批次级 commit+push+CI watch（实施工程师，自动化批次入库纪律）",
      "已收口阶段各经编排面五道门禁全集确定性复测（cargo/clippy/unittest/web/selftest 全集，exit 0）",
      "已收口阶段 HEAD 的 GitHub CI 以 gh run watch --exit-status 确认四 job 全绿",
    ],
    notCovered: [
      "未开始的阶段（见结论与 HANDOVER 第三节队列）",
      "实战校验三面（用户门控，独立待办，保持暂缓）",
      "RTIS 侧清账（rtis.py 自测与 license/maintenance 轴，RTIS 侧独立批次）",
    ],
  };
}

phase("环境预检与基线核对");
const preCmd =
  "git status -sb | head -5; git log --oneline -1; podman --version; podman start rip-pg17 >/dev/null 2>&1; podman exec rip-pg17 pg_isready -U postgres; echo PRE=$?";
const pre1 = await repo(preCmd);
log(`预检输出：\n${pre1.stdout.trim().slice(0, 600)}`);
if (pre1.exitCode !== 0 || !/PRE=0/.test(pre1.stdout)) {
  report({ phase: "环境预检", state: "受阻", note: "git/podman/PG 基线未就绪" }, "phases");
  await hole<void>(
    "环境预检受阻处置",
    `环境预检未过（exit=${pre1.exitCode}）：\n${(pre1.stdout + "\n" + pre1.stderr).slice(0, 800)}\n请修复环境（git 状态 / podman / rip-pg17 容器）后返回；返回即视为环境就绪。`,
  );
  const pre2 = await repo(preCmd);
  if (pre2.exitCode !== 0 || !/PRE=0/.test(pre2.stdout)) {
    return await finishNow([], findings, "环境预检处置后仍未就绪");
  }
}
report({ phase: "环境预检", state: "已收口", note: "git/podman/rip-pg17 就绪" }, "phases");

phase("Phase 3 知识治理实施");
report({ phase: "Phase 3 知识治理", state: "实施中", note: "" }, "phases");
const impl3 = agent("Phase 3 实施工程师", IMPL_PERSONA);
const p3 = await impl3.ask<PhaseOutcome>(P3_BRIEF);
log(`Phase 3 实施返回：${p3.commits.length} 个提交，release=${p3.release}`);

phase("Phase 3 收口门禁复测");
const base3 = await repo("git rev-parse --short HEAD");
log(`Phase 3 门禁基线：HEAD ${base3.stdout.trim()}`);
report({ phase: "Phase 3 知识治理", state: "门禁复测", note: `HEAD ${p3.head}` }, "phases");
const g3 = await gateAndCi(impl3, "Phase 3");
findings.push(...g3.incidents);
if (g3.block !== null) {
  report({ phase: "Phase 3 知识治理", state: "受阻", note: g3.block }, "phases");
  await hole<void>(
    "Phase 3 受阻处置",
    `Phase 3 知识治理 收口受阻：${g3.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g3re = await gateAndCi(impl3, "Phase 3");
  findings.push(...g3re.incidents);
  if (g3re.block !== null) {
    return await finishNow(results, findings, `Phase 3 处置后门禁仍未过：${g3re.block}`);
  }
}
results.push(p3);
report({ phase: "Phase 3 知识治理", state: "已收口", note: `HEAD ${p3.head}` }, "phases");

phase("Phase 4 混合检索实施");
report({ phase: "Phase 4 混合检索", state: "实施中", note: "" }, "phases");
const impl4 = agent("Phase 4 实施工程师", IMPL_PERSONA);
const p4 = await impl4.ask<PhaseOutcome>(P4_BRIEF);
log(`Phase 4 实施返回：${p4.commits.length} 个提交，release=${p4.release}`);

phase("Phase 4 收口门禁复测");
const base4 = await repo("git rev-parse --short HEAD");
log(`Phase 4 门禁基线：HEAD ${base4.stdout.trim()}`);
report({ phase: "Phase 4 混合检索", state: "门禁复测", note: `HEAD ${p4.head}` }, "phases");
const g4 = await gateAndCi(impl4, "Phase 4");
findings.push(...g4.incidents);
if (g4.block !== null) {
  report({ phase: "Phase 4 混合检索", state: "受阻", note: g4.block }, "phases");
  await hole<void>(
    "Phase 4 受阻处置",
    `Phase 4 混合检索 收口受阻：${g4.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g4re = await gateAndCi(impl4, "Phase 4");
  findings.push(...g4re.incidents);
  if (g4re.block !== null) {
    return await finishNow(results, findings, `Phase 4 处置后门禁仍未过：${g4re.block}`);
  }
}
results.push(p4);
report({ phase: "Phase 4 混合检索", state: "已收口", note: `HEAD ${p4.head}` }, "phases");

phase("Phase 5 工作流与安全研究实施");
report({ phase: "Phase 5 工作流与安全研究", state: "实施中", note: "" }, "phases");
const impl5 = agent("Phase 5 实施工程师", IMPL_PERSONA);
const p5 = await impl5.ask<PhaseOutcome>(P5_BRIEF);
log(`Phase 5 实施返回：${p5.commits.length} 个提交，release=${p5.release}`);

phase("Phase 5 收口门禁复测");
const base5 = await repo("git rev-parse --short HEAD");
log(`Phase 5 门禁基线：HEAD ${base5.stdout.trim()}`);
report({ phase: "Phase 5 工作流与安全研究", state: "门禁复测", note: `HEAD ${p5.head}` }, "phases");
const g5 = await gateAndCi(impl5, "Phase 5");
findings.push(...g5.incidents);
if (g5.block !== null) {
  report({ phase: "Phase 5 工作流与安全研究", state: "受阻", note: g5.block }, "phases");
  await hole<void>(
    "Phase 5 受阻处置",
    `Phase 5 工作流与安全研究 收口受阻：${g5.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g5re = await gateAndCi(impl5, "Phase 5");
  findings.push(...g5re.incidents);
  if (g5re.block !== null) {
    return await finishNow(results, findings, `Phase 5 处置后门禁仍未过：${g5re.block}`);
  }
}
results.push(p5);
report({ phase: "Phase 5 工作流与安全研究", state: "已收口", note: `HEAD ${p5.head}` }, "phases");

phase("Phase 6 报告智能实施");
report({ phase: "Phase 6 报告智能", state: "实施中", note: "" }, "phases");
const impl6 = agent("Phase 6 实施工程师", IMPL_PERSONA);
const p6 = await impl6.ask<PhaseOutcome>(P6_BRIEF);
log(`Phase 6 实施返回：${p6.commits.length} 个提交，release=${p6.release}`);

phase("Phase 6 收口门禁复测");
const base6 = await repo("git rev-parse --short HEAD");
log(`Phase 6 门禁基线：HEAD ${base6.stdout.trim()}`);
report({ phase: "Phase 6 报告智能", state: "门禁复测", note: `HEAD ${p6.head}` }, "phases");
const g6 = await gateAndCi(impl6, "Phase 6");
findings.push(...g6.incidents);
if (g6.block !== null) {
  report({ phase: "Phase 6 报告智能", state: "受阻", note: g6.block }, "phases");
  await hole<void>(
    "Phase 6 受阻处置",
    `Phase 6 报告智能 收口受阻：${g6.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g6re = await gateAndCi(impl6, "Phase 6");
  findings.push(...g6re.incidents);
  if (g6re.block !== null) {
    return await finishNow(results, findings, `Phase 6 处置后门禁仍未过：${g6re.block}`);
  }
}
results.push(p6);
report({ phase: "Phase 6 报告智能", state: "已收口", note: `HEAD ${p6.head}` }, "phases");

phase("Phase 7 知识回流实施");
report({ phase: "Phase 7 知识回流", state: "实施中", note: "" }, "phases");
const impl7 = agent("Phase 7 实施工程师", IMPL_PERSONA);
const p7 = await impl7.ask<PhaseOutcome>(P7_BRIEF);
log(`Phase 7 实施返回：${p7.commits.length} 个提交，release=${p7.release}`);

phase("Phase 7 收口门禁复测");
const base7 = await repo("git rev-parse --short HEAD");
log(`Phase 7 门禁基线：HEAD ${base7.stdout.trim()}`);
report({ phase: "Phase 7 知识回流", state: "门禁复测", note: `HEAD ${p7.head}` }, "phases");
const g7 = await gateAndCi(impl7, "Phase 7");
findings.push(...g7.incidents);
if (g7.block !== null) {
  report({ phase: "Phase 7 知识回流", state: "受阻", note: g7.block }, "phases");
  await hole<void>(
    "Phase 7 受阻处置",
    `Phase 7 知识回流 收口受阻：${g7.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g7re = await gateAndCi(impl7, "Phase 7");
  findings.push(...g7re.incidents);
  if (g7re.block !== null) {
    return await finishNow(results, findings, `Phase 7 处置后门禁仍未过：${g7re.block}`);
  }
}
results.push(p7);
report({ phase: "Phase 7 知识回流", state: "已收口", note: `HEAD ${p7.head}` }, "phases");

phase("Phase 8 规模测试实施");
report({ phase: "Phase 8 规模测试", state: "实施中", note: "" }, "phases");
const impl8 = agent("Phase 8 实施工程师", IMPL_PERSONA);
const p8 = await impl8.ask<PhaseOutcome>(P8_BRIEF);
log(`Phase 8 实施返回：${p8.commits.length} 个提交，release=${p8.release}`);

phase("Phase 8 收口门禁复测");
const base8 = await repo("git rev-parse --short HEAD");
log(`Phase 8 门禁基线：HEAD ${base8.stdout.trim()}`);
report({ phase: "Phase 8 规模测试", state: "门禁复测", note: `HEAD ${p8.head}` }, "phases");
const g8 = await gateAndCi(impl8, "Phase 8");
findings.push(...g8.incidents);
if (g8.block !== null) {
  report({ phase: "Phase 8 规模测试", state: "受阻", note: g8.block }, "phases");
  await hole<void>(
    "Phase 8 受阻处置",
    `Phase 8 规模测试 收口受阻：${g8.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为该阶段可继续推进。`,
  );
  const g8re = await gateAndCi(impl8, "Phase 8");
  findings.push(...g8re.incidents);
  if (g8re.block !== null) {
    return await finishNow(results, findings, `Phase 8 处置后门禁仍未过：${g8re.block}`);
  }
}
results.push(p8);
report({ phase: "Phase 8 规模测试", state: "已收口", note: `HEAD ${p8.head}` }, "phases");

phase("收官自查与版本评估实施");
report({ phase: "收官自查与版本评估", state: "实施中", note: "" }, "phases");
const closer = agent("收官工程师", IMPL_PERSONA);
const pc = await closer.ask<PhaseOutcome>(CLOSE_BRIEF);
log(`收官返回：${pc.commits.length} 个提交`);

phase("收官门禁复测");
const basec = await repo("git rev-parse --short HEAD");
log(`收官门禁基线：HEAD ${basec.stdout.trim()}`);
report({ phase: "收官自查与版本评估", state: "门禁复测", note: `HEAD ${pc.head}` }, "phases");
const gc = await gateAndCi(closer, "收官");
findings.push(...gc.incidents);
if (gc.block !== null) {
  report({ phase: "收官自查与版本评估", state: "受阻", note: gc.block }, "phases");
  await hole<void>(
    "收官受阻处置",
    `收官自查收口受阻：${gc.block}\n请处置（修复 / 回滚 / 显式豁免并写入批次报告）；返回即视为收官可继续。`,
  );
  const gcre = await gateAndCi(closer, "收官");
  findings.push(...gcre.incidents);
  if (gcre.block !== null) {
    return await finishNow(results, findings, `收官处置后门禁仍未过：${gcre.block}`);
  }
}
results.push(pc);
report({ phase: "收官自查与版本评估", state: "已收口", note: `HEAD ${pc.head}` }, "phases");

const allLeftovers = results.flatMap((r) => r.leftovers.map((l) => `${r.phase}：${l}`));
const md: string[] = [
  "# RIP3 收官执行报告",
  "",
  `队列 Phase 3 到 8 与收官自查全部收口（${results.length} 个阶段）。每阶段批次自动入库（commit+push+CI watch），收口经编排面五道门禁全集确定性复测与 HEAD CI 确认。`,
  "",
  "| 阶段 | 批次报告 | RELEASE | 提交 |",
  "|---|---|---|---|",
  ...results.map(
    (r) => `| ${r.phase} | ${r.batches.length} 份 | ${r.release} | ${r.commits.join(" ")} |`,
  ),
  "",
  "## 遗留台账",
  "",
  ...(allLeftovers.length > 0 ? allLeftovers.map((l) => `- ${l}`) : ["- 无（各批次报告 G 节如实登记）"]),
  "",
  "## 用户待决项",
  "",
  "- D8 版本命名最终拍板（评估输入 docs/rip3/D8-VERSION-ASSESSMENT.md）",
  "- 运营核是否移植 PG 最终拍板（Phase 8 基准报告给出数据与建议）",
  "- 实战校验三面（用户门控，独立待办）",
];
await artifact.markdown("report", md.join("\n"), {
  title: "RIP3 收官执行报告",
  description: "Phase 3 到 8 逐阶段实施、门禁与收口账目（明细见 docs/rip3/batches/ 与 docs/releases/）",
  primary: true,
});

return {
  conclusion: `RIP3 剩余全部内容完成：Phase 3 知识治理、Phase 4 混合检索、Phase 5 工作流与安全研究、Phase 6 报告智能、Phase 7 知识回流、Phase 8 规模测试与收官自查全部收口，每阶段五道门禁与 CI 硬门槛全绿；用户待决项（D8 版本命名、运营核移植拍板、实战校验三面）已单列。`,
  findings,
  verified: [
    "每阶段收口五道门禁全集由编排面确定性复测（cargo test --offline --workspace / clippy -D warnings / unittest discover / web build / web test / selftest_phase*.sh + auth，exit 0 才放行）",
    "每阶段 HEAD 的 GitHub CI 以 gh run watch --exit-status 确认四 job 全绿",
    "每批自动化入库（commit+push），批次报告 R2 §17.6 A-H 落 docs/rip3/batches/，阶段 RELEASE 落 docs/releases/",
  ],
  notCovered: [
    "实战校验三面（用户门控，RIP3 计划外独立待办，保持暂缓）",
    "RTIS 侧清账（rtis.py 自测 10 过 7 败与 license/maintenance 轴，RTIS 侧独立批次）",
    "D8 版本命名与运营核移植的最终拍板（评估输入已产出，决策留用户）",
    "浏览器实测截图与视觉门明细（批次报告与 browser-e2e 目录承载，编排面不重复复核）",
  ],
};
