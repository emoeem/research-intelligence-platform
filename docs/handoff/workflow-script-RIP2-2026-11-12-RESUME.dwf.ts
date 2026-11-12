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
    ].join("\n"),
  );
  },
);
