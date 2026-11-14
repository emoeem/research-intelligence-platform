# containerfiles：RIP 工具镜像族（P2-7 起按族管理）

本目录承载 RIP 工具执行面的镜像族定义。每族一目录，构建全部在容器内完成（宿主机零安装，镜像属运行时数据，ADR §3.2 口径）；仓库只保留 Containerfile 与配置，镜像与缓存不入库。

## 目录结构与镜像族拆分

| 目录 | 镜像 tag | 族定位 | 状态 |
|---|---|---|---|
| `rip-tool-base/` | `localhost/rip-tool-base:smoke` | podman_host 通道冒烟载具（rip.smoke.native，utility 族） | P2-5 起在用 |
| `rip-tool-webresearch/` | `localhost/rip-tool-webresearch:latest` | Web Research Pack（计划 §14:774，首工具 rip.http.inspect，纯标准库 http_inspector.py） | P3-1 建成（2026-11-10） |
| `rip-tool-coreutils/` | `localhost/rip-tool-coreutils:latest` | Core Utilities Pack（计划 §14:774，五工具共用入口 core_tools.py：file_meta/hash/compress/convert/validate，argv op 分派，纯标准库） | P3-1 建成；P3-4 起五 Manifest 登记 |
| `rip-tool-source/` | `localhost/rip-tool-source:latest` | Source Analysis Pack（收尾武器库批交付，pack_id source-analysis 冻结存根落地，三工具共用入口 src_tools.py：deps/static-check/search，argv op 分派，纯标准库） | 2026-11-14 建成（收尾武器库批，三 Manifest 登记） |
| `rip-tool-document/` | （planned） | Document Pack | planned（PHASE3-DESIGN §2.3 范围裁定，不建镜像） |

拆分原则（计划 §5.3：不为全部工具造一个巨型容器）：按 Tool Pack 拆族，异构依赖（Python/Node/Java/Go 各版本）留在各族镜像内；每族一目录，含 `Containerfile`、工具本体脚本、`tool-manifest.yaml` 与 `tool-manifest.json` 双份（json 供测试与 Registry 加载，yaml 为人读口径，内容一致）。

## digest 固定规则

1. **基础镜像 digest 固定**：`FROM` 一律 `...@sha256:<64hex>`（本仓库现状：`python:3.12-slim@sha256:4840d514c583a4d3d8cad3940e68dee6f0f6f3635c12fcd0a3d74c21a5b93a54`，见 rip-tool-base/Containerfile）。升级基础镜像 = 换 digest + 全链重建 + 重新验证。
2. **产物镜像 digest 登记**：构建后取真实 manifest digest 并三处同步（同一值，缺一即失配）：

   ```bash
   podman build -t localhost/<family>:<tag> -f containerfiles/<family>/Containerfile containerfiles/<family>/
   podman inspect localhost/<family>:<tag> --format '{{.Digest}}'
   #   → sha256:<64hex>，同步以下三处：
   #   a. containerfiles/<family>/tool-manifest.yaml  runtime.image 与 provenance.image_digest
   #   b. containerfiles/<family>/tool-manifest.json  同上两处
   #   c. crates/tool-engine/tests/podman_host_e2e.rs 的 MANIFEST_DIGEST 常量
   ```

3. **登记面强制**：Registry 校验九条第 4 条（TOOL-CONTRACT 2.5）拒绝 tag 引用与 `<digest>` 占位；Adapter 在 PREPARING 二道校验（`PodmanHostAdapter::image_ref_ok`）。注意：本地构建的 digest 不可跨主机字节级复现，换机重建后按本节流程重取并同步。

## digest 台账（产物镜像实测登记）

构建后按上节流程实取，逐镜像登记；Manifest 注册（P3-2/P3-3 批次）引用本表值。

| 镜像 | digest（podman inspect 实测） | 构建日期 | 批次 |
|---|---|---|---|
| `localhost/rip-tool-base:smoke` | `sha256:1dbc67084d5c1a1f6c31720752e64730da3524a83b5ba3efba5dadb9589cbe60` | 2026-11-11 重建（P2-5 首建；2026-11-10 夜间外部清理致三族镜像丢失，按 §31 行流程重取同步，原值 84877bf6 见 git 历史） | P2-5 / P5-10 重建 |
| `localhost/rip-tool-webresearch:latest` | `sha256:085b4ef304e990be18d8f6aa917ec0c04c33223486d2f971544493176e288c58` | 2026-11-11 重建（同上事件；原值 bf910119 见 git 历史） | P3-1 / P5-10 重建 |
| `localhost/rip-tool-coreutils:latest` | `sha256:49436f71b66974631ffc4ea7ac805c95bfa07d4d5fb00423c3d5c7ea9b18acaf` | 2026-11-11 重建（同上事件；原值 c90631b9 见 git 历史） | P3-1 / P5-10 重建 |
| `localhost/rip-tool-source:latest` | `sha256:8251ba32e6ae4d107190f2b2279ae3774facdc8e074ae000266be3aa1fd0ea29` | 2026-11-14 建成（基础镜像 digest 同 coreutils 行，受控拉取后离线构建；distrobox 侧经 distrobox-host-exec shim 调宿主 podman 6.2.0 构建） | 收尾武器库批 |
| `localhost/rip-pg17:latest` | `sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45` | 2026-11-14 建成（RIP3 情报核 PG 17.11：基座 paradedb/paradedb:pg17@sha256:7cb29053… 受控拉取 + pg_jieba 多阶段构建（third_party/ 内置源码含 cppjieba 子模块，零构建期网络），vector/pg_search/pg_jieba 三扩展实测在位，jiebacfg 中文分词实测正确） | RIP3 P1-1 |

构建口径（P3-1 实测）：两族镜像内零包安装（工具本体 Python 纯标准库，基础镜像自带能力），构建全程离线（基础镜像 digest 已在本地 containers-storage）；构建期宿主 `pacman -Q` 基线 2415 行无变化。工具本体功能在容器内验证（hash 与宿主 sha256sum 对账一致、gzip 回读一致、json 转 csv、filemeta、YAML 显式不支持负向、路径穿越拒绝负向、GET example.com 200 且 body sha256 宿主对账一致、无网与非 GET 拒绝负向）。

## 缓存策略（P2-7 定稿）

- **Adapter 只探不拉**：podman_host Adapter 在 PREPARING 以 `podman image exists` 探测本地 containers-storage，命中即用；缺失按 DEPENDENCY 失败（第 7 行），**从不执行 `podman pull` 或 `podman build`**。镜像预置（拉取/构建）属部署与登记流程，不在 run 关键路径上。
- **缓存命中实测**：`crates/tool-engine/tests/podman_host_budget.rs` 用 wrapper 脚本记账 podman 子命令，同一镜像两次派发 = 两次 `run` + 两次 `image exists`、零 `pull`、零 `build`。
- **任务容器用后即删**：`--rm` 常规回收；kill 路径补 `podman rm -f` 兜底；套件结束断言 `podman ps -a --filter label=rip.runtime=rip-tool` 前缀内清零。
- **镜像与缓存总量预算 50 GiB**（ADR §7「镜像与缓存总量」行）：`podman system df` 定期复核；超预算先清悬空镜像与孤儿容器再评审（ADR §6 磁盘自愈口径）；不低于阈值不自动删任务工作区。
- **构建缓存**：`podman build` 层缓存属本地存储，同一基础镜像多族共享底层（README 拆分原则的缓存收益）；`podman system df` 的 Build Cache 列纳入同一 50 GiB 预算。

## 资源基线注入对照（ADR §7 → podman 参数 → 断言位置）

| ADR §7 维度 | podman 参数（adapter 注入） | 断言位置 |
|---|---|---|
| CPU 每容器 2 核 | `--cpus 2`（NanoCpus=2000000000） | podman_host_e2e inspect |
| 内存 2 GiB | `--memory 2147483648` | podman_host_e2e inspect |
| PIDs 256 | `--pids-limit 256` | podman_host_e2e inspect |
| 执行时长 600000 ms 上限 | 看门狗 clamp [1,600000]ms；提交门帽 600s | podman_host_budget submit_gates |
| 输出 64 MiB 截断信封 | envelope::truncate（67108864 域帽） | podman_host_budget oversized_output |
| 任务 tmp 1 GiB | `--tmpfs /tmp:rw,size=1g,nosuid,nodev` | podman_host_e2e inspect |
| 每工具族默认 1 | FamilyLimiter（RUNNING 前占位） | podman_host_budget family_limiter |
| 根文件系统只读 | `--read-only` | podman_host_e2e inspect |
| 网络默认关闭 | `--network none` | podman_host_e2e inspect |
| 非 root + capabilities 收敛 | `--userns=keep-id` + `--cap-drop ALL` | podman_host_e2e inspect + 容器内 CapEff 自证 |

## 构建与验证

```bash
cd /home/emo/code/pensting/research-platform
podman build -t localhost/rip-tool-base:smoke -f containerfiles/rip-tool-base/Containerfile containerfiles/rip-tool-base/
cargo test --offline -p tool-engine --test podman_host_e2e --test podman_host_budget
podman system df          # 镜像与缓存总量在 50 GiB 预算内
```
