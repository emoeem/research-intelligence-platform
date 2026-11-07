# RIP：Research Intelligence Platform

把 ERI/RTIS 工具链（资源入库、仓库元数据、检索 Resolver、知识库 gate）重建为可商业化平台的
工程线。当前状态：**M0-M8 全部里程碑交付，M8 已切换生效**（RTIS 生产 Resolve 路径默认走
本平台统一库），Web 工作台以 systemd 用户服务常驻本机。

维护者文档：[`docs/SESSION-HANDOFF.md`](docs/SESSION-HANDOFF.md)（跨会话唯一状态源）与
[`docs/M8_READINESS.md`](docs/M8_READINESS.md)（切换检查单与已知限制）。

## 架构

```text
legacy/                    ERI P0-P2 与 architecture-2.0 报告（rsync 归档，db/cache/tarball 不入库）
migrations/                Schema V1-V4 + ETL（dryrun/evidence/tasks/m6）+ verify.py 对账
migrations/platform.db     生产化统一库（不入库；重建 = ETL 全量批 + verify 100%）
crates/                    Rust workspace：domain / database / audit-engine / resource-engine
                           / search-engine / quality-engine（vendored 离线构建）
workers/python/            三 worker（parsing / embedding / evaluation）+ 契约与功能测试
apps/server/               M7 只读 Web 工作台（stdlib http.server + Token 鉴权 + systemd 用户服务）
integrations/rtis/         生产 rtis.py 的接入层与备份（RTIS_RESOLVER_MODE=rp|legacy|off）
```

## 快速上手

```bash
cargo test --offline --workspace          # Rust 全量测试
cargo clippy --offline --workspace --all-targets -- -D warnings
python3 -m unittest discover -s workers/python/tests   # worker 测试（无模型环境自动 skip）

python3 migrations/verify.py              # 对账红线：一致率必须 100%（RP_TARGET_DB 可换目标库）
```

CI：`.github/workflows/ci.yml`（Rust 测试 + clippy 零警告 + worker 测试）；verify.py 的对账
对象含生产衍生快照，按纪律不入库，对账在本地执行。

## 部署形态（本机）

- 工作台：`apps/server/rp_service.sh {gen-token|install|start|stop|status|logs}`
  （systemd 用户服务，Bearer Token 鉴权，默认 127.0.0.1:7800）。
- 生产库：`migrations/platform.db`（M1 全量批：26 兼容库 evidence 700 行 + research_task 大 JSON 段）。
- embedding：bge-small-zh-v1.5 ONNX（ModelScope，见 `docs/M8_TAIL_EMBEDDING_REPORT.md`；
  模型不可用自动回退 hash 后端并在响应标注 backend）。
- 回滚：`RTIS_RESOLVER_MODE=legacy` 或还原 `integrations/rtis/backups`。

## 工程纪律

- 对账语义为**快照对快照**：源快照锁定在 `migrations/sources-snapshot/`，活源 drift 由
  registry 自愈豁免覆盖；一致率 100% 是切流与推送的硬前提。
- 构建走 vendored 离线（rsproxy 拉取后 vendor），构建与测试在 podman 容器（用户指定）。
- 文档纪律与授权口径见仓库外层 AGENTS.md 与 `docs/` 各报告。

## License

私有工程线（随外层工作区，未附开源许可证）。
