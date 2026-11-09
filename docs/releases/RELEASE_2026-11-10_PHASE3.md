# 更新说明 · 2026-11-10（RIP 2.0 Phase 3：首批 Tool Packs）

> 工具从「通道验证负载」升级为可用工具：Web Research 与 Core Utilities 两包共 6 个真实容器化工具完成登记、经 Tool Gateway 全链派发跑通并归档 Artifact；出站白名单代理落地为 podman_host 的网络面执行机制；Source Analysis、Document、Mobile Analysis 三包与 TPscan、Browser Automation 按「数量让位于可靠」裁定落为 planned 存根，解锁条件随存根写死。

| 项         | 值                                                  |
| ---------- | --------------------------------------------------- |
| 阶段交付   | 本发布提交（批次 P3-0 到 P3-6 全部交付物一次入库；前序基线 f64ee24） |
| 数据库迁移 | 无新增（PHASE3-DESIGN §2.4 零迁移声明兑现；git status migrations/ 空；生产 platform*.db 零写入，库操作走开发库副本文件库） |
| 自动化测试 | cargo test 207 全绿（Phase 2 基线 194 + 13 新用例）；worker unittest 125 全绿（跳过 11） |
| 静态检查   | cargo clippy `-D warnings` 零告警；cargo fmt --check 通过 |

## 一、面向使用者的变化

1. **首批可调用工具**：rip.http.inspect（T1 只读 GET/HEAD 检查，出站经白名单代理）与 rip.core.file_meta / rip.core.hash / rip.core.compress / rip.core.convert / rip.core.validate（T1 本地文件变换，network none）。六工具全部 podman_host 类型，Manifest json/yaml 双份在 containerfiles/ 两族目录，image digest 固定并三处对账（Manifest、podman inspect、containerfiles/README.md 台账）。
2. **端到端链路**：Gateway::dispatch（真实 podman 探针心跳）→ podman_host Adapter 容器执行 → out/ 产物 sha256 登记 artifact 表，全程有测试载具（web_research_e2e、core_utils_e2e）与开发库副本只读对账。
3. **网络面新能力**：Manifest network.mode=egress_allowlist 真实生效，出站经 run 生命周期内的本机 CONNECT 白名单代理（egress_proxy，std::net 零新依赖），目标 host 从 input.url 派生临时 allowlist，跨 host 重定向被拒（npmjs.com 实证）；白名单为代理层策略执行，硬隔离属 podman_vm 目标拓扑（ADR §8 已知代价）。
4. **planned 登记面**：schemas/pack-plan/v1.json + manifests/packs/planned/ 五存根（source-analysis、document、mobile-analysis、tpscan-p37、browser-automation-p38），只写能力点、信任级裁定与解锁条件，不进 Registry 不建镜像。

## 二、修复与根因

1. **工具错误帧两路可见**：Adapter 非零退出路径的信封消息取自 stderr，而错误帧原只走 stdout；两工具脚本补 stderr 镜像（worker 惯例），根因是 P2 语义（stdout 帧只承载成功路径）。
2. **null 字段判格式违规**：validate 对列表 json 输出 top_keys=null 撞 output_schema integer 声明；根因是「不适用字段」以 null 表达；修复为不适用即不输出（core_tools.py 统一剔除 None 值）。
3. **硬编码网络面**：Adapter 原 --network none 单值硬编码；修复为按 Manifest network.mode 分派（pasta:-T 端口转发 + 代理 env），本机 pasta/slirp4netns 可用性实测冻结（slirp4netns 不在位，宿主接口 IP 直连变体被 pasta 拒）。

## 三、已知取舍

1. 数量让位于可靠（计划 §14:784 原文）：三包 + TPscan + Browser Automation 为 planned 存根，解锁条件见各存根 exit_conditions；TPscan 等许可证核实与 VM 通道，Browser 落 VM 面。
2. Phase 2 遗留顺延：rip-gateway systemd 常驻编排、引擎层 submit_key 唯一性加固未触碰（PHASE3-DESIGN §3.2 承接表）。
3. 测试对公共端点的依赖：example.com（GET 正例）与 npmjs.com（跨 host 重定向负例触发面）；端点行为变化时按测试头注更换。
4. coreutils 镜像 tag 为 latest（按批次构建命令原文），digest 固定面由 Manifest 与台账承担；换机重建按台账流程重取。

## 四、回滚方式

- 代码与文档：各批次独立 commit，`git revert <批次 commit>` 撤销；TOOL-CONTRACT 占位回填与 V-10 标注随 P3-3 commit 回滚即恢复原文。
- 运行时面：`podman rmi $(podman images -q --filter label=rip.runtime=rip-tool)` 清两 pack 镜像（base 为共用层不动，用户资产不触碰）；开发库副本 /tmp/rip-p33-devcopy、/tmp/rip-p34-* 整目录删除即清净。
- 生产库零写入：无生产回滚面（红线）；manifests/packs/planned/ 存根随 commit 回滚。

## 五、涉及回滚的关联文档

- 验收记录：docs/phase3/PHASE3-ACCEPTANCE.md（ROADMAP §4.4 四步命令原文与输出实录）。
- 设计冻结：docs/phase3/PHASE3-DESIGN.md（§5.3 网络面实测冻结、§6 批次表与执行偏差记录）。
- 契约回填：docs/phase0/TOOL-CONTRACT-DRAFT.md §9.1/§9.3/§11.1；docs/phase0/PHASE-ROADMAP.md V-10 关闭标注。
