# SESSION HANDOFF：跨会话交接文档（RIP 主线）

更新：2026-11-06 01:2x　用途：新会话接续执行的唯一状态源。配合自动加载的记忆索引（MEMORY.md）使用。

## 一、授权口径（用户三轮授权，已生效）

1. 项目内全部决策自主推进，无需逐步请示（2026-11-06）。
2. 目标 = 完成主计划书 /home/emo/Downloads/RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md 的全部内容（M1→M8）。
3. M8 切流为条件授权：四条硬检查全绿才切（对账 100% / 影子运行差异已解释 / 回滚演练成功 / 切换前快照完备）。
4. 每完成一个阶段目标 → commit + push 到回滚仓库（外发授权已给）。
5. 仍不动：engagements 靶标数据、runs/、knowledge.db 生产库本体（只读）、攻击证据。

## 二、回滚仓库（已建立）

- https://github.com/emoeem/research-intelligence-platform （private，gh 认证可用）
- 本地仓库根：/home/emo/code/pensting/research-platform/（main 分支）
- 结构：probe-cargo/（DEC-001 验证工件）、migrations/（M1 产出）、legacy/eri + legacy/architecture-2.0（rsync 自 security-research-upgrade/，排除 db/cache/tarball）、db-snapshot.sql（catalog 快照）
- 首次基线：a87d82b（F0/M0 基线）
- 阶段推送节奏：rsync 同步 legacy → git add -A → commit（中文，注明阶段与验证结果）→ push origin main

## 三、RIP 当前进度

- **授权补充（2026-11-06）**：可用任何技能/资源；容器隔离（docker 29.9.2 active，本机可用）；完成后清理临时内容；**质量目标 = 专业级可商业化**（这改变 M2+ 的工程标准：clippy 零警告、CI 容器可复现构建、测试金字塔、语义版本、文档与许可证完备）。
- 容器策略（用户指定 podman）：M2 起 Rust 构建与测试跑在 podman 容器（6.1.4 rootless，已验证 alpine 秒级运行；Containerfile 固化工具链+vendored 依赖）；Python Workers 同理；宿主机只留源码与产物。清理纪律：临时容器/镜像用完即删（podman rm -f / rmi），只保留 Containerfile 与 CI 配置。不用 docker。

- F0/M0：完成（13 份报告在 security-research-upgrade/architecture-2.0/）。
- M1 就绪评审：完成，判定 ready。产物在 architecture-2.0/m1/（READINESS_REVIEW / DECISION_LOG / DATA_BASELINE_SNAPSHOT / IMPLEMENTATION_PLAN）。15 条 M0 矛盾修正意见在评审报告里，实施必须遵守。
- M1 第一批实施：**后台 agent 进行中**（2026-11-06 01:1x 启动），产出 migrations/001_init.sql + etl_dryrun.py + verify.py + platform-dryrun.db + M1_DRYRUN_REPORT.md，agent 会自行推送。验收点：verify 一致率 100% 红线、幂等两次执行、只读源库。
- rtis-bridge.js：已修复（复制备份到 security-research-upgrade/rtis/tools/，MCP 检索实测恢复 ok:true）。R-02 关闭。
- 关键实测数：资源 81；D5=68（63 待人工+5 相关保留）；28 库=26 兼容/2 隔离；kb verified=130（0.88%，"13"是 M0 笔误）。
- DEC-001（Rust）：验证通过。本机 cargo 走 rsproxy.cn 镜像，在线 9.6s / vendored 离线 8.7s 构建全通（见 research-platform/PROBE-RESULT.md）。M2-A 前置已满足。
- 下一步序列：验收 M1 第一批 → M1 第二批（task JSON 字段映射、assets.db Evidence 映射、ID 冲突审计；评审指出 M0 方案缺此环节）→ M1 收尾验收报告 → M2 Rust workspace（research-platform/ 下建 crates/，重新锁版本+vendor）→ 推送 → M3-M6 隔离实现 → M7/M8 准备就绪（切流按第 3 条四检查）。

## 四、Flutterfox/Voicefox（第二项目，RIP 完成后再继续）

- 计划书：/home/emo/Downloads/flutterfox-ui-redesign-plan-v2.md；视觉基准 HTML：/home/emo/Downloads/flutterfox-desktop-ui-concept.html；工程：/home/emo/code/voicefox-gui（main @ 32b1f37）。
- Phase 0 已完成：基线截图 docs/ui-baseline-20261106/（含用户提供的 7 张）+ PHASE0-FINDINGS.md（2 个 P0 功能缺陷：duration 00:00 假进度、窄窗口播放栏被挤出视口）。
- Phase 1（设计令牌+组件画廊）：**后台 agent 进行中**，产出 lib/src/design_system/（全部新增文件），验收门：flutter analyze 零 error + gallery widget test 通过。通知到达后验收即可，不要重复实施。
- 待办：验收 Phase 1 → Phase 2（Shell 重构，见计划书 §8 各 Phase）→ integration_test 的 takeScreenshot API 需按 Flutter 3.47 修正（现版无此方法，flutter test 模式不可用，改用 flutter drive 或 golden 方案）。
- Flutter 3.47.9；应用实跑正常（release bundle 直接启动，FFI/mpv OK）。

## 五、环境事实

- 会话工作目录曾在 pensting 与 security-research-upgrade 间切换；workflow/agent 的 world cwd = 启动时 cwd。用绝对路径最稳。
- GitHub：gh 认证 emoeem（repo+workflow scope）。git user：emoeem / 3353745917@qq.com。
- 无 sudo 权限（密码不可用）；无 xdotool/wmctrl；Wayland+X11 并存；截图用 grim。
- 破折号纪律：全局 hook 拦 em/en dash，文档一律用逗号/冒号替代。

## 六、新会话开场动作（建议）

1. 读本文件 + 记忆索引。
2. 检查后台任务通知（M1 agent、Voicefox Phase 1 agent）是否已到；未到则查状态。
3. 验收 M1 第一批（跑 migrations/verify.py 复核 100%），然后按第三节序列推进。
