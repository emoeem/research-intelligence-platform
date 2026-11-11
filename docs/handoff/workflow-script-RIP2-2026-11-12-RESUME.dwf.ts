// RIP 2.0 接手续作工作流（重建版，2026-11-12）
// 由原活体脚本 docs/handoff/workflow-script-RIP2-2026-11-12.dwf.ts 改编：
// 1. Phase 5 与 Phase 6 已完成入库（c0a4284/6217ca8、49b9377/ace3c53），原实施段改为只读核对；
// 2. SK-1 半成品已在 WIP 备份 commit fec5a3f，SK 批次定位为补齐半成品；
// 3. 门禁命令按本运行 cwd（即仓库根）修正，去掉原脚本的 cd research-platform 前缀；
// 4. push 已在仓库配 core.sshCommand 指向宿主密钥，子代理直接 push 即可。
// 队列来源：docs/HANDOVER-RIP2-CONTINUATION.md 第二节与第三节。

interface VerifyReport {
  /** 逐条核对结论。 */
  items: { criterion: string; pass: boolean; evidence: string }[];
  /** 全部核对通过。 */
  ok: boolean;
  /** 接手方需要注意的偏差或缺口，无则空数组。 */
  gaps: string[];
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
  title: "RIP 2.0 接手续作进度",
  key: "phase",
  status: "status",
  columns: ["待办", "进行中", "完成", "受阻"],
  detail: [{ field: "note", label: "说明" }],
});
const TRACKED = [
  "Phase 5 Web Workbench（基础版已入库）",
  "Phase 6 Plugin Runtime 与 Center（基础已入库）",
  "Phase 7 Agent Engine",
  "Phase 8 Workflow Studio",
  "Phase 9 MCP/REST/CLI 对外能力",
  "Phase 10 生态与发布",
];
report({ phase: TRACKED[0], status: "完成", note: "commit c0a4284/6217ca8，验收通过（2026-11-11 入库）" }, "phases");
report({ phase: TRACKED[1], status: "完成", note: "commit 49b9377/ace3c53，验收通过（2026-11-12 入库）" }, "phases");

const DISCIPLINE = [
  "通用约束：",
  "1. RIP 工程根为 /home/emo/code/pensting/research-platform（独立 git 仓库，origin 已配置 SSH，仓库已配 core.sshCommand 指向宿主密钥 /home/emo/.ssh/id_ed25519，push 直接执行即可）。cargo 门禁必须从仓库根执行（vendor 配置按 CWD 发现），本运行的工作目录就是仓库根。",
  "2. 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md 与 docs/phase0/PHASE-ROADMAP.md（冻结批次与验收命令）。",
  "3. 红线：不在宿主机安装任何系统级依赖（npm 包只进 web/ 项目目录）；不用 sudo；不修改生产 migrations/platform*.db 与 RTIS 生产行为；不推翻 Phase 0-6 已验收能力。",
  "4. 文档中文，全文禁止 em dash 与 en dash（长横线），用逗号、冒号、括号替代；文档规范见 docs/DOC-STANDARD.md。",
  "5. 结论必须来自实际读到的文件或实际执行的命令，未验证的明确标注「待验证」；无法完成时如实说明，不伪造通过。",
  "6. 复用优先（用户硬性规则）：通用逻辑一律先用成熟且仍在维护的开源库或仓库既有实现，不手写重复造轮子；需要新依赖先评估维护状态再引入并锁定版本；Rust 新依赖走增量 cargo vendor 保 --offline 构建。",
].join("\n");

phase("核对已完成批次状态");
const verified = await agent("入库核对员", `${DISCIPLINE}

你是只读核对员：不改任何文件，不重跑 cargo/npm 门禁（已完成批次不重跑，门禁在后续 SK 与 Phase 7 批次后照常全量执行），只用 git 与文档核对。如果某条核对做不了，如实写未验证，不猜。`).ask<VerifyReport>(`接手核对：Phase 5 与 Phase 6 已由前序工作流完成入库，本工作流从 SK 扩展与 Phase 7 接续。逐条核对并给证据：
1. git log --oneline -12 可见 Phase 5（c0a4284、6217ca8）与 Phase 6（49b9377、ace3c53）入库；git status -sb 与 origin/main 同步，无未提交源码改动。
2. docs/releases/RELEASE_2026-11-11_PHASE5.md 与 docs/releases/RELEASE_2026-11-12_PHASE6.md 存在且含元信息表与五道门禁实测数字。
3. docs/README.md 已登记 Phase 5/6 行；docs/SESSION-HANDOFF.md 第三节有 Phase 5/6 完成行，第八节有武器库扩展路径交付注记。
4. 浏览器实测证据在位：docs/phase5/browser-e2e/ 与 docs/phase6/browser-e2e/（后者不少于 7 张截图）。
5. 武器库扩展路径交付物在位：docs/guides/ADDING-A-TOOL.md、scripts/rip_new_tool.py、docs/phase6/PHASE6-ACCEPTANCE.md。
6. SK-1 半成品在位：git show --stat fec5a3f 应见 crates/domain/src/knowledge.rs、crates/plugin-runtime/src/knowledge_parse.rs、crates/plugin-runtime/src/knowledge_review.rs、crates/plugin-runtime/tests/sk1_extended_ingest.rs、migrations/011_knowledge_sources.sql、docs/phase6/SK1-INGEST-ADR.md 等文件。
返回 VerifyReport：items 每条 criterion/pass/evidence；ok 仅当全部通过；gaps 列出核对中发现的任何偏差（含文档互相矛盾处）。`);
log(`接手核对${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`);

return await hole<FinalReport>(
  "SK 扩展与 Phase 7 实施",
  [
    `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
    `基线：Phase 5（c0a4284/6217ca8）与 Phase 6（49b9377/ace3c53）已入库；SK-1 半成品 fec5a3f；本运行 cwd 即仓库根，门禁命令不带 cd 前缀直接执行`,
    `本洞内容：SK-1 补齐半成品 → SK-2 三源插件 → SK-3 审查工具 → 五道门禁 → 独立验收（RTIS 15,155 条对账不回归是硬红线）→ 入库 → Phase 7（壳层升级首批 + Agent Engine + trace 关联 + 研究能力契约化）→ 门禁 → 验收 → 入库 → 留 Phase 8 尾洞`,
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

  report({ phase: TRACKED[1], status: "进行中", note: "SK 扩展（P6-9 到 P6-11）实施中，SK-1 有半成品 fec5a3f" }, "phases");

  phase("规划安全知识源扩展");
  const skPlan = await agent("SK 扩展规划员").ask<SkPlan>(`${DISCIPLINE}

  你是 SK 扩展规划员。读 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md 全文（用户提供的安全知识源接入计划，§8 建议批次 P6-9/10/11 在本工作流中改称 SK-1/2/3，因 P6-9 编号已被武器库指南批次占用）、docs/phase6/PHASE6-PLAN.md 与 git log --oneline -8（Phase 6 已落地：plugin-registry/plugin-runtime/010 迁移/rip-data/Catalog/Plugin Center/ADDING-A-TOOL 指南），并 git show --stat fec5a3f 盘点 SK-1 半成品（knowledge 域模型、knowledge_parse/knowledge_review、011 迁移、SK1-INGEST-ADR、扩展摄取测试与 context-broker/search-engine 接线改动），核对计划 §2 的缺口判断哪些已被半成品覆盖、哪些仍成立，产出 SK-1/2/3 批次清单。要求：
  1. SK-1 批次定位为「补齐半成品」：严格按计划 §8 P6-9 七步（ADR 先行、Manifest/Schema 扩展三内容类型、Ingest 通用化保 RTIS 回归、Parser 扩展、Search 接线、Context Broker PluginSource 实装、Evidence 固化）对照半成品逐项标注已完成/待完成，paths 给具体文件。
  2. SK-2 三源插件按计划 §3/§5/§8 P6-10：三个独立 Manifest（固定计划 §3 核实的 Commit：awesome-hacking 440486f、secskills 1867eb6、tongui 课程 45a12a7）、只摄取 catalog_entry 元数据、tongui 无 LICENSE 不批量镜像正文、去重与站点/仓库合并、Plugin Center 与 Knowledge 工作区展示来源与许可状态。
  3. SK-3 按计划 §6/§8 P6-11：rip.knowledge.catalog.extract 与 rip.security.extension.audit（T0）、rip.knowledge.document.fetch 与 rip.knowledge.source.sync（T1，走既有 Egress 白名单与受控 Ingest API）、候选映射 Resource、恶意 fixture 拒绝测试。
  4. acceptance 逐条对应计划 §10 完整验收清单（15 条），改写为可核验陈述；RTIS 旧链 15,155 条对账不回归是硬红线。
  5. 每批自验 check 写明命令与预期输出。
  返回 SkPlan。`);

  phase("补齐 SK-1 通用摄取接线");
  const sk1 = await agent().ask<BatchResult>(`${DISCIPLINE}

  你是 SK-1 实施员（全新上下文，动手前先读 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §4/§8 与 docs/phase6/PHASE6-PLAN.md 了解现状，并盘点工作树中已有的 SK-1 半成品（WIP 备份 commit fec5a3f：crates/domain/src/knowledge.rs、crates/plugin-runtime/src/knowledge_parse.rs、knowledge_review.rs、tests/sk1_extended_ingest.rs、migrations/011_knowledge_sources.sql、docs/adr/ 与 docs/phase6/SK1-INGEST-ADR.md、context-broker PluginSource 与 search-engine 接线改动）。先对照计划 §8 P6-9 七步标注半成品已完成与缺失项，再补齐缺失项完成整批，已有实现与计划一致就不推翻）。执行 SK 扩展规划员给出的 SK-1 批次：
  ${JSON.stringify(skPlan.batches[0])}
  关键约束：ADR 落 docs/adr/（半成品已起草就核对补齐）；三内容类型走 Schema 与 Domain 新实体，不放宽旧 CHECK 了事（新迁移 + 旧类型保留，半成品已有 011 迁移就核对补齐）；RTIS 旧插件行为与计数零回归（15,155 条对账测试必须过）；Context Broker PluginSource 从空实现改为实装（L1 元数据/L2 摘要/L3 按需正文 + 访问控制）；Search 接线只把受治理条目入可重建派生索引；Evidence 固化引用片段加内容哈希加来源位置。不建平行知识库。返回 BatchResult：files、check（命令与退出码）、caveats。`);

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
    const t = await world.run("bash", ["-lc", "cargo test --offline --workspace"], { timeoutMs: 900000, output: { tail: 8192 } });
    const c = await world.run("bash", ["-lc", "cargo clippy --offline --workspace --all-targets -- -D warnings"], { timeoutMs: 900000, output: { tail: 8192 } });
    const u = await world.run("bash", ["-lc", "python3 -m unittest discover -s workers/python/tests"], { timeoutMs: 300000, output: { tail: 4096 } });
    const fb = await world.run("bash", ["-lc", "cd web && npm run build"], { timeoutMs: 600000, output: { tail: 8192 } });
    const ft = await world.run("bash", ["-lc", "cd web && npm run test"], { timeoutMs: 600000, output: { tail: 8192 } });
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

  phase("SK 扩展入库");
  const skCommit = await agent("SK 扩展入库员", `${DISCIPLINE}

  你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 SK 扩展：1) git status 核对范围（crates/、migrations/、rip-data/、docs/、schemas/；无 *.db、node_modules 入暂存）。2) 按 docs/DOC-STANDARD.md 写 docs/releases/RELEASE_<date +%F>_SK-SECURITY-SOURCES.md（引语、元信息表、三批正文、RTIS 对账数字、已知取舍、回滚方式），docs/README.md 登记。3) docs/SESSION-HANDOFF.md 第三节追加 SK 扩展完成行。4) git add -A 核对后 commit（中文：RIP 安全知识源接入 SK-1 到 SK-3），push origin main（仓库已配 core.sshCommand 指向宿主密钥，直接 push，重试最多 2 次，仍失败如实报告错误原文）。返回 CommitResult。`);
  report({ phase: TRACKED[1], status: skOk && skCommit.pushed ? "完成" : "受阻", note: `基础 49b9377/ace3c53 + SK 扩展 commit ${skCommit.hash}，验收${skOk ? "通过" : "有遗留"}` }, "phases");
  log(`SK 扩展 ${skOk && skCommit.pushed ? "完成" : "受阻"}：commit ${skCommit.hash}`);

  report({ phase: TRACKED[2], status: "进行中", note: "Agent Engine + WebUI 壳层升级首批，每批新实施员模式" }, "phases");
  phase("规划 Phase 7 批次");
  const plan7 = await agent("Phase 7 规划员").ask<PhasePlan>(`${DISCIPLINE}

  你是 Phase 7 规划员。读 docs/phase0/PHASE-ROADMAP.md 的 Phase 7 节、2.0 计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）§9 与 §14 Phase 7、docs/SESSION-HANDOFF.md §十（trace 关联与研究能力契约化验收）与 §十一（WebUI 工作台目标形态：五区布局、十一工作区、Workspace/Project/Agent/Workflow 概念分离）、git log --oneline -5，产出批次清单。硬性要求：
  1. 首批必须是 WebUI 壳层升级：五区布局（顶部栏/多标签中央区/右 Inspector/底部 Activity）加 workspaces 纯数据模型（Workspace 是界面组织、Project 是研究对象、Agent 是执行主体、Workflow 是执行过程，数据模型与 UI 分离，参照 telegram-bot workspaces.ts 模式，telegram-bot 工程在 /home/emo/code/telegram-bot，读其 admin/README.md 与 workspaces 实现前先读 docs/HANDOVER.md 结论），Plugin Center 与既有页面迁移进新壳。
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

  你是入库员：只做 add/commit/push，禁止 reset/clean/checkout/rebase/force push，commit 消息中文无长横线。`).ask<CommitResult>(`入库 Phase 7：1) git status 核对范围与暂存卫生（无 node_modules、dist、*.db）。2) 按 DOC-STANDARD 写 docs/releases/RELEASE_<date +%F>_PHASE7.md（含五道门禁数字、trace 关联说明、新壳截图路径引用、已知取舍、回滚方式），docs/README.md 登记 Phase 7 行。3) docs/SESSION-HANDOFF.md 第三节追加 Phase 7 完成行、第十一节标注壳层升级落地。4) git add -A，commit（中文：RIP 2.0 Phase 7 Agent Engine 与工作台壳层升级），push origin main（仓库已配 core.sshCommand 指向宿主密钥，直接 push，重试最多 2 次，仍失败如实报告错误原文）。返回 CommitResult。`);
  report({ phase: TRACKED[2], status: ok7 && commit7.pushed ? "完成" : "受阻", note: `commit ${commit7.hash}，验收${ok7 ? "通过" : "有遗留"}，门禁${green7 ? "全绿" : "未全绿"}` }, "phases");
  log(`Phase 7 ${ok7 && commit7.pushed ? "完成" : "受阻"}：commit ${commit7.hash}`);

  return await hole<FinalReport>(
    "Phase 8 起逐阶段实施",
    [
      `接手核对：${verified.ok ? "通过" : "有偏差：" + JSON.stringify(verified.gaps)}`,
      `SK 扩展：${skOk ? "完成" : "受阻"}（commit ${skCommit.hash}，验收${skOk ? "通过" : JSON.stringify(skAcc.items.filter((x) => !x.pass))}）`,
      `Phase 7 结果：${ok7 && commit7.pushed ? "完成" : "受阻"}，commit ${commit7.hash}，push ${commit7.pushed ? "成功" : commit7.note}`,
      `Phase 7 验收：${acc7.acceptable ? "全部通过" : JSON.stringify(acc7.items.filter((x) => !x.pass))}`,
      `批次摘要：${results7.map((r) => `${r.batch}:${r.done}${r.caveats.length > 0 ? "（caveats：" + r.caveats.join("；") + "）" : ""}`).join("；")}`,
      `继续方式：读 docs/phase0/PHASE-ROADMAP.md 的 Phase 8 节（Workflow Studio）与 2.0 计划 §10.4 G、SESSION-HANDOFF §十（持久化/checkpoint/恢复验收，参考 LangGraph 与 Temporal 设计不替换 Rust 自研）。写「实施 Phase 8」填充：规划员 → 每批新实施员（含 XYFlow 画布批次与浏览器实测画布交互）→ 五道门禁 → 独立验收（多节点工作流编辑运行、暂停恢复、失败重试、版本管理，浏览器实测）→ 入库 RELEASE_PHASE8；结尾留 Phase 9 尾洞。`,
    ].join("\n"),
  );
  },
);
