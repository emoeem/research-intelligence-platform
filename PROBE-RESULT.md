# DEC-001 构建验证记录（Rust Core 准入实验）

日期：2026-11-06　执行：主会话　位置：`research-platform/probe-cargo/`（验证工件，保留）

## 结论

**Rust Core 路线准入条件满足。** 计划书 §7.2 要求的"无 crates.io API / 无 sparse index 条件下 clean build"已在本机验证通过，且条件比预想更宽松：本机 cargo 全局配置走 **rsproxy.cn 镜像**（`~/.cargo/config.toml`），M0 审计直连 crates.io 的超时/TLS EOF 结论**不适用于 cargo 实际路径**。

## 实验数据

| 步骤 | 命令 | 结果 |
|---|---|---|
| 工具链 | `cargo --version` | 1.99.1（rustc 同版） |
| 依赖 | rusqlite(bundled) + serde + serde_json + anyhow | 解析成功 |
| 在线构建 | `cargo build`（rsproxy） | **9.57s**，无重试无失败 |
| vendoring | `cargo vendor vendor` | 43MB 全量 vendor |
| 离线构建 | `cargo clean && cargo build --offline`（vendored-sources 配置） | **8.73s**，成功 |
| 运行 | `./target/debug/rp_probe` | 正常输出 |

## 对主计划的影响

| 项 | 变化 |
|---|---|
| R-03（crates.io 网络阻塞构建） | **降级为已缓解**：rsproxy 在线 + vendor 离线双通道均可用；保留 vendor 目录进版本库的策略即可 |
| DEC-001 | 从"建议 + 回退条款"升级为"**验证通过**"；M2-A 构建基线的前置条件已满足 |
| §7.2 门槛 | lockfile（已生成 Cargo.lock）+ vendor 目录 + `.cargo/config.toml` 三件套就位；CI 复现条件具备 |

## M2 保留的工程约定

1. `Cargo.lock` 与 `vendor/` 一并提交（离线可复现）；vendor 更新走 `cargo vendor` 全量重生成。
2. 真实 workspace 不复用 probe 依赖版本，M2-A 重新锁定并重新 vendor。
3. probe-cargo 保留为验证工件（含 `.cargo/config.toml` 的 vendored 配置示例）。
