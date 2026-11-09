# SESSION HANDOFF：跨会话交接文档（RIP 主线）

更新：2026-11-09　用途：新会话接续执行的唯一状态源。配合自动加载的记忆索引（MEMORY.md）使用。

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
- **M8 切换已执行（2026-11-06，eff2918）**：RTIS_RESOLVER_MODE 默认 rp + platform.db 生产库 + 双路拆词修复。Flutterfox 授权更新：后续（含 Phase 7/8 移动端）不受 Rust 约束，能用非 Rust 方案完成即可。
- 下一步序列：M1 第一批 → 第二批 → M2-M8 全部完成（见 git log a87d82b..eff2918）。
- **RIP 主线收口（2026-11-06 23:5x 复核）**：M8 切换后重跑 verify.py，PASS 一致率 100% exit 0。
- **尾巴进展（2026-11-07 凌晨）**：CI 流水线已建（7d84c89 + gitignore vendor 修复 08f178d，Actions 全绿：Rust 69 测试+clippy 零警告+worker 109 测试）。M1 全量批已交付（f1de6ae + 评审修复 6da1d5e）：evidence 26 兼容库全量 700 行、research_task 大 JSON 126/126、verify 扩展 203 项 100% PASS、幂等 no-op、源库零写入；platform.db = fullbatch 生产批；M8_READINESS 限制 1/2 关闭。注意：platform-fullbatch.db 含凭据类证据，已移出版库（gitignore），评审时抓到。
- **剩余尾巴**：license 轴数据源（低优先）。真实 embedding 已收口（ae705b3 + 守卫评审修复 2ffd635）：bge-small-zh-v1.5 ONNX（ModelScope，94.8MB，sha256 校验），512 维显式协商，回退语义 backend 字段，125 测试双口径验证；**至此"彻底部署上线"四缺口（M1 全量批 / CI / M7 鉴权服务化 / 真实 embedding）全部关闭**，工作台本机常驻 active 且生产数据在线。M7 细节：08846d7（鉴权 11 项自验 + systemd user 服务）+ e5abdf1（harness flake 根治）。
- **RIP 2.0 程序启动（2026-11-09）**：计划书 docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md（架构规划稿，基于本机仓库只读检查，未动源码与生产库）。Phase 0 只读审计完成，八份交付物在 docs/phase0/：CURRENT-ARCHITECTURE-INVENTORY、TOOL-INVENTORY、TOOL-RUNTIME-ADR、TELEGRAM-ADMIN-UI-REUSE、PLUGIN-CONTRACT-DRAFT、TOOL-CONTRACT-DRAFT、CONTEXT-BROKER-DESIGN、PHASE-ROADMAP。后续按 docs/phase0/PHASE-ROADMAP.md 的依赖冻结顺序推进（Phase 1 Tool Contract 与 Registry 起步，详见该文件依赖图与阶段边界）。

## 四、Flutterfox/Voicefox（第二项目，Phase 0-8 完成，v0.9.0 已发布）

- 计划书：/home/emo/Downloads/flutterfox-ui-redesign-plan-v2.md；工程：/home/emo/code/voicefox-gui（main，远程 flutterfox/main，最新 a0cfbf4）。
- **Phase 0-8 全部完成并推送**：Phase 7（c2c72fa）MobileShell/平板三态/手势全屏播放，批次 0 补齐桌面冻结门禁截图（桌面逐像素 RMSE=0 零回归）；Phase 8（525f3d2）Rust 转发型 PlayerBackend + FRB 反向通道 + jniLibs + just_audio/audio_service。主会话验收：analyze 0 issue、Dart 189 + Rust 813 测试全过。报告：docs/PHASE7-REPORT.md、docs/PHASE8-REPORT.md。
- **Release v0.9.0 已发布**（github.com/emoeem/flutterfox/releases/tag/v0.9.0）：Flutter 四平台产物（Linux/Android 全链路验证 + Windows/macOS CI 实跑通过）+ TUI 五平台资产同挂一个 Release。CI 全绿（fmt/clippy 零警告门禁修复 7e33e74 + flutter test 门禁增补）；release-flutter 管线落地（a0cfbf4）。踩坑记录：.cargo/config.toml 本机硬编码 NDK 路径已去本地化（本机配置外置 ~/.cargo）、Flutter 3.47 移除 --no-codesign、runner 镜像变量不进 env 上下文。
- 剩余：真机验证（通知栏/锁屏/耳机键/蓝牙路由，需真机）、iOS 批次（无工具链未启动）、Windows/macOS 产物尝试矩阵转正。
- Flutter 3.47.9；桌面实跑正常（FFI/mpv OK）。
- **toolbox-hub 联动（2026-11-07）**：`~/.config/toolbox-hub/tools/rip-*` 四个脚本（verify 对账红线 / status 部署健康 / ci 双仓状态 / service 服务控制）经 MCP 网关可被 AI 调用；授权在 `~/.config/toolbox-hub/mcp.toml`（rip-verify/status/ci 在 allow，network 仅这三个 id 可用）。

## 五、环境事实

- 会话工作目录曾在 pensting 与 security-research-upgrade 间切换；workflow/agent 的 world cwd = 启动时 cwd。用绝对路径最稳。
- GitHub：gh 认证 emoeem（repo+workflow scope）。git user：emoeem / 3353745917@qq.com。
- 无 sudo 权限（密码不可用）；无 xdotool/wmctrl；Wayland+X11 并存；截图用 grim。
- 破折号纪律：全局 hook 拦 em/en dash，文档一律用逗号/冒号替代。

## 六、新会话开场动作（建议）

1. 读本文件 + 记忆索引。
2. RIP 主线已收口（第三节）；RIP 2.0 已启动（2026-11-09），Phase 0 完成，按 docs/phase0/PHASE-ROADMAP.md 推进；Flutterfox 桌面重构已完成（第四节），剩余项均在各节列明。
3. 未决决策（用户定）：RIP 尾巴（CI/鉴权/evidence 迁移等）与 Flutterfox Phase 7/8（移动端）的优先级；实机截图需桌面空闲。
