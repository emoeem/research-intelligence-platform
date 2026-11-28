# RIP 1.0 架构说明

> 状态：实现架构快照。日期：2026-11-28。

## 一、逻辑架构

```text
RIP Web Console
      |
  Unified API
      |
Knowledge / Agent / Workflow / Tool / Trace
      |
      +-- SQLite compatibility runtime
      +-- PostgreSQL governed knowledge and relations
      +-- Derived search index
      +-- Rust Core
      +-- Python Workers
```

## 二、核心模块

- Rust crates：domain、database、tool registry、tool engine、runtime gateway、context broker、agent engine、workflow engine、search engine、intel database、verification engine 和 report engine。
- Python workers：解析、embedding、评估及数据治理辅助进程。
- API：`apps/server/rp_server.py` 提供 REST、鉴权、审计、静态资源和受保护写操作。
- MCP：`crates/mcp-server` 使用 rmcp 提供工具、资源和 prompt 通道。
- Web：`web/` 下 React/Vite 工作台。

## 三、存储职责

SQLite 继续承担兼容运行数据、派生运行状态和本地工作台数据。PostgreSQL 承担知识实体、来源、证据、审核批次、关系和审计治理。

搜索索引是派生数据，可以重建，不能作为唯一事实源。PostgreSQL content/metadata relation 和派生 FTS 状态必须分别识别。

## 四、执行链路

工具调用统一经过 Tool Registry、Tool Engine、Runtime Gateway 和执行记录。工件、证据、run、trace 和 workspace 信息通过现有六 ID 体系关联。

Python Worker 和本地 Native Rust 能力优先原生执行；明确隔离需求时进入 Podman Host 或 VM。容器不是默认构建环境。

## 五、安全边界

- 默认 workspace 模式为 personal。
- 统一检索 API 对默认工作区之外 fail-closed。
- 写操作保留 Bearer 鉴权和 Origin/Fetch Metadata 校验。
- 未受信任文件和外部资料不得裸作为 Agent 指令。
- POC、模板和工具不得仅凭来源存在或静态通过就宣称已验证。
