// RIP 2.0 跨机续作工作流（2026-11-13 免编辑版）
// 由 docs/handoff/workflow-script-RIP2-2026-11-12-RESUME.dwf.ts 再改写，适用场景：新机器上新会话重建（journal 不跨机，原运行 dwfrun-954913b0 已于 2026-11-13 被用户暂停且不可跨机 Resume）。
// 与上一版差异：
// 1. SK 扩展（SK-1 到 SK-3）已收口入库 commit 2bf8e9c，原实施段改为只读核对；
// 2. Phase 7 批次 1 到 4（五区壳层、agent-engine 最小执行闭环、013 三类 Memory、014 运行控制）半成品已在 WIP 备份 7effba9，改为核对后直接续作；
// 3. P7-5（trace 统一关联与跨栈查询）规格按被停运行规划员输出钉死（迁移编号改 015：014 已被 P7-4 agent_run_control 占用）；
// 4. 只规划 P7-6 与 P7-7 两批（P7-7 必须含 Agent Studio 页落新壳）；
// 5. 浏览器二进制口径机器无关化：实测前先探测本机可用浏览器，不装系统级依赖。
// 队列来源：docs/HANDOVER-RIP2-CONTINUATION.md 第二节与第三节，状态源 docs/SESSION-HANDOFF.md 第三节。

interface VerifyReport {
  /** 逐条核对结论。 */
  items: { criterion: string; pass: boolean; evidence: string }[];
  /** 全部核对通过。 */
  ok: boolean;
  /** 需要接手方注意的偏差、缺口或环境问题（无则空数组）。 */
  gaps: string[];
}

interface PhaseBatch {
  /** 批次编号，如 P7-5。 */
  id: string;
  /** 一句话目标。 */
  goal: string;
  /** 本批要新建或修改的具体文件路径。 */
  paths: string[];
  /** 本批完成即可自验的检查。 */
  check: string;
}

interface PhasePlan {
  batches: PhaseBatch[];
  /** 本阶段验收要点，逐条可核验。 */
  acceptance: string[];
  risk: string;
}

interface BatchResult {
  batch: string;
  done: string;
  files: string[];
  check: string;
  caveats: string[];
}

interface PhaseAcceptance {
  items: { criterion: string; pass: boolean; evidence: string }[];
  acceptable: boolean;
  fixes: string[];
}

interface CommitResult {
  hash: string;
  pushed: boolean;
  note: string;
}

interface FinalReport {
  /** 给用户的结论：推进到哪个阶段、当前状态、卡点。 */
  conclusion: string;
  /** 各阶段要点与状态。 */
  findings: { phase: string; what: string; status: string }[];
  /** 实际执行过的门禁命令与结果。 */
  verified: string[];
  /** 未覆盖或未能验证的项与原因。 */
  notCovered: string[];
}

artifact.board("phases", {
  title: "RIP 2.0 续作进度",
  key: "phase",
  status: "status",
  columns: ["待办", "进行中", "完成", "受阻"],
  detail: [{ field: "note", label: "说明" }],
});
const TRACKED = [
  "Phase 5 Web Workbench（基础版已入库）",
  "Phase 6 与 SK 扩展（已入库）",
  "Phase 7 Agent Engine",
  "Phase 8 Workflow Studio",
  "Phase 9 MCP/REST/CLI 对外能力",
  "Phase 10 生态与发布",
];
report({ phase: TRACKED[0], status: "完成", note: "commit c0a4284/6217ca8，验收通过" }, "phases");
report({ phase: TRACKED[1], status: "完成", note: "Phase 6 基础 49b9377/ace3c53 + SK 扩展 2bf8e9c，验收通过" }, "phases");

const DISCIPLINE = [
  "通用约束：",
  "1. RIP 工程根为 research-platform 仓库根（独立 git 仓库，origin 为 github.com/emoeem/research-intelligence-platform）。本运行的工作目录必须是仓库根：cargo 门禁从仓库根执行（vendor 配置按 CWD 发现，vendor/ 已入库可离线构建）。",
  "2. 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md 与 docs/phase0/PHASE-ROADMAP.md（冻结批次与验收命令）。",
  "3. 红线：不在宿主机安装任何系统级依赖（npm 包只进 web/ 项目目录，cargo vendor 已入库不需要网络）；不用 sudo；不修改生产 migrations/platform*.db 与 RTIS 生产行为；不推翻 Phase 0-6 已验收能力。",
  "4. 文档中文，全文禁止 em dash 与 en dash（长横线），用逗号、冒号、括号替代；文档规范见 docs/DOC-STANDARD.md。",
  "5. 结论必须来自实际读到的文件或实际执行的命令，未验证的明确标注「待验证」；无法完成时如实说明，不伪造通过。",
  "6. 复用优先（用户硬性规则）：通用逻辑一律先用成熟且仍在维护的开源库或仓库既有实现，不手写重复造轮子；需要新依赖先评估维护状态再引入并锁定版本；Rust 新依赖走增量 cargo vendor 保 --offline 构建。",
  "7. 浏览器二进制机器无关口径：实测前先探测本机可用浏览器（command -v chromium chromium-browser google-chrome-stable，或查 playwright/puppeteer 用户缓存），用 PW_CHROMIUM 或等价环境变量注入给视觉门禁；缺浏览器用用户级缓存方案（如 npx playwright install chromium，只写用户缓存目录），绝不装系统级依赖；参考实现见 git log 7effba9 的 P7-1 caveat（上一台机器用的是宿主 Chrome for Testing 153 缓存，本机大概率没有）。",
].join("\n");

phase("核对接手基线与新机环境");
const verified = await agent("入库核对员", `${DISCIPLINE}

你是只读核对员兼环境自检员：除 web/node_modules 缺失时执行 npm --prefix web ci（项目本地安装，属白名单）外不改任何文件、不重跑 cargo/npm 门禁（半成品批次的阶段级门禁在 P7-5 到 P7-7 完成后照常全量执行）。推送通道自检只做 git push --dry-run origin main，失败如实报告（新机器 SSH 密钥需要用户自行配置，不要试图改用户的 SSH 配置）。`).ask<VerifyReport>(`跨机接手核对：前一台机器的活体运行 dwfrun-954913b0 已暂停，本机从 origin 重建续作。逐条核对并给证据：
1. git log --oneline -10 可见：SK 扩展收口 2bf8e9c 与 RELEASE 回填 05ed5ee、Phase 7 批次 1 到 4 半成品 WIP 备份 7effba9、交接文档更新 9639689；git status -sb 与 origin/main 同步无未提交源码改动。
2. Phase 5/6 完成态在位：docs/releases/RELEASE_2026-11-11_PHASE5.md 与 RELEASE_2026-11-12_PHASE6.md、docs/phase5/browser-e2e/ 与 docs/phase6/browser-e2e/。
3. SK 完成态在位：docs/releases/RELEASE_2026-11-12_SK-SECURITY-SOURCES.md、docs/phase6/browser-e2e/skverify-01 到 04 截图。
4. Phase 7 半成品在位：docs/releases/RELEASE_2026-11-12_PHASE7-P7-2.md、RELEASE_2026-11-13_PHASE7-P7-3.md、RELEASE_2026-11-13_PHASE7-P7-4.md、docs/phase7/AGENT-MEMORY-REVIEW.md、docs/phase7/browser-e2e/01/（P7-1 五区壳层 9 截图）、migrations/013_agent_memory.sql 与 014_agent_run_control.sql、crates/agent-engine/、web/src/components/TabArea.tsx 与 InspectorPanel.tsx 与 ActivityPanel.tsx。
5. 环境自检：cargo --version、python3 --version、node --version 各报版本；vendor/ 与 web/package-lock.json 在位；web/node_modules 缺失则 npm --prefix web ci 后报结果；git push --dry-run origin main 结果。
6. SESSION-HANDOFF.md 第三节有 SK 完成行与 Phase 7 进行中行（2026-11-13 用户暂停点）。
返回 VerifyReport：items 每条 criterion/pass/evidence；ok 仅当全部通过；gaps 列出任何偏差与环境缺件（如 node 版本过低、SSH 推送不可用、浏览器缺失）。`);
log(`接手核对${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`);

return await hole<FinalReport>(
  "Phase 7 后续批次实施（P7-5 到 P7-7）",
  [
    `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
    `基线：Phase 5（c0a4284/6217ca8）、Phase 6 + SK 扩展（49b9377/ace3c53/2bf8e9c）已入库；Phase 7 批次 1 到 4 半成品 7effba9（五区壳层、agent-engine 最小执行闭环、013 三类 Memory、014 运行控制九类护栏）`,
    `本洞内容：P7-5（trace 统一关联与跨栈查询，规格已钉死，迁移编号用 015）→ P7-6/P7-7（规划员补规划，P7-7 含 Agent Studio 页）→ 五道门禁 → 独立验收（浏览器实测新壳与 Agent Studio）→ 入库 RELEASE_PHASE7 → 留 Phase 8 尾洞`,
  ].join("\n"), async () => {
  report({ phase: TRACKED[2], status: "进行中", note: "批次 1 到 4 半成品 7effba9，从 P7-5 续作" }, "phases");

  const p75: PhaseBatch = {
    id: "P7-5",
    goal: "trace 统一关联与跨栈查询（SESSION-HANDOFF §十追加项 1）：trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id 六 ID 统一定义（crates/domain/src/ids.rs 集中类型）并贯穿三栈：Rust 写面统一注入（audit-engine correlation_id 体系扩展 trace_id，tool_run/artifact/evidence/agent_run 行均可按 trace_id 关联），Python 侧 apps/server/rp_server.py 与 workers 响应透传六 ID（现状 grep correlation/trace 零命中，开工先实查响应面），Web 侧 Runs/RunDetail 与 Agent Studio 展示六 ID 并可点击跳转；提供统一只读查询入口（rp-agent trace 子命令或等价 SQL 视图）：按任一 ID 关联出任务、意图、上下文、工具、证据全链；未验证项如实标注",
    paths: ["crates/domain/src/ids.rs", "crates/domain/src/audit.rs", "crates/audit-engine/src/lib.rs", "crates/tool-engine/src/（run 行补 trace 关联字段）", "crates/agent-engine/src/trace.rs（新）", "migrations/015_trace_correlation.sql（新，注意：014 已被 P7-4 agent_run_control 占用，编号顺延）", "apps/server/rp_server.py（响应透传）", "web/src/lib/api.ts", "web/src/routes/RunDetailPage.tsx", "web/src/routes/RunsPage.tsx"],
    check: "cargo test --offline --workspace 全过；一条端到端 run 后：按 trace_id 与按 evidence_id 两个方向各执行一次关联查询均返回完整五类事件链（audit_event 只读查开发库副本）；RunDetailPage smoke 测试断言六 ID 字段在场；Python /api 响应实测含 trace 字段（selftest 脚本断言）",
  };

  phase("规划 Phase 7 剩余批次");
  const plan7 = await agent("Phase 7 规划员").ask<PhasePlan>(`${DISCIPLINE}

  你是 Phase 7 规划员（跨机续作场景）。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 7 节、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§9 与 §14 Phase 7、docs/SESSION-HANDOFF.md §十（trace 关联与研究能力契约化验收）与 §十一（WebUI 工作台目标形态）、第三节 Phase 7 进行中行、git log --oneline -8 与 git show --stat 7effba9（批次 1 到 4 半成品现状）。产出剩余批次清单。硬性要求：
  1. P7-5 已钉死如下，原样列入 batches[0]，不要改动其 goal/paths/check：${JSON.stringify(p75)}
  2. 只规划 P7-6 与 P7-7 两批。必须覆盖两项用户硬性验收：a) 研究能力契约化（§十）：Research Planner/Source Discovery/Evidence Extraction/Verification/Synthesis/Citation/Evaluator 七能力以输入输出契约 Schema 加 mock 实现加测试交付，不是 prompt；b) Agent Studio 页落新壳（§十一接入点，P7-1 已在 web/src/routes/AgentsPage.tsx 留占位并标注 P7-7）。
  3. 批次实现要基于半成品现状续作不推翻：crates/agent-engine/（store/state/control/memory/loop 已在位）、013/014 迁移已落、五区壳层已迁完全部页面。
  4. acceptance 对应 2.0 计划 §14 Phase 7 验收加 §十/§十一追加项，逐条可核验；每批自验 check 写明命令与预期输出。
  5. Agent Studio 需要的 AgentRun/step 轨迹读取面若 CLI 未覆盖，在对应批次补 API 或 CLI（014 的 agent_run_step 表已落数据面）。
  返回 PhasePlan（batches 共 3 项：P7-5 钉死项 + P7-6 + P7-7）。`);

  const results7: BatchResult[] = [];
  phase("逐批实施 P7-5 到 P7-7");
  for (let i = 0; i < plan7.batches.length; i++) {
    const b = plan7.batches[i];
    const prev = results7.slice(-1)[0];
    const r = await agent().ask<BatchResult>(`${DISCIPLINE}

  你是 Phase 7 实施员（全新上下文，每批独立以控成本）。执行批次 ${b.id}（第 ${i + 1}/${plan7.batches.length} 批）：${b.goal}
  关键路径：${b.paths.join("、")}
  本批自验：${b.check}
  ${prev ? `上一批交接（done 与 caveats）：${JSON.stringify({ done: prev.done, caveats: prev.caveats })}` : `这是本机续作首批。基线：批次 1 到 4 半成品在 7effba9（五区壳层、agent-engine 最小执行闭环与九类运行控制、013 三类 Memory、014 控制与步骤轨迹两表），工作树与 origin/main 同步；P7-4 交接要点：九类护栏与循环同批落地、pause/resume 轨迹续接不重跑、CLI 四子命令（run/def-add/def-show/run-show）未挂控制面（pause/resume/cancel/retry 以 Rust API 加测试为验收面）、目标偏移检测是确定性代理定义。`}
  复用优先：通用逻辑用成熟库或仓库既有实现；迁移只新增编号文件（下一可用编号从 015 起）在测试副本验证；web 面不动既有页面行为。完成后返回 BatchResult：files、check、caveats。`);
    results7.push(r);
    report({ batch: r.batch, done: r.done, caveats: r.caveats.join("；") });
  }

  phase("跑 Phase 7 门禁");
  let green7 = false;
  let fail7 = "";
  for (let round = 1; round <= 3; round++) {
    const t = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
    const c = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
    const u = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
    const fb = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
    const ft = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
    if (t.exitCode === 0 && c.exitCode === 0 && u.exitCode === 0 && fb.exitCode === 0 && ft.exitCode === 0) {
      green7 = true;
      fail7 = "";
      log(`Phase 7 五道门禁全绿（第 ${round} 轮）`);
      break;
    }
    fail7 = [
      t.exitCode !== 0 ? `cargo test exit=${t.exitCode}\n${t.stdout.slice(-1000)}` : "",
      c.exitCode !== 0 ? `clippy exit=${c.exitCode}\n${c.stdout.slice(-1000)}` : "",
      u.exitCode !== 0 ? `unittest exit=${u.exitCode}\n${u.stderr.slice(-1000)}` : "",
      fb.exitCode !== 0 ? `npm build exit=${fb.exitCode}\n${fb.stdout.slice(-1000)}` : "",
      ft.exitCode !== 0 ? `npm test exit=${ft.exitCode}\n${ft.stdout.slice(-1000)}` : "",
    ].filter((s) => s !== "").join("\n----\n");
    await agent().ask<string>(`${DISCIPLINE}

  你是 Phase 7 门禁修复员（全新上下文，看工作树与失败输出定位修复）。第 ${round} 轮失败：\n${fail7}`);
  }

  phase("Phase 7 独立验收");
  const acc7 = await agent("Phase 7 验收员", `${DISCIPLINE}

  你是 Phase 7 独立验收员，立场是挑毛病而不是放行。读实现找证据并实测（浏览器实测新壳五区布局与 Agent Studio：先加载 browser-use:control-browser 技能，留截图到 docs/phase7/browser-e2e/02/ 起新目录，浏览器二进制按纪律第 7 条探测）；脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验：
  ${plan7.acceptance.map((a) => "- " + a).join("\n")}
  批次摘要：${JSON.stringify(results7.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
  另必核验批次 1 到 4 半成品未回归（git show --stat 7effba9 对应面）：1) Agent 端到端：创建研究任务让内置 Agent 经 Context Broker 查知识（含 SK 三源条目）并调用 Tool Engine 真实工具，AgentRun/ToolRun/Artifact/Evidence 关联可查（trace_id 贯穿，P7-5 交付面）；2) 暂停/恢复/取消真实生效（P7-4 已有 Rust API 测试面，复验不回归）；3) 新壳五区布局与多标签、Workspace/Project/Agent/Workflow 概念在数据模型与 UI 分离（读 workspaces 模型与 API 证据）；4) 研究能力契约有 Schema 有测试非 prompt；5) 复用检查与旧页面不回归（selftest_auth 11 项：bash apps/server/selftest_auth.sh）。
  返回 PhaseAcceptance。`);
  for (let round = 1; round <= 2 && !acc7.acceptable; round++) {
    await agent().ask<string>(`${DISCIPLINE}

  你是 Phase 7 修复员。按验收意见修复（第 ${round} 轮）：${JSON.stringify(acc7.fixes)}`);
    const reacc = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

  你是 Phase 7 复审员，复核上轮 fixes 并复测问题点（含浏览器复测），返回 PhaseAcceptance。上轮结论：${JSON.stringify(acc7.items.filter((x) => !x.pass))}`);
    acc7.items = reacc.items;
    acc7.acceptable = reacc.acceptable;
    acc7.fixes = reacc.fixes;
  }

  phase("Phase 7 入库");
  const ok7 = acc7.acceptable && green7;
  const commit7 = await agent("Phase 7 入库员", `${DISCIPLINE}

  你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 7 整段（含批次 1 到 4 半成品 7effba9 的正式收口与本机 P7-5 到 P7-7 新增）：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE7.md（总账：批次 1 到 7 汇总、五道门禁数字、trace 关联说明、新壳与 Agent Studio 截图路径引用、已知取舍、回滚方式；既有 RELEASE_*_PHASE7-P7-2/3/4.md 作为分批记录引用不重写），docs/README.md 登记 Phase 7 行。3) docs/SESSION-HANDOFF.md 第三节把 Phase 7 进行中行改写为完成行（含 push 短哈希回填注记）、第十一节标注壳层升级落地。4) git add -A，commit（中文：RIP 2.0 Phase 7 Agent Engine 与工作台壳层升级），push origin main（若新机器 SSH 未配置导致 push 失败，如实报错原文不伪造）。返回 CommitResult。`);
  report({ phase: TRACKED[2], status: ok7 && commit7.pushed ? "完成" : "受阻", note: `commit ${commit7.hash}，验收${ok7 ? "通过" : "有遗留"}，门禁${green7 ? "全绿" : "未全绿"}，push ${commit7.pushed ? "成功" : "待核"}` }, "phases");
  log(`Phase 7 ${ok7 && commit7.pushed ? "完成" : "受阻"}：commit ${commit7.hash}`);

  return await hole<FinalReport>(
    "Phase 8 起逐阶段实施",
    [
      `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
      `Phase 7 结果：${ok7 && commit7.pushed ? "完成" : "受阻"}，commit ${commit7.hash}，push ${commit7.pushed ? "成功" : commit7.note}`,
      `Phase 7 验收：${acc7.acceptable ? "全部通过" : JSON.stringify(acc7.items.filter((x) => !x.pass))}`,
      `批次摘要：${results7.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
      `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 8 节（Workflow Studio）与 2.0 计划 §10.4 G、SESSION-HANDOFF §十（持久化/checkpoint/恢复验收，参考 LangGraph 与 Temporal 设计不替换 Rust 自研）。写「实施 Phase 8」填充：规划员 → 每批新实施员（含 XYFlow 画布批次与浏览器实测画布交互）→ 五道门禁 → 独立验收（多节点工作流编辑运行、暂停恢复、失败重试、版本管理，浏览器实测）→ 入库 RELEASE_PHASE8；结尾留 Phase 9 尾洞。`,
    ].join("\n"), async () => {
    report({ phase: TRACKED[3], status: "进行中", note: `Phase 7 已收口（commit ${commit7.hash}），Phase 8 Workflow Studio 启动` }, "phases");

    phase("规划 Phase 8 批次");
    const plan8 = await agent("Phase 8 规划员").ask<PhasePlan>(`${DISCIPLINE}

      你是 Phase 8 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 8 节（Workflow Studio）、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§10.4 G 与 §14 Phase 8、docs/SESSION-HANDOFF.md §十（持久化/checkpoint/恢复验收）与 §十一、git log --oneline -8（Phase 7 刚收口 commit ${commit7.hash}，含 P7-5 trace 六 ID 贯穿）。产出 Phase 8 批次清单。硬性要求：
      1. 覆盖三项用户硬性验收：a) 多节点工作流可视化编辑与真实运行：画布用成熟维护的开源库（web 侧首选 @xyflow/react 即 React Flow），npm 依赖只进 web/ 并锁定版本，不手写画布引擎；b) 持久化/checkpoint/恢复：工作流定义与运行状态落库（迁移只新增编号文件，下一可用编号 016 起），断点恢复与失败重试真实生效；设计参考 LangGraph 与 Temporal 的状态机与 checkpoint 思路，但执行引擎沿用仓库 Rust 自研不替换；c) 工作流版本管理：定义可版本化、历史版本可查可回看。
      2. 批次基于 Phase 7 现状续作不推翻：agent-engine（run/control/memory/trace 关联）、013/014/015 迁移、五区壳层与 Agent Studio 页、Web 工作台占位路由在位。
      3. acceptance 对应 2.0 计划 §14 Phase 8 验收加 §十追加项，逐条可核验；每批自验 check 写明命令与预期输出。
      4. 画布批次必须安排浏览器实测（拖拽节点、连线、保存、运行、暂停恢复全程真实浏览器操作）；浏览器二进制按纪律第 7 条探测。
      返回 PhasePlan（batches 2 到 4 项）。`);

    const results8: BatchResult[] = [];
    phase("逐批实施 Phase 8");
    for (let i8 = 0; i8 < plan8.batches.length; i8++) {
      const b8 = plan8.batches[i8];
      const prev8 = results8.slice(-1)[0];
      const r8 = await agent().ask<BatchResult>(`${DISCIPLINE}

      你是 Phase 8 实施员（全新上下文，每批独立以控成本）。执行批次 ${b8.id}（第 ${i8 + 1}/${plan8.batches.length} 批）：${b8.goal}
      关键路径：${b8.paths.join("、")}
      本批自验：${b8.check}
      ${prev8 ? `上一批交接（done 与 caveats）：${JSON.stringify({ done: prev8.done, caveats: prev8.caveats })}` : `这是 Phase 8 首批。基线：Phase 7 已收口（commit ${commit7.hash}）：agent-engine 执行闭环与九类运行控制、013 三类 Memory、014 运行控制、015 trace 关联（v_trace_chain 视图、rp_server /api/traces 端点、TracePage 链路页）、五区壳层与 Agent Studio 页全部在位，工作树与 origin/main 同步；迁移下一可用编号 016。`}
      复用优先：通用逻辑用成熟库或仓库既有实现；迁移只新增编号文件在测试副本验证；web 面不动既有页面行为，新 npm 依赖只进 web/ 并锁定版本。完成后返回 BatchResult：files、check、caveats。`);
      results8.push(r8);
      report({ batch: r8.batch, done: r8.done, caveats: r8.caveats.join("；") });
    }

    phase("跑 Phase 8 门禁");
    let green8 = false;
    let fail8 = "";
    for (let round8 = 1; round8 <= 3; round8++) {
      const t8g = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
      const c8g = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
      const u8g = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
      const fb8g = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
      const ft8g = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
      if (t8g.exitCode === 0 && c8g.exitCode === 0 && u8g.exitCode === 0 && fb8g.exitCode === 0 && ft8g.exitCode === 0) {
        green8 = true;
        fail8 = "";
        log(`Phase 8 五道门禁全绿（第 ${round8} 轮）`);
        break;
      }
      fail8 = [
        t8g.exitCode !== 0 ? `cargo test exit=${t8g.exitCode}\n${t8g.stdout.slice(-1000)}` : "",
        c8g.exitCode !== 0 ? `clippy exit=${c8g.exitCode}\n${c8g.stdout.slice(-1000)}` : "",
        u8g.exitCode !== 0 ? `unittest exit=${u8g.exitCode}\n${u8g.stderr.slice(-1000)}` : "",
        fb8g.exitCode !== 0 ? `npm build exit=${fb8g.exitCode}\n${fb8g.stdout.slice(-1000)}` : "",
        ft8g.exitCode !== 0 ? `npm test exit=${ft8g.exitCode}\n${ft8g.stdout.slice(-1000)}` : "",
      ].filter((s) => s !== "").join("\n----\n");
      await agent().ask<string>(`${DISCIPLINE}

      你是 Phase 8 门禁修复员（全新上下文，看工作树与失败输出定位修复，只修失败不推翻既有实现）。第 ${round8} 轮失败：\n${fail8}`);
    }

    phase("Phase 8 独立验收");
    const acc8 = await agent("Phase 8 验收员", `${DISCIPLINE}

      你是 Phase 8 独立验收员，立场是挑毛病而不是放行。读实现找证据并实测（浏览器实测画布全流程：先加载 browser-use:control-browser 技能，截图落 docs/phase8/browser-e2e/01/ 起新目录；浏览器二进制按纪律第 7 条探测，本机已知 Playwright 用户缓存有 Chrome for Testing 151（/home/emo/.distrobox/ubuntu-home/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome），设 PW_CHROMIUM 指向该二进制即可）；脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验：
      ${plan8.acceptance.map((a) => "- " + a).join("\n")}
      批次摘要：${JSON.stringify(results8.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
      另必核验 Phase 7 未回归：1) Agent 端到端与 trace 六 ID 关联查询（/api/traces 与 TracePage 双向可达）；2) AgentRun 暂停/恢复/取消不回归；3) 五区壳层与 Agent Studio 不回归；4) 复用检查与旧页面不回归（selftest_auth 11 项：bash apps/server/selftest_auth.sh）。
      返回 PhaseAcceptance。`);
    for (let round8 = 1; round8 <= 2 && !acc8.acceptable; round8++) {
      await agent().ask<string>(`${DISCIPLINE}

      你是 Phase 8 修复员。按验收意见修复（第 ${round8} 轮）：${JSON.stringify(acc8.fixes)}`);
      const reacc8 = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

      你是 Phase 8 复审员，复核上轮 fixes 并复测问题点（含浏览器复测），返回 PhaseAcceptance。上轮结论：${JSON.stringify(acc8.items.filter((x) => !x.pass))}`);
      acc8.items = reacc8.items;
      acc8.acceptable = reacc8.acceptable;
      acc8.fixes = reacc8.fixes;
    }

    phase("Phase 8 入库");
    const ok8 = acc8.acceptable && green8;
    const commit8 = await agent("Phase 8 入库员", `${DISCIPLINE}

      你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 8 整段：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE8.md（总账：批次汇总、五道门禁数字、画布与 checkpoint/恢复说明、浏览器实测截图路径引用、已知取舍、回滚方式），docs/README.md 登记 Phase 8 行。3) docs/SESSION-HANDOFF.md 第三节新增 Phase 8 完成行（含 push 短哈希回填注记）。4) git add -A，commit（中文：RIP 2.0 Phase 8 Workflow Studio），push origin main（push 失败重试 2 到 3 次，仍失败如实报错原文不伪造）。返回 CommitResult。`);
    report({ phase: TRACKED[3], status: ok8 && commit8.pushed ? "完成" : "受阻", note: `commit ${commit8.hash}，验收${ok8 ? "通过" : "有遗留"}，门禁${green8 ? "全绿" : "未全绿"}，push ${commit8.pushed ? "成功" : "待核"}` }, "phases");
    log(`Phase 8 ${ok8 && commit8.pushed ? "完成" : "受阻"}：commit ${commit8.hash}`);

    return await hole<FinalReport>(
      "Phase 9 起逐阶段实施",
      [
        `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
        `Phase 7 结果：完成，commit ${commit7.hash}，push 成功；验收全部通过`,
        `Phase 8 结果：${ok8 && commit8.pushed ? "完成" : "受阻"}，commit ${commit8.hash}，push ${commit8.pushed ? "成功" : commit8.note}`,
        `Phase 8 验收：${acc8.acceptable ? "全部通过" : JSON.stringify(acc8.items.filter((x) => !x.pass))}`,
        `批次摘要：${results8.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
        `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 9 节（MCP/REST/CLI 对外能力）与 2.0 计划 §11、SESSION-HANDOFF §十（correlation id 对外、MCP 与 Tool Center 同源验收）。写「实施 Phase 9」填充：规划员 → 每批新实施员（MCP server、REST 对外收口、CLI 面向外部用户，correlation/trace id 对外透出与 Phase 8 工作流运行衔接）→ 五道门禁 → 独立验收（MCP 客户端实测、REST 端到端、CLI 冒烟）→ 入库 RELEASE_PHASE9；结尾留 Phase 10 尾洞。`,
      ].join("\n"), async () => {
      report({ phase: TRACKED[4], status: "进行中", note: `Phase 8 已收口（commit ${commit8.hash}）且 CI 全绿，Phase 9 MCP/REST/CLI 对外能力启动` }, "phases");

      phase("规划 Phase 9 批次");
      const plan9 = await agent("Phase 9 规划员").ask<PhasePlan>(`${DISCIPLINE}

        你是 Phase 9 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 9 节（MCP/REST/CLI 对外能力）、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§11 与 §14 Phase 9、docs/SESSION-HANDOFF.md §十（correlation id 对外透出、MCP 与 Tool Center 同源验收）、git log --oneline -10（Phase 8 刚收口 commit ${commit8.hash}：workflow-engine 与画布在库）。产出 Phase 9 批次清单。硬性要求：
        1. 三面同源，禁止重建接口体系：a) MCP server 面向外部客户端暴露平台能力（知识检索/工具派发/运行查询等，范围按 2.0 计划 §11），与 Tool Center 同源（同一能力清单同一 Schema，MCP 工具面不得出现 Tool Center 没有的能力）；b) REST 对外收口基于 apps/server/rp_server.py 既有 API 面做对外契约化（鉴权沿用 selftest_auth 口径，trace 六 ID 与 correlation id 在响应与文档中透出），不另起第二套接口风格；c) CLI 面向外部用户（rp-* 命令族补齐对外子命令与帮助面，复用既有 bin）。
        2. 对外安全边界：对外接口只暴露只读或白名单能力，漏洞细节与 RTIS 生产数据面不进对外接口（仓库红线）；鉴权、限频、超时必须有。
        3. 批次基于现状续作：trace 六 ID（P7-5）、agent-engine 闭环（P7）、workflow-engine 与画布（P8）、research-engine 契约（P7-6）全部在库；迁移只新增编号文件（下一可用编号从 migrations/ 实际清单确认，018 已占用）。
        4. acceptance 对应 2.0 计划 §14 Phase 9 验收加 §十追加项，逐条可核验；每批自验 check 写明命令与预期输出。
        5. 若涉及 Tool Center 页改动（MCP 状态展示等），该批必须含浏览器实测；浏览器二进制按纪律第 7 条探测。
        返回 PhasePlan（batches 2 到 4 项）。`);

      const results9: BatchResult[] = [];
      phase("逐批实施 Phase 9");
      for (let i9 = 0; i9 < plan9.batches.length; i9++) {
        const b9 = plan9.batches[i9];
        const prev9 = results9.slice(-1)[0];
        const r9 = await agent().ask<BatchResult>(`${DISCIPLINE}

        你是 Phase 9 实施员（全新上下文，每批独立以控成本）。执行批次 ${b9.id}（第 ${i9 + 1}/${plan9.batches.length} 批）：${b9.goal}
        关键路径：${b9.paths.join("、")}
        本批自验：${b9.check}
        ${prev9 ? `上一批交接（done 与 caveats）：${JSON.stringify({ done: prev9.done, caveats: prev9.caveats })}` : `这是 Phase 9 首批。基线：Phase 8 已收口（commit ${commit8.hash}）且 CI 全绿：workflow-engine（DSL/六类节点/016 到 018 迁移/checkpoint/控制面）与画布五区实页在库，trace 六 ID 贯穿三栈（v_trace_chain、/api/traces、TracePage），research-engine 七能力契约在库，rp_server API 面含 selftest_auth 11 项口径；迁移下一可用编号从 migrations/ 实际清单确认。`}
        复用优先：MCP/REST/CLI 三面共用同一 Application Services 与 Schema；通用逻辑用成熟库或仓库既有实现；迁移只新增编号文件在测试副本验证；web 面不动既有页面行为。完成后返回 BatchResult：files、check、caveats。`);
        results9.push(r9);
        report({ batch: r9.batch, done: r9.done, caveats: r9.caveats.join("；") });
      }

      phase("跑 Phase 9 门禁");
      let green9 = false;
      let fail9 = "";
      for (let round9 = 1; round9 <= 3; round9++) {
        const t9g = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
        const c9g = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
        const u9g = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
        const fb9g = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
        const ft9g = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
        if (t9g.exitCode === 0 && c9g.exitCode === 0 && u9g.exitCode === 0 && fb9g.exitCode === 0 && ft9g.exitCode === 0) {
          green9 = true;
          fail9 = "";
          log(`Phase 9 五道门禁全绿（第 ${round9} 轮）`);
          break;
        }
        fail9 = [
          t9g.exitCode !== 0 ? `cargo test exit=${t9g.exitCode}\n${t9g.stdout.slice(-1000)}` : "",
          c9g.exitCode !== 0 ? `clippy exit=${c9g.exitCode}\n${c9g.stdout.slice(-1000)}` : "",
          u9g.exitCode !== 0 ? `unittest exit=${u9g.exitCode}\n${u9g.stderr.slice(-1000)}` : "",
          fb9g.exitCode !== 0 ? `npm build exit=${fb9g.exitCode}\n${fb9g.stdout.slice(-1000)}` : "",
          ft9g.exitCode !== 0 ? `npm test exit=${ft9g.exitCode}\n${ft9g.stdout.slice(-1000)}` : "",
        ].filter((s) => s !== "").join("\n----\n");
        await agent().ask<string>(`${DISCIPLINE}

        你是 Phase 9 门禁修复员（全新上下文，看工作树与失败输出定位修复，只修失败不推翻既有实现）。第 ${round9} 轮失败：\n${fail9}`);
      }

      phase("Phase 9 独立验收");
      const acc9 = await agent("Phase 9 验收员", `${DISCIPLINE}

        你是 Phase 9 独立验收员，立场是挑毛病而不是放行。读实现找证据并实测：MCP 面用真实客户端实测（inspector 或等价 stdio 客户端脚本，逐工具调用并核对与 Tool Center 能力清单同源）；REST 用端到端请求实测（含鉴权拒绝路径与 trace 字段透出）；CLI 逐子命令冒烟；若涉及 Tool Center 页改动，浏览器实测（加载 browser-use:control-browser 技能，截图落 docs/phase9/browser-e2e/01/，浏览器二进制按纪律第 7 条探测：本机已知 Playwright 用户缓存 Chrome for Testing 151 于 /home/emo/.distrobox/ubuntu-home/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome，设 PW_CHROMIUM 指向即可）；脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验：
        ${plan9.acceptance.map((a) => "- " + a).join("\n")}
        批次摘要：${JSON.stringify(results9.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
        另必核验 Phase 8 未回归：1) workflow 端到端（画布保存/运行/暂停/恢复）；2) trace 六 ID 关联查询（/api/traces 双向）；3) Agent Studio 与五区壳层不回归；4) selftest_auth 11 项：bash apps/server/selftest_auth.sh。
        返回 PhaseAcceptance。`);
      for (let round9 = 1; round9 <= 2 && !acc9.acceptable; round9++) {
        await agent().ask<string>(`${DISCIPLINE}

        你是 Phase 9 修复员。按验收意见修复（第 ${round9} 轮）：${JSON.stringify(acc9.fixes)}`);
        const reacc9 = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

        你是 Phase 9 复审员，复核上轮 fixes 并复测问题点（含浏览器与 MCP 客户端复测），返回 PhaseAcceptance。上轮结论：${JSON.stringify(acc9.items.filter((x) => !x.pass))}`);
        acc9.items = reacc9.items;
        acc9.acceptable = reacc9.acceptable;
        acc9.fixes = reacc9.fixes;
      }

      phase("Phase 9 入库");
      const ok9 = acc9.acceptable && green9;
      const commit9 = await agent("Phase 9 入库员", `${DISCIPLINE}

        你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 9 整段：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE9.md（总账：批次汇总、五道门禁数字、MCP/REST/CLI 三面同源说明、鉴权与限频边界、浏览器与客户端实测证据路径引用、已知取舍、回滚方式），docs/README.md 登记 Phase 9 行。3) docs/SESSION-HANDOFF.md 第三节新增 Phase 9 完成行（含 push 短哈希回填注记）。4) git add -A，commit（中文：RIP 2.0 Phase 9 MCP REST CLI 对外能力），push origin main（push 失败重试 2 到 3 次，仍失败如实报错原文不伪造）。返回 CommitResult。`);
      report({ phase: TRACKED[4], status: ok9 && commit9.pushed ? "完成" : "受阻", note: `commit ${commit9.hash}，验收${ok9 ? "通过" : "有遗留"}，门禁${green9 ? "全绿" : "未全绿"}，push ${commit9.pushed ? "成功" : "待核"}` }, "phases");
      log(`Phase 9 ${ok9 && commit9.pushed ? "完成" : "受阻"}：commit ${commit9.hash}`);

      return await hole<FinalReport>(
        "Phase 10 起逐阶段实施",
        [
          `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
          `Phase 7 结果：完成，commit 102cab5，push 成功；验收全部通过`,
          `Phase 8 结果：完成，commit ${commit8.hash}，push 成功；验收全部通过；CI 修复批次 452fa38/0ddb397/da9e0c1/d04eca2 后全绿`,
          `Phase 9 结果：${ok9 && commit9.pushed ? "完成" : "受阻"}，commit ${commit9.hash}，push ${commit9.pushed ? "成功" : commit9.note}`,
          `Phase 9 验收：${acc9.acceptable ? "全部通过" : JSON.stringify(acc9.items.filter((x) => !x.pass))}`,
          `批次摘要：${results9.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
          `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 10 节（生态与发布）与 2.0 计划 §12、SESSION-HANDOFF §十（backup/restore、FTS 重建校验、保留归档、许可证逐包清单、axe 对比度、视觉基线）与 autoresearch 性能基准要求。写「实施 Phase 10」填充：规划员 → 每批新实施员（备份恢复实现、FTS 重建校验、许可证逐包清单、axe 对比度修正、视觉基线建立、autoresearch 性能基准）→ 五道门禁 → 独立验收（备份恢复实演、基线校验，浏览器实测）→ 入库 RELEASE_PHASE10；结尾留收尾闭环尾洞（SESSION-HANDOFF §十二与 §十四：ChatGPT 全面审阅、GitHub 知识库与武器库补齐、实战校验三面）。`,
        ].join("\n"), async () => {
        report({ phase: "rmcp 迁移（阻断性整改批）", status: "进行中", note: "Phase 9 已收口（commit 95f4e5e）且 CI 绿（ae73b92 文档批次后），按用户定案审计方案执行 rmcp 迁移前置批" }, "phases");

        phase("rmcp 迁移前置整改批");
        const rR = await agent().ask<BatchResult>(`${DISCIPLINE}

          你是 rmcp 迁移整改批实施员（全新上下文）。背景：Phase 9 的 crates/mcp-server 手写了 MCP 2024-11-05 子集协议层，外部审计（docs/phase9/MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md，先完整读它）判定不可长期保留并给出十项整改任务，用户定案「按照这个方案」。本批执行审计 §5 全部十项：
          1) 复核最终树：git log --oneline -5 与 git status -sb 记录基线；通读 crates/mcp-server/、crates/rp-mcp-parity/、apps/server/selftest_phase9_mcp.sh、docs/phase9/ 设计文档。
          2) rmcp 选型记录（写入 docs/phase9/RMCP-MIGRATION.md）：官方 crates.io 发布版（不跟 git main），记录版本、MSRV、许可证、依赖树、安全公告、feature 集与 2024-11-05 兼容性；版本号必须以实际构建验证为准。
          3) 增量 vendor：cargo 已配 rsproxy 镜像（AGENTS.md 记载），受控联网拉取 rmcp 最小依赖集（只启用 server 与 stdio 所需 feature，不引入 HTTP/OAuth/client/elicitation），更新 Cargo.lock 与 vendor/，恢复 .cargo/config 的 vendored 口径后在干净状态验证 --offline 构建与测试可复现。
          4) rp-mcp 改造为 rmcp server + stdio transport：RIP 业务 handler（目录查询、tools/call 派发）接入 SDK，协议解析、initialize 生命周期、版本协商、协议错误封装、stdio I/O 全部交给 rmcp。
          5) 修 P0：tools/list 每个 Tool 对象必须含标准必需 name 与 inputSchema（description 可精简）；mcp_protocol.rs 里断言 inputSchema 不得出现的旧测试改写为标准一致性断言；分页保留用于控量，不得用省略必需 Schema 控 Token；rip.get_tool_schema 保留为平台自定义按需详情工具。
          6) 修 P1：initialize 版本协商由 SDK 处理（支持则协商，不支持按规范报错或返回服务端版本），不得回显任意客户端版本。
          7) 保留并复核这些设计不被破坏：MCP 目录与 Tool Center 同一 tool 表读面（主库优先 aux 回退）、catalog 只读零 SQL 写、工具执行唯一经 tool-engine Engine/Gateway 派发（不建第二套）、EvidenceRegistrar 复用、rip.* 命名空间、stdout 只出协议消息 stderr 走日志。
          8) 重定义 RP_MCP_TOKEN 安全边界：明确「宿主进程环境变量注入即授权边界」与「客户端协议认证」的区别并写进部署文档与测试（缺失/错误/正确令牌三态实测）；不发明 MCP 标准外的认证字段。
          9) 真实 MCP 客户端端到端验收：initialize（含版本协商）→ notifications/initialized → tools/list 多页（含 inputSchema 校验）→ tools/call 成功与失败 → 未知工具 → 通知与超时处理 → stdout/stderr 分离 → 进程退出；留日志或截图证据到 docs/phase9/rmcp-client-e2e/。
          10) conformance 测试：覆盖 2024-11-05 与 rmcp 实际支持协议版本；测试不得以手写期望固化非标准行为；apps/server/selftest_phase9_mcp.sh 与 REST/MCP parity 断言按新协议行为同步更新（parity op1/op2 必须继续全过）。
          离线验收：cargo test --offline --workspace 全过、clippy -D warnings 零告警、bash apps/server/selftest_phase9_mcp.sh 全过、生产 platform*.db 零写指纹一致。
          红线：integrations/rtis/ 与 scripts/ 零修改；不建第二套 Tool Registry 或派发链；受控联网仅限 crates.io 与 rsproxy 拉依赖，vendor 完成后一切回归离线。
          成熟库七小节（审计 §6.6）随 RMCP-MIGRATION.md 交付：成熟方案检索结果/候选比较/采用库与 feature/vendor 方案/自研边界（业务 handler 清单）/例外 ADR（无）/兼容性能安全回归验收。
          完成后返回 BatchResult：files、check、caveats。`);
        report({ batch: rR.batch, done: rR.done, caveats: rR.caveats.join("；") });

        phase("rmcp 迁移门禁");
        let greenR = false;
        let failR = "";
        for (let roundR = 1; roundR <= 3; roundR++) {
          const tRg = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
          const cRg = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
          const uRg = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
          const fbRg = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
          const ftRg = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
          if (tRg.exitCode === 0 && cRg.exitCode === 0 && uRg.exitCode === 0 && fbRg.exitCode === 0 && ftRg.exitCode === 0) {
            greenR = true;
            failR = "";
            log(`rmcp 迁移批五道门禁全绿（第 ${roundR} 轮）`);
            break;
          }
          failR = [
            tRg.exitCode !== 0 ? `cargo test exit=${tRg.exitCode}\n${tRg.stdout.slice(-1000)}` : "",
            cRg.exitCode !== 0 ? `clippy exit=${cRg.exitCode}\n${cRg.stdout.slice(-1000)}` : "",
            uRg.exitCode !== 0 ? `unittest exit=${uRg.exitCode}\n${uRg.stderr.slice(-1000)}` : "",
            fbRg.exitCode !== 0 ? `npm build exit=${fbRg.exitCode}\n${fbRg.stdout.slice(-1000)}` : "",
            ftRg.exitCode !== 0 ? `npm test exit=${ftRg.exitCode}\n${ftRg.stdout.slice(-1000)}` : "",
          ].filter((s) => s !== "").join("\n----\n");
          await agent().ask<string>(`${DISCIPLINE}

          你是 rmcp 迁移门禁修复员（看工作树与失败输出定位修复，只修失败）。第 ${roundR} 轮失败：\n${failR}`);
        }

        phase("rmcp 迁移入库");
        const accR = await agent("rmcp 迁移验收员", `${DISCIPLINE}

          你是 rmcp 迁移独立验收员，立场是挑毛病而不是放行。按审计 §5 验收标准逐条核验：协议解析/initialize 生命周期/版本协商/tools list/协议错误/stdio transport 是否全部由 rmcp 承担；tools/list 每条目是否含必需 inputSchema；是否存在第二套 Registry 或派发链；vendor 是否含 rmcp 全依赖树且干净环境 --offline 可复现；真实客户端端到端证据是否完整（docs/phase9/rmcp-client-e2e/）；RP_MCP_TOKEN 三态测试是否在；被固化的非标准断言是否已改写；生产库零写。脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验（对照 docs/phase9/MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md §5）：
          - 十项任务逐项给证据（命令与输出摘要）
          - 批次摘要：${JSON.stringify({ done: rR.done, caveats: rR.caveats, files: rR.files })}
          返回 PhaseAcceptance。`);
        for (let roundR = 1; roundR <= 2 && !accR.acceptable; roundR++) {
          await agent().ask<string>(`${DISCIPLINE}

          你是 rmcp 迁移修复员。按验收意见修复（第 ${roundR} 轮）：${JSON.stringify(accR.fixes)}`);
          const reaccR = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

          你是 rmcp 迁移复审员，复核上轮 fixes 并复测问题点，返回 PhaseAcceptance。上轮结论：${JSON.stringify(accR.items.filter((x) => !x.pass))}`);
          accR.items = reaccR.items;
          accR.acceptable = reaccR.acceptable;
          accR.fixes = reaccR.fixes;
        }
        const commitR = await agent("rmcp 迁移入库员", `${DISCIPLINE}

          你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 rmcp 迁移整改批：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db；vendor/ 与 Cargo.lock 更新必须在列）。2) git add -A，commit（中文：RIP 2.0 rmcp 迁移整改批次（MCP 协议层换官方 SDK，P0 与 P1 修复，增量 vendor）），push origin main（失败重试 2 到 3 次，仍失败如实报错不伪造）。返回 CommitResult。`);
        report({ phase: "rmcp 迁移（阻断性整改批）", status: accR.acceptable && greenR && commitR.pushed ? "完成" : "受阻", note: `commit ${commitR.hash}，验收${accR.acceptable ? "通过" : "有遗留"}，门禁${greenR ? "全绿" : "未全绿"}` }, "phases");
        log(`rmcp 迁移批 ${accR.acceptable && greenR && commitR.pushed ? "完成" : "受阻"}：commit ${commitR.hash}`);

        phase("规划 Phase 10 批次");
        const plan10 = await agent("Phase 10 规划员").ask<PhasePlan>(`${DISCIPLINE}

          你是 Phase 10 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 10 节（生态与发布）、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§12 与 §14 Phase 10、docs/SESSION-HANDOFF.md §十（backup/restore 实现、FTS 重建校验、保留归档、许可证逐包清单、axe 对比度、视觉基线）与 §十二（收尾审阅口径）、git log --oneline -10（Phase 9 commit ${commit9.hash} 与 rmcp 迁移批 commit ${commitR.hash} 刚入库）。产出 Phase 10 批次清单。硬性要求：
          1. 覆盖 ROADMAP Phase 10 全部交付：backup/restore 实现与实演、FTS 重建校验、保留归档、许可证逐包清单（Rust vendor + npm 双面）、axe 对比度修正、Playwright 视觉基线建立、autoresearch 性能基准。
          2. 成熟库优先硬约束（AGENTS.md 成熟库优先节 + 审计 §6）：每个任务书必须含七小节（成熟方案检索结果/候选比较/采用库与 feature/vendor 方案/自研边界/例外 ADR 仅当拒绝/兼容性能安全回归验收）；备份压缩、归档、许可证清单、基准等通用能力优先用成熟维护库（Rust 走增量 vendor，Python/web 走容器或项目本地）。
          3. 批次基于现状续作：Phase 7 到 9 全部交付面在库（trace 六 ID、agent-engine、workflow-engine 与画布、MCP/REST/CLI 三面同源 + parity 载具）。
          4. acceptance 对应 2.0 计划 §14 Phase 10 验收逐条可核验；每批自验 check 写明命令与预期输出；备份恢复必须真实实演（备份 → 受控破坏副本 → 恢复 → 校验一致）。
          5. UI 相关批次必须含浏览器实测与视觉基线落盘；浏览器二进制按纪律第 7 条探测。
          返回 PhasePlan（batches 2 到 4 项）。`);

        const results10: BatchResult[] = [];
        phase("逐批实施 Phase 10");
        for (let i10 = 0; i10 < plan10.batches.length; i10++) {
          const b10 = plan10.batches[i10];
          const prev10 = results10.slice(-1)[0];
          const r10 = await agent().ask<BatchResult>(`${DISCIPLINE}

          你是 Phase 10 实施员（全新上下文，每批独立以控成本）。执行批次 ${b10.id}（第 ${i10 + 1}/${plan10.batches.length} 批）：${b10.goal}
          关键路径：${b10.paths.join("、")}
          本批自验：${b10.check}
          ${prev10 ? `上一批交接（done 与 caveats）：${JSON.stringify({ done: prev10.done, caveats: prev10.caveats })}` : `这是 Phase 10 首批。基线：Phase 7/8/9 全部收口（Phase 9 commit ${commit9.hash}、rmcp 迁移批 commit ${commitR.hash} 且 CI 全绿），MCP 三面同源 + parity 载具在库，trace 六 ID 贯穿，迁移下一可用编号从 migrations/ 实际清单确认。`}
          成熟库优先：通用能力先评估成熟维护库并按需增量 vendor（七小节随批次设计文档交付），不手写已有库覆盖的通用逻辑；迁移只新增编号文件在测试副本验证；web 面不动既有页面行为。完成后返回 BatchResult：files、check、caveats。`);
          results10.push(r10);
          report({ batch: r10.batch, done: r10.done, caveats: r10.caveats.join("；") });
        }

        phase("跑 Phase 10 门禁");
        let green10 = false;
        let fail10 = "";
        for (let round10 = 1; round10 <= 3; round10++) {
          const t10g = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
          const c10g = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
          const u10g = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
          const fb10g = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
          const ft10g = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
          if (t10g.exitCode === 0 && c10g.exitCode === 0 && u10g.exitCode === 0 && fb10g.exitCode === 0 && ft10g.exitCode === 0) {
            green10 = true;
            fail10 = "";
            log(`Phase 10 五道门禁全绿（第 ${round10} 轮）`);
            break;
          }
          fail10 = [
            t10g.exitCode !== 0 ? `cargo test exit=${t10g.exitCode}\n${t10g.stdout.slice(-1000)}` : "",
            c10g.exitCode !== 0 ? `clippy exit=${c10g.exitCode}\n${c10g.stdout.slice(-1000)}` : "",
            u10g.exitCode !== 0 ? `unittest exit=${u10g.exitCode}\n${u10g.stderr.slice(-1000)}` : "",
            fb10g.exitCode !== 0 ? `npm build exit=${fb10g.exitCode}\n${fb10g.stdout.slice(-1000)}` : "",
            ft10g.exitCode !== 0 ? `npm test exit=${ft10g.exitCode}\n${ft10g.stdout.slice(-1000)}` : "",
          ].filter((s) => s !== "").join("\n----\n");
          await agent().ask<string>(`${DISCIPLINE}

          你是 Phase 10 门禁修复员（看工作树与失败输出定位修复，只修失败）。第 ${round10} 轮失败：\n${fail10}`);
        }

        phase("Phase 10 独立验收");
        const acc10 = await agent("Phase 10 验收员", `${DISCIPLINE}

          你是 Phase 10 独立验收员，立场是挑毛病而不是放行。读实现找证据并实测：备份/恢复真实实演复核（备份 → 破坏副本 → 恢复 → 校验一致的证据链）；FTS 重建校验实测；许可证逐包清单对账（Rust vendor 与 npm 双面）；axe 对比度与视觉基线落盘核验（浏览器实测：加载 browser-use:control-browser 技能，截图落 docs/phase10/browser-e2e/01/，浏览器二进制按纪律第 7 条探测：本机已知 Playwright 用户缓存 Chrome for Testing 151 于 /home/emo/.distrobox/ubuntu-home/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome，设 PW_CHROMIUM 指向即可）；autoresearch 性能基准数据复核；脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验：
          ${plan10.acceptance.map((a) => "- " + a).join("\n")}
          批次摘要：${JSON.stringify(results10.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
          另必核验 Phase 7 到 9 未回归：1) rmcp conformance 与 parity op1/op2（CI 口径）；2) trace 六 ID 关联查询双向；3) workflow 画布端到端；4) selftest_auth 11 项：bash apps/server/selftest_auth.sh。
          返回 PhaseAcceptance。`);
        for (let round10 = 1; round10 <= 2 && !acc10.acceptable; round10++) {
          await agent().ask<string>(`${DISCIPLINE}

          你是 Phase 10 修复员。按验收意见修复（第 ${round10} 轮）：${JSON.stringify(acc10.fixes)}`);
          const reacc10 = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

          你是 Phase 10 复审员，复核上轮 fixes 并复测问题点（含浏览器复测），返回 PhaseAcceptance。上轮结论：${JSON.stringify(acc10.items.filter((x) => !x.pass))}`);
          acc10.items = reacc10.items;
          acc10.acceptable = reacc10.acceptable;
          acc10.fixes = reacc10.fixes;
        }

        phase("Phase 10 入库");
        const ok10 = acc10.acceptable && green10;
        const commit10 = await agent("Phase 10 入库员", `${DISCIPLINE}

          你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 10 整段：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE10.md（总账：批次汇总、五道门禁数字、备份恢复实演证据、许可证清单与视觉基线路径引用、性能基准数字、已知取舍、回滚方式），docs/README.md 登记 Phase 10 行。3) docs/SESSION-HANDOFF.md 第三节新增 Phase 10 完成行（含 push 短哈希回填注记）。4) git add -A，commit（中文：RIP 2.0 Phase 10 生态与发布），push origin main（失败重试 2 到 3 次，仍失败如实报错不伪造）。返回 CommitResult。`);
        report({ phase: TRACKED[5], status: ok10 && commit10.pushed ? "完成" : "受阻", note: `commit ${commit10.hash}，验收${ok10 ? "通过" : "有遗留"}，门禁${green10 ? "全绿" : "未全绿"}，push ${commit10.pushed ? "成功" : "待核"}` }, "phases");
        log(`Phase 10 ${ok10 && commit10.pushed ? "完成" : "受阻"}：commit ${commit10.hash}`);

        return await hole<FinalReport>(
          "项目完成自查与收尾",
          [
            `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
            `Phase 7 结果：完成，commit 102cab5；Phase 8 结果：完成，commit ${commit8.hash}；Phase 9 结果：完成，commit ${commit9.hash}；rmcp 迁移批：commit ${commitR.hash}，验收${accR.acceptable ? "通过" : "有遗留"}；Phase 10 结果：${ok10 && commit10.pushed ? "完成" : "受阻"}，commit ${commit10.hash}，push ${commit10.pushed ? "成功" : commit10.note}`,
            `Phase 10 验收：${acc10.acceptable ? "全部通过" : JSON.stringify(acc10.items.filter((x) => !x.pass))}`,
            `批次摘要：${results10.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
            `用户已定案指令：收尾闭环删除 ChatGPT 全面审阅步骤（暂缓）；实战校验三面暂缓（项目完成并由用户启动后才做，ROE 照旧）。`,
            `继续方式：写「2.0 计划 §17 全绿自查收尾」填充：以 2.0 计划 §17 验收清单为面，自查员逐条实测（含五道门禁与 CI 口径），不合格项进修复循环（至多 3 轮），随后 GitHub 知识库与武器库补齐（三源插件 Manifest/catalog 元数据/checksums 入库、Tool Pack Containerfile/Manifest 与首批静态分析安全工具，课程原文不批量镜像、*.db 不入库）；全程无外部 AI 询问、无实战校验。全部完成后返回最终 FinalReport（2.0 全景总结：各阶段 commit、门禁数字、CI 状态、已知取舍、暂缓项清单）。`,
          ].join("\n"),
        );
      },
      );
    },
    );
  },
  );
  },
);
