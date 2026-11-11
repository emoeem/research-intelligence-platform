interface FixReport {
  /** 一句话完成情况。 */
  done: string;
  /** 实际改动文件。 */
  files: string[];
  /** 验证命令与退出码清单（含 cargo test 连跑结果）。 */
  checks: string[];
  /** 未完成或让步项，无则空数组。 */
  caveats: string[];
}

interface Reacceptance {
  /** 逐条结论。 */
  items: { criterion: string; pass: boolean; evidence: string }[];
  /** 总体是否通过。 */
  acceptable: boolean;
  /** 修复意见。 */
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
  title: "RIP 2.0 收尾进度",
  key: "phase",
  status: "status",
  columns: ["待办", "进行中", "完成", "受阻"],
  detail: [{ field: "note", label: "说明" }],
});
const TRACKED = [
  "Phase 5 Web Workbench（修复与收口）",
  "Phase 6 Plugin Runtime 与 Center",
  "Phase 7 Agent Engine",
  "Phase 8 Workflow Studio",
  "Phase 9 MCP/REST/CLI 对外能力",
  "Phase 10 生态与发布",
];
for (const p of TRACKED) {
  report({ phase: p, status: "待办", note: "" }, "phases");
}
report({ phase: TRACKED[0], status: "进行中", note: "两项验收问题修复中" }, "phases");

const DISCIPLINE = [
  "通用约束：",
  "1. RIP 工程根为 /home/emo/code/pensting/research-platform（独立 git 仓库，origin 已配置 SSH）。cargo 门禁必须从仓库根执行（vendor 配置按 CWD 发现）。",
  "2. 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md 与 docs/phase0/PHASE-ROADMAP.md（冻结批次与验收命令）。",
  "3. 红线：不在宿主机安装任何系统级依赖（npm 包只进 web/ 项目目录）；不用 sudo；不修改生产 migrations/platform*.db 与 RTIS 生产行为；不推翻 Phase 0-4 已验收能力。",
  "4. 文档中文，全文禁止 em dash 与 en dash（长横线），用逗号、冒号、括号替代；文档规范见 docs/DOC-STANDARD.md。",
  "5. 结论必须来自实际读到的文件或实际执行的命令，未验证的明确标注「待验证」；无法完成时如实说明，不伪造通过。",
  "6. 复用优先（用户硬性规则）：通用逻辑一律先用成熟且仍在维护的开源库或仓库既有实现，不手写重复造轮子；需要新依赖先评估维护状态再引入并锁定版本；Rust 新依赖走增量 cargo vendor 保 --offline 构建。",
].join("\n");

const fixer = agent("Phase 5 修复员", `${DISCIPLINE}

你是 Phase 5 修复实施员。工作树已有 Phase 5 十批未提交成果（web/ React 前端五页面、apps/server 只读 API 增量、docs/phase5），保留它们，只做下述修复。独立验收 6/7 条通过，两个待修项按序完成，格式化与 clippy 零警告是底线，无法完成时如实说明。`);

phase("修复 egress flaky 与数据层迁移");
const fix = await fixer.ask<FixReport>(`任务 1（发布阻塞项：cargo test flaky）：独立验收 4/4 次全量 cargo test exit 101，挂点漂移在两处：
a) crates/tool-engine/tests/podman_host_egress.rs:375 与 471 附近，容器内 urlopen 对本地测试代理偶发 SSL UNEXPECTED_EOF_WHILE_READING（代理未就绪即被连）。
b) crates/tool-engine/src/adapter/egress_proxy.rs:680 附近，shutdown 后固定 sleep 100ms 即断言端口关闭的竞态。
修法：b) 定长 sleep 换有界轮询（循环探测直到端口拒绝连接或约 2s 超时），不引入新依赖；a) 测试代理加就绪保障（bind 后端口探活重试约 10 次 x 100ms 再放行测试请求），选最小侵入方案。在 docs/phase0/PHASE-ROADMAP.md 的 Phase 验收命令处补一行注记：cargo 门禁必须从仓库根执行（vendor 配置按 CWD 发现）。
验证：仓库根连续 5 次 cargo test --offline --workspace 全 exit 0；cargo clippy --offline --workspace --all-targets -- -D warnings exit 0。失败继续修，最多 4 轮。

任务 2（复用纪律：数据层迁移到 react-query）：@tanstack/react-query 在 web/src/App.tsx 挂了 QueryClientProvider 但 src 内零 useQuery/useMutation，取数全走手写 web/src/lib/hooks.ts（75 行），vendor-query chunk 空转。把取数层迁移到 useQuery（查询键按端点+参数，staleTime 参照现行为，Loading/Empty/Error 三态组件消费方式与页面行为保持一致），删除被取代的手写通用取数逻辑，vitest 测试同步更新（mock react-query 或真实 QueryClient 包装），全局搜索面板一并迁移。web/package.json 的 test 脚本改为 vitest run（watch 移到 test:watch）。验证：npm --prefix web run build exit 0；npm --prefix web run test exit 0。若某页迁移代价异常大，可部分完成但必须如实列出。

返回 FixReport：done 一句话、files、checks（每条命令与退出码，含 5 连跑）、caveats。`);

phase("复跑五道门禁");
let green = false;
let failText = "";
for (let round = 1; round <= 3; round++) {
  const t = await world.run("bash", ["-lc", "cd research-platform && cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
  const c = await world.run("bash", ["-lc", "cd research-platform && cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
  const u = await world.run("bash", ["-lc", "cd research-platform && python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
  const fb = await world.run("bash", ["-lc", "cd research-platform/web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
  const ft = await world.run("bash", ["-lc", "cd research-platform/web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
  if (t.exitCode === 0 && c.exitCode === 0 && u.exitCode === 0 && fb.exitCode === 0 && ft.exitCode === 0) {
    green = true;
    failText = "";
    log(`五道门禁全绿（第 ${round} 轮实测）`);
    break;
  }
  failText = [
    t.exitCode !== 0 ? `cargo test exit=${t.exitCode}\n${t.stdout.slice(-1200)}\n${t.stderr.slice(-600)}` : "",
    c.exitCode !== 0 ? `clippy exit=${c.exitCode}\n${c.stdout.slice(-1200)}\n${c.stderr.slice(-600)}` : "",
    u.exitCode !== 0 ? `unittest exit=${u.exitCode}\n${u.stderr.slice(-1200)}` : "",
    fb.exitCode !== 0 ? `npm build exit=${fb.exitCode}\n${fb.stdout.slice(-1200)}\n${fb.stderr.slice(-600)}` : "",
    ft.exitCode !== 0 ? `npm test exit=${ft.exitCode}\n${ft.stdout.slice(-1200)}\n${ft.stderr.slice(-600)}` : "",
  ].filter((s) => s !== "").join("\n----\n");
  await fixer.ask<string>(`第 ${round} 轮门禁失败，修复后说明改了什么：\n${failText}`);
}

phase("Phase 5 独立复审");
const reacc = await agent("Phase 5 复审员", `${DISCIPLINE}

你是 Phase 5 独立复审员，立场是挑毛病而不是放行。此前一轮独立验收 6/7 条通过，遗留两项已修复。只读验收加跑验证命令，不修改文件，不安装东西。`).ask<Reacceptance>(`复审 research-platform 的 Phase 5（工作树未提交状态），逐条核验：
1. 前次验收通过项未回归：六页与全局搜索只消费 rp_server.py 真实只读端点（无 mock）；路由懒加载 chunk 生效；桌面与移动断点存在且被引用；Loading/Empty/Error 三态组件统一消费；bash apps/server/selftest_auth.sh 11 项全过；node_modules 与 dist 未被 git 跟踪。
2. 修复项核验：a) egress_proxy 定长 sleep 换有界轮询且测试代理有就绪探活（读代码给 path:line）；仓库根连续 3 次 cargo test --offline --workspace 全 exit 0（复审员自己跑，脚本另行复跑，你跑 3 次即可）；b) web/src 内取数已迁移到 useQuery，手写通用取数逻辑已删除，无空转依赖（grep useQuery 计数与 vendor-query chunk 内容抽查），页面三态行为不变（读组件证据）。
3. 复用纪律全量抽查：src 内无「成熟库已有却手写」的通用逻辑残留。
返回 Reacceptance：items 每条 criterion/pass/evidence；acceptable 仅当全部通过；fixes 列修复意见。验不了的写未验证。`);

phase("Phase 5 入库");
const ok5 = reacc.acceptable && green;
const committer = agent("Phase 5 入库员", `${DISCIPLINE}

你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push。commit 消息中文且无长横线。`);
const commit = await committer.ask<CommitResult>(`在 /home/emo/code/pensting/research-platform 入库 Phase 5：
1. git status 核对改动范围（web/、apps/server/、containerfiles/ 台账、crates/ 测试修正、docs/；确认 node_modules 与 dist 未入暂存、无 *.db 文件）。
2. 用 date +%F 取实际日期，按 docs/DOC-STANDARD.md 第四节写 docs/releases/RELEASE_<当日日期>_PHASE5.md：引语（用户能感知什么：五个基础页面加全局搜索接真实后端）、元信息表（阶段交付 commit 短哈希在提交后回填、迁移无新增、五道门禁实测数字、clippy 零告警、验收结论${ok5 ? "通过" : "有遗留"}）、分章正文（页面清单与真实数据源、API 五个新增端点、复用点：focus-trap 等与 telegram-bot 同构、react-query 迁移）、已知取舍（egress flaky 修复方式、视觉回归 Playwright 待引入）、回滚方式。数字必须实测写来源。同时更新 docs/README.md 阶段工作目录表（Phase 5 行改完成加短哈希）并登记新 RELEASE 文档。
3. 更新 docs/SESSION-HANDOFF.md 第三节追加一行：Phase 5 完成（Web Workbench 基础版五页面接真实 API）、验收${ok5 ? "通过" : "有遗留"}、关键落地物一句话。
4. git add -A 后核对暂存无 node_modules、dist、*.db；commit（中文消息：RIP 2.0 Phase 5 Web Workbench 基础版）；git push origin main（origin 为 SSH，直接 push，失败重试最多 2 次并报错原文）。
返回 CommitResult（hash 用 git rev-parse --short HEAD 实取）。`);
report({ phase: TRACKED[0], status: ok5 && commit.pushed ? "完成" : "受阻", note: `commit ${commit.hash}，复审${reacc.acceptable ? "通过" : "有遗留"}，门禁${green ? "全绿" : "未全绿"}，push ${commit.pushed ? "成功" : "待核"}` }, "phases");
log(`Phase 5 ${ok5 && commit.pushed ? "完成" : "受阻"}：commit ${commit.hash}`);

return await hole<FinalReport>(
  "Phase 6 起逐阶段实施",
  [
    `Phase 5 结果：${ok5 && commit.pushed ? "完成" : "受阻"}，commit ${commit.hash}，push ${commit.pushed ? "成功" : commit.note}`,
    `复审结论：${reacc.acceptable ? "全部通过" : JSON.stringify(reacc.items.filter((x) => !x.pass))}`,
    `修复摘要：${fix.done}；caveats：${fix.caveats.join("；") || "无"}`,
    `门禁：${green ? "五道全绿（从仓库根实测）" : "未全绿：" + failText}`,
    `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 6 节（Plugin Runtime 与 Plugin Center）与 docs/phase0/PLUGIN-CONTRACT-DRAFT.md（六类插件、七态生命周期、五类来源与 plugin.lock、三级按需加载），写「实施 Phase 6」填充：同一套结构（规划员读冻结范围出批次 → 常驻实施员带复用优先规则逐批做 → 五道门禁从仓库根实测 → 独立验收员逐条给证据含复用检查 → 入库员按 DOC-STANDARD 写当日 RELEASE_PHASE6 并登记 docs/README.md 与 SESSION-HANDOFF），验收要点含至少接入一个外部知识插件与一个工具插件、插件启停不影响核心数据库、元数据与完整内容按需加载；结尾留 Phase 7 尾洞。`,
  ].join("\n"), async () => {
  interface BrowserFinding {
    /** 页面或功能点。 */
    where: string;
    /** 一句话问题。 */
    what: string;
    /** high = 功能不可用或数据错误；low = 体验问题。 */
    severity: "high" | "low";
  }

  interface BrowserAudit {
    /** 逐页实测结果。 */
    pages: { page: string; pass: boolean; evidence: string }[];
    /** 发现的问题清单，空数组表示全部通过。 */
    issues: BrowserFinding[];
    /** 截图与证据文件路径清单（仓库相对路径）。 */
    evidence: string[];
    /** 总体是否通过。 */
    pass: boolean;
  }

  interface PhaseBatch {
    /** 批次编号，如 P6-1。 */
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

  report({ phase: TRACKED[1], status: "进行中", note: "浏览器实测前置 + Plugin Runtime/Center + 武器库扩展路径" }, "phases");

  phase("Phase 5 浏览器实测");
  const browserAudit = await agent("浏览器实测员", `${DISCIPLINE}

  你是浏览器实测员。用户硬性要求：凡浏览器端开发必须用真实浏览器亲自逐页逐功能点查看，不以代码检查与构建通过替代。先调用 Skill 工具加载 browser-use:control-browser 技能（如需 GUI 测试流程再加载 browser-use:web-gui-tester），按技能流程用 node_repl 驱动真实浏览器。不改代码，只实测与记录；起服用测试副本数据库，绝不写生产 migrations/platform*.db；截图与证据存 research-platform/docs/phase5/browser-e2e/（文件名英文，不含敏感 token，鉴权用测试 token 且不把 token 写进截图文本）。`).ask<BrowserAudit>(`对 Phase 5 Web Workbench 做真实浏览器端到端实测：
  1. 读 research-platform/web/README.md 与 docs/phase5/PHASE5-PLAN.md 探明启动方式（rp_server 测试实例挂开发库副本 + vite dev 或构建产物预览），起真实服务。
  2. 用真实浏览器逐页打开并逐功能点核验：Overview（KPI 数值与 curl /api/overview 对账）、Knowledge（搜索、过滤、详情）、Tool Center（目录列表、20 工具、Schema 详情渲染、治理过滤）、Runs（运行与状态）、Runtime、Projects；⌘K 全局搜索（输入 rtis 看工具/知识/任务分组命中）；每页的 Loading/Empty/Error 三态（断服一次看 Error 面板与重试）；移动视口（390px 宽）抽屉导航与布局。
  3. 每页留至少一张截图到 docs/phase5/browser-e2e/，evidence 记路径。
  4. 发现的任何问题（数据错、交互断、样式崩、控制台报错）逐条记入 issues（severity：high = 功能不可用或数据错误，low = 体验问题）。
  返回 BrowserAudit。`);
  report({ page: "browser-e2e", issues: browserAudit.issues.length, pass: browserAudit.pass });
  let browserGreen = browserAudit.pass;
  if (!browserAudit.pass) {
    for (let round = 1; round <= 2 && !browserGreen; round++) {
      await fixer.ask<string>(`浏览器实测发现以下问题，修复后说明改了什么（不改测试副本数据库，前端问题只动 web/ 与必要时 apps/server）：${JSON.stringify(browserAudit.issues)}`);
      const recheck = await agent().ask<BrowserAudit>(`你是浏览器复测员（${DISCIPLINE}）。用真实浏览器（先加载 browser-use:control-browser 技能）只复测以下问题点是否已修复，返回 BrowserAudit（pages 可只列复测项）：${JSON.stringify(browserAudit.issues)}`);
      browserAudit.issues = recheck.issues;
      browserGreen = recheck.pass;
      if (recheck.evidence.length > 0) browserAudit.evidence.push(...recheck.evidence);
    }
  }
  log(browserGreen ? "浏览器实测通过（全部页面与功能点）" : `浏览器实测遗留 ${browserAudit.issues.length} 个问题，如实带入后续`);
  if (browserAudit.issues.some((i) => i.severity === "high")) {
    await fixer.ask<string>(`浏览器实测仍有 high 级问题未解决：${JSON.stringify(browserAudit.issues.filter((i) => i.severity === "high"))}。继续修复并自验，返回改动文件与验证命令清单。`);
  }

  phase("规划 Phase 6 批次");
  const plan6 = await agent("Phase 6 规划员").ask<PhasePlan>(`${DISCIPLINE}

  你是 Phase 6 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 6 节（Plugin Runtime 与 Plugin Center 冻结批次与验收）、docs/phase0/PLUGIN-CONTRACT-DRAFT.md 全文（六类插件、统一 Manifest 加 capability 注册、七态生命周期加禁用/卸载/升级语义、五类来源与 plugin.lock 版本固定、三层依赖兼容检查、rip-data/plugins 目录布局、三级按需加载）、git log --oneline -6（Phase 5 现状：web/ 五页面、API 九端点、tool-registry/tool-engine 已在位），产出可执行批次清单。硬性要求：
  1. 范围沿 ROADMAP Phase 6 冻结范围，不新增范围。
  2. 必须包含两个用户硬性交付：a) docs/guides/ADDING-A-TOOL.md 武器库扩展指南加脚手架命令（新增工具 = Tool Manifest 写好放进对应 pack 或 python worker + 脚手架命令生成骨架 + tool-registry 校验注册 + 健康检查 + Tool Center 页面自动露出，一条线可复制；脚手架脚本放 scripts/，stdlib 实现，复用既有校验器）；b) Plugin Center 网页（web/ 新增 Plugins 页：已安装/可发现/禁用分类、Manifest 详情、启停操作走真实 API）。
  3. acceptance 逐条摘录 2.0 计划 §14 Phase 6 与 ROADMAP 的验收要点，改写为可核验陈述，必须含：至少接入一个外部知识插件与一个工具插件（真实落位 rip-data/plugins/installed）、插件启停不影响核心数据库（启停前后核对生产库零写入口径）、元数据与完整内容按需加载（三级加载实测）、扩展路径端到端（用指南加脚手架实际添加一个新工具并从 Registry 到 Tool Center 露出）。
  4. 工程约束写进批次 check：Rust 新 crate 遵循分层与依赖方向、新依赖增量 cargo vendor；迁移只新增编号文件在测试副本验证；web 改动保持既有页面行为不回归。
  返回 PhasePlan。`);

  phase("逐批实施 Phase 6");
  const impl6Agent = agent("Phase 6 实施员", `${DISCIPLINE}

  你是 Phase 6 实施员，负责把 Phase 6 全部批次做成可验收的落地物：Plugin Runtime（crate 层：Manifest 解析校验复用 tool-registry 既有校验器、生命周期状态机、来源拉取与 plugin.lock、依赖兼容检查、三级按需加载）、插件接入（一个外部知识插件加一个工具插件真实落位 rip-data/plugins/）、Plugin Center 网页、武器库扩展指南与脚手架。迁移与 Rust 面不动生产库；web 面不动既有页面行为；格式化与 clippy 零警告是底线；无法完成时如实说明，不伪造通过。`);
  const results6: BatchResult[] = [];
  for (let i = 0; i < plan6.batches.length; i++) {
    const b = plan6.batches[i];
    const r = await impl6Agent.ask<BatchResult>(`实施批次 ${b.id}（第 ${i + 1}/${plan6.batches.length} 批）：${b.goal}
  关键路径：${b.paths.join("、")}
  本批自验：${b.check}
  完成自验后返回 BatchResult：files 列实际改动文件，check 写自验命令与退出码，caveats 写未完成或让步项（无则空数组）。`);
    results6.push(r);
    report({ batch: r.batch, done: r.done, caveats: r.caveats.join("；") });
  }

  phase("跑 Phase 6 门禁");
  let green6 = false;
  let fail6 = "";
  for (let round = 1; round <= 3; round++) {
    const t = await world.run("bash", ["-lc", "cd research-platform && cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
    const c = await world.run("bash", ["-lc", "cd research-platform && cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
    const u = await world.run("bash", ["-lc", "cd research-platform && python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
    const fb = await world.run("bash", ["-lc", "cd research-platform/web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
    const ft = await world.run("bash", ["-lc", "cd research-platform/web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
    if (t.exitCode === 0 && c.exitCode === 0 && u.exitCode === 0 && fb.exitCode === 0 && ft.exitCode === 0) {
      green6 = true;
      fail6 = "";
      log(`Phase 6 五道门禁全绿（第 ${round} 轮）`);
      break;
    }
    fail6 = [
      t.exitCode !== 0 ? `cargo test exit=${t.exitCode}\n${t.stdout.slice(-1200)}` : "",
      c.exitCode !== 0 ? `clippy exit=${c.exitCode}\n${c.stdout.slice(-1200)}` : "",
      u.exitCode !== 0 ? `unittest exit=${u.exitCode}\n${u.stderr.slice(-1200)}` : "",
      fb.exitCode !== 0 ? `npm build exit=${fb.exitCode}\n${fb.stdout.slice(-1200)}` : "",
      ft.exitCode !== 0 ? `npm test exit=${ft.exitCode}\n${ft.stdout.slice(-1200)}` : "",
    ].filter((s) => s !== "").join("\n----\n");
    await impl6Agent.ask<string>(`第 ${round} 轮门禁失败，修复后说明改了什么：\n${fail6}`);
  }

  phase("Phase 6 独立验收");
  const accAgent6 = agent("Phase 6 验收员", `${DISCIPLINE}

  你是 Phase 6 独立验收员，立场是挑毛病而不是放行。读实现找证据并做实测（含浏览器实测：先加载 browser-use:control-browser 技能驱动真实浏览器核验 Plugin Center 页面与扩展路径露出）；脚本已跑五道门禁，你不必重跑全套。`);
  let acc6 = await accAgent6.ask<PhaseAcceptance>(`对 Phase 6 逐条核验：
  ${plan6.acceptance.map((a) => "- " + a).join("\n")}
  批次完成情况：${JSON.stringify(results6.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
  另必做三项：
  1. 武器库扩展路径端到端（用户硬性要求）：按 docs/guides/ADDING-A-TOOL.md 与脚手架命令实际添加一个新工具（建议 stdlib 实现的 rip.http.tls_inspect 或 rip.net.dns_lookup，放 webresearch pack 或等价），走完 Manifest 生成、校验、注册、健康检查、Tool Center 露出全流程，把每步命令与输出记入 evidence；走不通就是 fail。
  2. 浏览器实测 Plugin Center 页面（列表、Manifest 详情、启停操作后状态刷新）与 Tool Center 露出新工具，留截图路径到 docs/phase6/browser-e2e/。
  3. 复用检查：通用逻辑有无手写重复（Manifest 校验是否复用 tool-registry 既有校验器、前端取数是否走 react-query、脚手架是否复用既有 schema），违规标 pass=false 给换库方案。
  返回 PhaseAcceptance。`);
  for (let round = 1; round <= 2 && !acc6.acceptable; round++) {
    await impl6Agent.ask<string>(`按验收意见修复（第 ${round} 轮）：${JSON.stringify(acc6.fixes)}`);
    acc6 = await accAgent6.ask<PhaseAcceptance>(`第 ${round} 轮修复完成。复核全部验收要点（重点复验上轮 fail 项），返回 PhaseAcceptance。上轮结论：${JSON.stringify(acc6.items.filter((x) => !x.pass))}`);
  }

  phase("Phase 6 入库");
  const ok6 = acc6.acceptable && green6 && browserGreen;
  const committer6 = agent("Phase 6 入库员", `${DISCIPLINE}

  你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push。commit 消息中文且无长横线。`);
  const commit6r = await committer6.ask<CommitResult>(`在 /home/emo/code/pensting/research-platform 入库 Phase 6（含浏览器实测证据与可能的 Phase 5 浏览器修复）：
  1. git status 核对范围（crates/、rip-data/plugins/、web/、scripts/、docs/、containerfiles/ 台账；无 node_modules、dist、*.db 入暂存）。
  2. date +%F 取日期，按 docs/DOC-STANDARD.md 第四节写 docs/releases/RELEASE_<当日日期>_PHASE6.md：引语、元信息表（commit 短哈希提交后回填、迁移情况、五道门禁实测数字、浏览器实测结论）、分章正文（Plugin Runtime 交付、两插件接入、Plugin Center 页面、武器库扩展指南与端到端实测记录）、已知取舍、回滚方式。docs/README.md 登记 Phase 6 行与 guides 目录。
  3. docs/SESSION-HANDOFF.md 第三节追加 Phase 6 完成行；第八节追加一行武器库扩展路径交付注记（指南路径与脚手架命令、端到端实测结论）。
  4. git add -A 核对暂存；commit（中文消息：RIP 2.0 Phase 6 Plugin Runtime 与 Center 含武器库扩展路径）；git push origin main（SSH，失败重试最多 2 次）。
  返回 CommitResult（hash 用 git rev-parse --short HEAD 实取）。`);
  report({ phase: TRACKED[1], status: ok6 && commit6r.pushed ? "完成" : "受阻", note: `commit ${commit6r.hash}，验收${ok6 ? "通过" : "有遗留"}，门禁${green6 ? "全绿" : "未全绿"}，push ${commit6r.pushed ? "成功" : "待核"}` }, "phases");
  log(`Phase 6 ${ok6 && commit6r.pushed ? "完成" : "受阻"}：commit ${commit6r.hash}`);

  return await hole<FinalReport>(
    "Phase 7 起逐阶段实施",
    [
      `Phase 6 结果：${ok6 && commit6r.pushed ? "完成" : "受阻"}，commit ${commit6r.hash}，push ${commit6r.pushed ? "成功" : commit6r.note}`,
      `验收结论：${acc6.acceptable ? "全部通过" : JSON.stringify(acc6.items.filter((x) => !x.pass))}`,
      `浏览器实测：${browserGreen ? "通过" : "遗留：" + JSON.stringify(browserAudit.issues)}`,
      `批次摘要：${results6.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
      `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 7 节（Agent Engine）与 2.0 计划 §9，写「实施 Phase 7」填充：同一套结构（规划员 → 常驻实施员带复用优先 → 五道门禁 → 独立验收含浏览器实测与复用检查 → 入库 RELEASE_PHASE7）；Agent Engine 验收要点：Agent 能独立查询知识（经 Context Broker）并调用 Tool Engine 工具、调用可追踪（AgentRun/ToolRun 关联）、结果回到任务上下文形成 Evidence、暂停/恢复/取消与运行轨迹；结尾留 Phase 8 尾洞。`,
    ].join("\n"), async () => {
    interface SkBatch {
      /** 批次编号 SK-1/SK-2/SK-3。 */
      id: string;
      goal: string;
      paths: string[];
      check: string;
    }

    interface SkPlan {
      batches: SkBatch[];
      /** 对照计划 §10 验收清单的可核验陈述。 */
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

    interface PhasePlan {
      batches: SkBatch[];
      acceptance: string[];
      risk: string;
    }

    interface CommitResult {
      hash: string;
      pushed: boolean;
      note: string;
    }

    report({ phase: TRACKED[1], status: "进行中", note: "安全知识源扩展 SK-1 到 SK-3（用户提供计划）" }, "phases");

    phase("规划安全知识源扩展");
    const skPlan = await agent("SK 扩展规划员").ask<SkPlan>(`${DISCIPLINE}

    你是 SK 扩展规划员。读 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md 全文（用户提供的安全知识源接入计划，§8 建议批次 P6-9/10/11 在本工作流中改称 SK-1/2/3，因 P6-9 编号已被武器库指南批次占用）、docs/phase0/PHASE-ROADMAP.md Phase 6 节、docs/phase6/PHASE6-PLAN.md 与 git log --oneline -8（Phase 6 已落地：plugin-registry/plugin-runtime/010 迁移/rip-data/Catalog/Plugin Center/ADDING-A-TOOL 指南），核对计划 §2 的缺口判断是否仍成立（ingest.rs CONTENT_KINDS、context-broker PluginSource、search-engine 索引面），产出 SK-1/2/3 批次清单。要求：
    1. SK-1 通用摄取接线严格按计划 §8 P6-9 七步（ADR 先行、Manifest/Schema 扩展三内容类型、Ingest 通用化保 RTIS 回归、Parser 扩展、Search 接线、Context Broker PluginSource 实装、Evidence 固化），paths 给具体文件。
    2. SK-2 三源插件按计划 §3/§5/§8 P6-10：三个独立 Manifest（固定计划 §3 核实的 Commit：awesome-hacking 440486f、secskills 1867eb6、tongui 课程 45a12a7）、只摄取 catalog_entry 元数据、tongui 无 LICENSE 不批量镜像正文、去重与站点/仓库合并、Plugin Center 与 Knowledge 工作区展示来源与许可状态。
    3. SK-3 按计划 §6/§8 P6-11：rip.knowledge.catalog.extract 与 rip.security.extension.audit（T0）、rip.knowledge.document.fetch 与 rip.knowledge.source.sync（T1，走既有 Egress 白名单与受控 Ingest API）、候选映射 Resource、恶意 fixture 拒绝测试。
    4. acceptance 逐条对应计划 §10 完整验收清单（15 条），改写为可核验陈述；RTIS 旧链 15,155 条对账不回归是硬红线。
    5. 每批自验 check 写明命令与预期输出。
    返回 SkPlan。`);

    phase("实施 SK-1 通用摄取接线");
    const sk1 = await agent().ask<BatchResult>(`${DISCIPLINE}

    你是 SK-1 实施员（全新上下文，动手前先读 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §4/§8 与 docs/phase6/PHASE6-PLAN.md 了解现状）。执行 SK 扩展规划员给出的 SK-1 批次：
    ${JSON.stringify(skPlan.batches[0])}
    关键约束：ADR 落 docs/adr/；三内容类型走 Schema 与 Domain 新实体，不放宽旧 CHECK 了事（新迁移 + 旧类型保留）；RTIS 旧插件行为与计数零回归（15,155 条对账测试必须过）；Context Broker PluginSource 从空实现改为实装（L1 元数据/L2 摘要/L3 按需正文 + 访问控制）；Search 接线只把受治理条目入可重建派生索引；Evidence 固化引用片段加内容哈希加来源位置。不建平行知识库。返回 BatchResult：files、check（命令与退出码）、caveats。`);

    phase("实施 SK-2 三源插件接入");
    const sk2 = await agent().ask<BatchResult>(`${DISCIPLINE}

    你是 SK-2 实施员（全新上下文，先读 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §3/§5/§7 与 SK-1 交接：${JSON.stringify({ done: sk1.done, files: sk1.files.slice(0, 15), caveats: sk1.caveats })}）。执行 SK-2 批次：
    ${JSON.stringify(skPlan.batches[1])}
    关键约束：三个 Manifest 用 schemas/plugin-manifest/v1.json 完整校验并按计划 §3 固定 Commit；只摄取目录项元数据（catalog_entry）与 skill_candidate 候选记录，外链项目不 clone 不安装不执行；tongui 课程无 LICENSE，只登记章节/标题/URL 元数据，正文不批量持久化；站点与仓库同文章合并为一个逻辑资源双 source_ref；去重用规范化 canonical_url 加内容哈希；Plugin Center 与 Knowledge 页展示来源、分类、许可证、快照 Commit、审核状态（浏览器可验）。返回 BatchResult。`);

    phase("实施 SK-3 审查工具与目录抽取");
    const sk3 = await agent().ask<BatchResult>(`${DISCIPLINE}

    你是 SK-3 实施员（全新上下文，先读计划 §6 与 SK-1/SK-2 交接：${JSON.stringify([{ done: sk1.done, caveats: sk1.caveats }, { done: sk2.done, caveats: sk2.caveats }])}）。执行 SK-3 批次：
    ${JSON.stringify(skPlan.batches[2])}
    关键约束：四个工具按计划 §6.2 契约（输入输出 Schema、T0/T1 权限标注）；catalog.extract 只解析不访问链接、拒绝危险 scheme；security.extension.audit 纯静态、不执行被审查物、恶意 fixture（浮动分支/无许可证/安装钩子/越权 Shell/私网 URL）有拒绝测试；document.fetch 只接受 Registry 已批准 canonical URL，域名 allowlist 加私网检查加重定向逐跳复核，禁止 localhost/RFC1918/metadata 端点，默认 GET 无凭据；source.sync 走受控 Ingest API 不直写 SQLite，支持 dry-run 与幂等键；候选 Skill/MCP/Tool 映射为 Resource 候选，首批 Tool Pack 只收静态分析类。返回 BatchResult。`);

    phase("SK 扩展门禁");
    let skGreen = false;
    let skFail = "";
    for (let round = 1; round <= 3; round++) {
      const t = await world.run("bash", ["-lc", "cd research-platform && cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
      const c = await world.run("bash", ["-lc", "cd research-platform && cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
      const u = await world.run("bash", ["-lc", "cd research-platform && python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
      const fb = await world.run("bash", ["-lc", "cd research-platform/web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
      const ft = await world.run("bash", ["-lc", "cd research-platform/web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
      if (t.exitCode === 0 && c.exitCode === 0 && u.exitCode === 0 && fb.exitCode === 0 && ft.exitCode === 0) {
        skGreen = true;
        skFail = "";
        log(`SK 扩展五道门禁全绿（第 ${round} 轮）`);
        break;
      }
      skFail = [
        t.exitCode !== 0 ? `cargo test exit=${t.exitCode}\n${t.stdout.slice(-1000)}` : "",
        c.exitCode !== 0 ? `clippy exit=${c.exitCode}\n${c.stdout.slice(-1000)}` : "",
        u.exitCode !== 0 ? `unittest exit=${u.exitCode}\n${u.stderr.slice(-1000)}` : "",
        fb.exitCode !== 0 ? `npm build exit=${fb.exitCode}\n${fb.stdout.slice(-1000)}` : "",
        ft.exitCode !== 0 ? `npm test exit=${ft.exitCode}\n${ft.stdout.slice(-1000)}` : "",
      ].filter((s) => s !== "").join("\n----\n");
      await agent().ask<string>(`${DISCIPLINE}

    你是 SK 扩展门禁修复员（全新上下文，先看工作树与失败输出定位归属模块再修）。第 ${round} 轮门禁失败，修复后说明改了什么：\n${skFail}`);
    }

    phase("SK 扩展验收");
    let skAcc = await agent("SK 扩展验收员", `${DISCIPLINE}

    你是 SK 扩展独立验收员，立场是挑毛病而不是放行。读实现找证据并实测（含浏览器实测 Plugin Center 与 Knowledge 页的三源展示：先加载 browser-use:control-browser 技能）；脚本已跑五道门禁，不必重跑全套。`).ask<PhaseAcceptance>(`对照 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §10 十五条验收清单逐条核验，重点：
    - 三源有独立 Source ID 与固定 Commit；许可状态明确（tongui 未批量持久化正文）
    - 三内容类型过 Schema 校验；RTIS 旧摄取对账无回归（跑对账测试取实际数字）
    - Search 可搜审核通过条目；Context Broker PluginSource 非空且 L1/L2/L3 可用
    - Evidence 含内容哈希与来源位置；T1 工具私网/重定向拒绝测试真实存在
    - 浏览器实测：Plugin Center 显示三插件与许可状态，Knowledge 检索能命中目录条目并跳转原始来源，留截图路径到 docs/phase6/browser-e2e/
    - 复用检查：Git 下载/校验/索引是否复用既有 Registry/Search/Egress，违规给整改
    批次摘要：SK-1:${sk1.done}；SK-2:${sk2.done}；SK-3:${sk3.done}；caveats 合并：${JSON.stringify([sk1.caveats, sk2.caveats, sk3.caveats].flat())}
    返回 PhaseAcceptance。`);
    for (let round = 1; round <= 2 && !skAcc.acceptable; round++) {
      await agent().ask<string>(`${DISCIPLINE}

    你是 SK 扩展修复员。按验收意见修复（第 ${round} 轮）：${JSON.stringify(skAcc.fixes)}`);
      skAcc = await agent().ask<PhaseAcceptance>(`${DISCIPLINE}

    你是 SK 扩展复审员，只复核上轮 fixes 是否解决且未回归，重点复跑对账与浏览器复测问题点，返回 PhaseAcceptance。上轮结论：${JSON.stringify(skAcc.items.filter((x) => !x.pass))}`);
    }
    const skOk = skAcc.acceptable && skGreen;
    const skCommit = await agent("SK 扩展入库员", `${DISCIPLINE}

    你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 SK 扩展：1) git status 核对范围（crates/、migrations/、rip-data/、docs/、schemas/；无 *.db、node_modules 入暂存）。2) 按 docs/DOC-STANDARD.md 写 docs/releases/RELEASE_<date +%F>_SK-SECURITY-SOURCES.md（引语、元信息表、三批正文、RTIS 对账数字、已知取舍、回滚方式），docs/README.md 登记。3) docs/SESSION-HANDOFF.md 第三节追加 SK 扩展完成行。4) git add -A 核对后 commit（中文：RIP 安全知识源接入 SK-1 到 SK-3），push origin main（SSH，重试最多 2 次）。返回 CommitResult。`);
    report({ phase: TRACKED[1], status: skOk && skCommit.pushed ? "完成" : "受阻", note: `SK 扩展 commit ${skCommit.hash}，验收${skOk ? "通过" : "有遗留"}` }, "phases");
    log(`SK 扩展 ${skOk && skCommit.pushed ? "完成" : "受阻"}：commit ${skCommit.hash}`);

    report({ phase: TRACKED[2], status: "进行中", note: "Agent Engine + WebUI 壳层升级首批，每批新实施员模式" }, "phases");
    phase("规划 Phase 7 批次");
    const plan7 = await agent("Phase 7 规划员").ask<PhasePlan>(`${DISCIPLINE}

    你是 Phase 7 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 7 节、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§9 与 §14 Phase 7、docs/SESSION-HANDOFF.md §十（trace 关联与研究能力契约化验收）与 §十一（WebUI 工作台目标形态：五区布局、十一工作区、Workspace/Project/Agent/Workflow 概念分离）、git log --oneline -5，产出批次清单。硬性要求：
    1. 首批必须是 WebUI 壳层升级：五区布局（顶部栏/多标签中央区/右 Inspector/底部 Activity）加 workspaces 纯数据模型（Workspace 是界面组织、Project 是研究对象、Agent 是执行主体、Workflow 是执行过程，数据模型与 UI 分离，参照 telegram-bot workspaces.ts 模式），Plugin Center 与既有页面迁移进新壳。
    2. Agent Engine 批次：Agent Definition 与能力绑定、Context Broker 集成（SK 扩展后的 PluginSource 可用）、Tool Engine 调用、Task/Evidence/Research Memory（Context Memory 雏形：可追溯研究成果，与聊天记忆分离）、暂停/恢复/取消/重试、AgentRun 持久化与运行轨迹。
    3. trace 关联批次：trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id 统一并可查询（跨 Rust/Python/Web）。
    4. 研究能力契约化批次：Research Planner/Source Discovery/Evidence Extraction/Verification/Synthesis/Citation/Evaluator 七能力以输入输出契约 Schema 加 mock 实现加测试交付，不是 prompt。
    5. Agent Studio 页落新壳（浏览器实测项）。acceptance 对应 2.0 计划 §14 Phase 7 验收加 §十/§十一追加项。返回 PhasePlan（batches 沿用 SkBatch 形状，id 用 P7-1 起编）。`);

    const results7: BatchResult[] = [];
    phase("逐批实施 Phase 7");
    for (let i = 0; i < plan7.batches.length; i++) {
      const b = plan7.batches[i];
      const prev = results7.slice(-1)[0];
      const r = await agent().ask<BatchResult>(`${DISCIPLINE}

    你是 Phase 7 实施员（全新上下文，每批独立以控成本）。执行批次 ${b.id}（第 ${i + 1}/${plan7.batches.length} 批）：${b.goal}
    关键路径：${b.paths.join("、")}
    本批自验：${b.check}
    ${prev ? `上一批交接（done 与 caveats）：${JSON.stringify({ done: prev.done, caveats: prev.caveats })}` : "这是首批。"}
    复用优先：通用逻辑用成熟库或仓库既有实现；迁移只新增编号文件在测试副本验证；web 面不动既有页面行为。完成后返回 BatchResult：files、check、caveats。`);
      results7.push(r);
      report({ batch: r.batch, done: r.done, caveats: r.caveats.join("；") });
    }

    phase("跑 Phase 7 门禁");
    let green7 = false;
    let fail7 = "";
    for (let round = 1; round <= 3; round++) {
      const t = await world.run("bash", ["-lc", "cd research-platform && cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
      const c = await world.run("bash", ["-lc", "cd research-platform && cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
      const u = await world.run("bash", ["-lc", "cd research-platform && python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
      const fb = await world.run("bash", ["-lc", "cd research-platform/web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
      const ft = await world.run("bash", ["-lc", "cd research-platform/web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
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

    你是 Phase 7 独立验收员，立场是挑毛病而不是放行。读实现找证据并实测（浏览器实测新壳五区布局与 Agent Studio：先加载 browser-use:control-browser 技能，留截图到 docs/phase7/browser-e2e/）；脚本已跑五道门禁。`).ask<PhaseAcceptance>(`逐条核验：
    ${plan7.acceptance.map((a) => "- " + a).join("\n")}
    批次摘要：${JSON.stringify(results7.map((r) => ({ batch: r.batch, done: r.done, caveats: r.caveats })))}
    重点实测：1) Agent 端到端：创建研究任务让内置 Agent 经 Context Broker 查知识（含 SK 三源条目）并调用 Tool Engine 真实工具，AgentRun/ToolRun/Artifact/Evidence 关联可查（trace_id 贯穿）；2) 暂停/恢复/取消真实生效；3) 新壳五区布局与多标签、Workspace/Project/Agent/Workflow 概念在数据模型与 UI 分离（读 workspaces 模型与 API 证据）；4) 研究能力契约有 Schema 有测试非 prompt；5) 复用检查与旧页面不回归（selftest_auth 11 项）。
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

    你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 7：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE7.md（含五道门禁数字、trace 关联说明、新壳截图路径引用、已知取舍、回滚方式），docs/README.md 登记 Phase 7 行。3) docs/SESSION-HANDOFF.md 第三节追加 Phase 7 完成行、第十一节标注壳层升级落地。4) git add -A，commit（中文：RIP 2.0 Phase 7 Agent Engine 与工作台壳层升级），push origin main（SSH，重试最多 2 次）。返回 CommitResult。`);
    report({ phase: TRACKED[2], status: ok7 && commit7.pushed ? "完成" : "受阻", note: `commit ${commit7.hash}，验收${ok7 ? "通过" : "有遗留"}，门禁${green7 ? "全绿" : "未全绿"}` }, "phases");
    log(`Phase 7 ${ok7 && commit7.pushed ? "完成" : "受阻"}：commit ${commit7.hash}`);

    return await hole<FinalReport>(
      "Phase 8 起逐阶段实施",
      [
        `SK 扩展：${skOk ? "完成" : "受阻"}（commit ${skCommit.hash}，验收${skOk ? "通过" : JSON.stringify(skAcc.items.filter((x) => !x.pass))}）`,
        `Phase 7 结果：${ok7 && commit7.pushed ? "完成" : "受阻"}，commit ${commit7.hash}，push ${commit7.pushed ? "成功" : commit7.note}`,
        `Phase 7 验收：${acc7.acceptable ? "全部通过" : JSON.stringify(acc7.items.filter((x) => !x.pass))}`,
        `批次摘要：${results7.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
        `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 8 节（Workflow Studio）与 2.0 计划 §10.4 G、SESSION-HANDOFF §十（持久化/checkpoint/恢复验收，参考 LangGraph 与 Temporal 设计不替换 Rust 自研）。写「实施 Phase 8」填充：规划员 → 每批新实施员（含 XYFlow 画布批次与浏览器实测画布交互）→ 五道门禁 → 独立验收（多节点工作流编辑运行、暂停恢复、失败重试、版本管理，浏览器实测）→ 入库 RELEASE_PHASE8；结尾留 Phase 9 尾洞。`,
      ].join("\n"),
    );
  },
  );
},
);