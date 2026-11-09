# 更新说明 · 2026-11-09（RIP 2.0 Phase 0：只读审计与架构冻结）

> RIP 2.0 程序正式启动：八份只读审计交付物落盘，冻结 Phase 1 到 10 的依赖顺序与验收命令，全程未改任何源代码与生产数据。本阶段的价值是让后续每个阶段都有可核验的现状基线，避免边猜边建。

| 项         | 值                                                          |
| ---------- | ----------------------------------------------------------- |
| 阶段交付   | git `cc9dcfb`（19:12 CST）                                  |
| 数据库迁移 | 无新增；全程只读，生产 platform.db 与 RTIS 行为零写入（盘点员实测核对） |
| 自动化测试 | 未执行新门禁（只读审计，无代码改动）；既有基线复核：worker unittest 125 全绿（跳过 11） |
| 静态检查   | 八份文档 em/en dash grep 零命中（`grep -rP '\x{2014}|\x{2013}'` exit 1） |
| 独立评审   | 通过（评审员对照 2.0 计划 §14/§16 抽查仓库事实，零遗留问题） |

## 一、交付了什么

1. **CURRENT-ARCHITECTURE-INVENTORY.md**（224 行）：6 个 Rust crates、3 个 Python Worker、apps/server（4 API 加 Token 鉴权加 systemd）、integrations/rtis 两层、Schema V1 到 V4、CI 与两份 Containerfile 逐一盘点，每项给保留/扩展/替换结论，另列 7 条与 2.0 目标的差距。
2. **TOOL-INVENTORY.md**（238 行）：识别 12 组可登记资产（7 直接登记、3 包装 Adapter、1 待许可证核实、1 协议底座）与 8 项淘汰建议；关键结论全部实跑验证（ERI 离线套件 ALL PASS、RTIS 双入口只读冒烟通过）。
3. **TELEGRAM-ADMIN-UI-REUSE.md**（250 行）：telegram-bot/admin 三级复用矩阵（直接借鉴/解耦后复用/不复用），逐模块给路径、耦合点与工作量级，全部结论带 path:line 出处；文末附「复用前必读」文档清单（2026-11-09 用户要求补入）。
4. **TOOL-RUNTIME-ADR.md**（275 行，Nygard 格式）：裁决两步走拓扑，起步为宿主机 rootless Podman 分容器（过渡 Runtime 类型 podman_host），目标拓扑（专用 VM 内 Podman）不推翻并写明迁移充分条件与三条提前触发条件。依据为当夜实测：podman 6.1.4 rootless、无 sudo（`sudo -n true` exit 1）、qemu/libvirt 会话级可用、KVM 0666、磁盘可用 259G。
5. **TOOL-CONTRACT-DRAFT.md**（993 行）：Tool Manifest v1 字段规范、ToolRun 状态机（13 条跃迁表加协作式取消）、ToolCallRequest/ToolRunResult 协议、9 类标准错误、健康检查域；两个示例 Manifest；附录 A 可直接落为 JSON Schema；4 个 JSON 块与 2 个 YAML 块实跑解析通过。
6. **PLUGIN-CONTRACT-DRAFT.md**（472 行）：七种 capability 类型、七态生命周期加五个辅助态、五类来源与 plugin.lock 版本固定、三级按需加载、rip-data/plugins 目录布局。
7. **CONTEXT-BROKER-DESIGN.md**：L0 到 L3 四级上下文、五步意图识别与加权匹配、分项 Token 预算与降级驱逐、ContextManifest 三表、内容寻址缓存与七类失效触发器。
8. **PHASE-ROADMAP.md**（737 行，15 节）：冻结主干 Phase 1 到 2 到 3 到 6 到 7 到 8 到 9 到 10，三条并行车道（Context Broker Mock 先行、Web 壳层先行、宿主容器通道先行）与两条提前通道（P9-2 rip.run_tool、P7-1 数据面）；Phase 2 固化 9 条硬前置（PC2-1 到 PC2-9）；裁决 005 迁移编号冲突（先到先得）。

## 二、为什么这样做

- 审计先行不改代码：2.0 计划 §14 Phase 0 的验收就是「没有未经批准的业务代码改动」，全部产出是文档与实测证据。
- 两步走拓扑而非硬上 VM：本机无 sudo 且 quickemu 缺件，专用 VM 当下不可落地；宿主 rootless Podman 已实测可用（rootless true、subuid/subgid emo:100000:65536），先用起来，VM 迁移条件写在 ADR 复审节。

## 三、已知取舍

- license 轴数据源未接（M8 遗留，低优先）。
- rtis.py 自测 13 过 4 失败（T2/T4/T11/T13）为既有状态，非本阶段引入，留待清账工作流。
- TELEGRAM-ADMIN-UI-REUSE 矩阵审计时点早于 telegram-bot 11-08 HANDOVER，editor/ 相关结论以 HANDOVER 为准（矩阵文末已注明）。

## 四、回滚方式

纯文档提交，`git revert cc9dcfb` 即可完全回退，无数据与代码影响。
