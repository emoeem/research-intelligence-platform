# 外部 AI 快速上手（MCP 执行通道）

> 目标读者：没有本地工具的外部 AI（或给它装 MCP 客户端的宿主程序）。读完全文你能：经 stdio MCP 发现平台工具目录、按需取 Schema、用 `rip.run_tool` 真实执行一个工具、按 run_id 查状态并拿到 Artifact 与 Evidence 引用；本文所有命令在仓库根可直接复制执行（执行纪律：cargo 门禁从仓库根 CWD 发现 vendor 配置）。

| 项       | 值 |
| ---      | --- |
| 状态     | 生效中（Phase 9 P9-2，2026-11-14） |
| 适用版本 | Phase 9 P9-2 及之后 |
| 相关     | docs/phase9/PHASE9-DESIGN.md、docs/releases/RELEASE_2026-11-14_PHASE9-P9-2.md、apps/server/selftest_phase9_mcp.sh、integrations/rtis/rtis_mcp.mjs（kit 面，非本通道） |

## 一、30 秒总览

1. 启动方式：stdio 上的 NDJSON JSON-RPC 2.0，一行一请求，宿主把服务当子进程 spawn 并注入令牌环境变量（先例：github-mcp-server 的 GITHUB_TOKEN）。仓库根执行：
   ~~~bash
   cargo build --offline -p mcp-server --bin rp-mcp
   RP_MCP_TOKEN=<你的令牌> target/debug/rp-mcp
   # 之后逐行喂 JSON-RPC 请求；EOF 退出
   ~~~
2. 禁止匿名：`RP_MCP_TOKEN` 未配置时所有请求以 -32001 auth_required 拒绝（P9-1 纪律延续）。
3. 工具目录分两段：能力面（`rip.*` 六条平台协议面）+ 注册面（tool 表逐行投影，MCP 名 = `rip.<tool_id>`）。条目只有摘要列，没有 description 和 inputSchema，Schema 一律经 `rip.get_tool_schema` 按需获取（计划 §11:658：不要在 MCP 初始化时把所有工具的完整说明暴露给模型）。
4. 执行只走 `rip.run_tool` 一个入口：内部经 Tool Engine 受理（D-TC-05）与既有适配器派发，返回结构化结果 + Artifact + Evidence 引用；与 Web Workflow Tool 节点是同一个引擎（`crates/tool-engine/src/lib.rs:147` open、`:157` submit），不是第二套派发。

## 二、环境变量（宿主 spawn 时注入）

| 变量 | 必填 | 缺省 | 说明 |
| --- | --- | --- | --- |
| `RP_MCP_TOKEN` | 是 | 无 | 握手令牌；未配置一律 -32001 |
| `RP_DB_PATH` | 否 | `migrations/platform.db` | 目录主库（只读） |
| `RP_AUX_DB` | 否 | 不挂载 | 目录 aux 库（只读，handle_tools 同款增量合并） |
| `RP_RUN_DB` | 否 | `rip-data/tool-runs.db` | 运行库（可写数据面：tool_run / artifact / evidence / runtime_node 行，gitignore 运行时数据；生产 platform.db 零写） |
| `RP_MIGRATIONS_DIR` | 否 | `migrations` | 运行库迁移目录 |
| `RP_MANIFEST_DIRS` | 否 | `manifests/tools,containerfiles/rip-tool-webresearch,containerfiles/rip-tool-coreutils` | runtime 块解析来源（csv） |
| `RP_ARTIFACTS_DIR` | 否 | `rip-data/artifacts` | Artifact 落盘根 |
| `RP_RUNTIME_NODE_ID` | 否 | `host-local` | 容器 Runner 节点 id |
| 设为 `RP_RUN_DB=none` | 否 |  | 关停执行面（run_tool 返回结构化未配置说明） |

## 三、外部 AI 八步实测走一遍（计划 §11:670-679）

下面用 `rtk.mcp`（TOOL-INVENTORY T08 的最小 stdio MCP 客户端，纯标准库）当外部 AI 的客户端；没有 rtk 时用第四节的一次性管道形态等价。本节每步都是 2026-11-14 在本机实跑的原样输出摘要。

~~~bash
# 前置：执行面工具先登记进运行库（rp-tool register，P6 既有路径）
cargo build --offline -p tool-registry --bin rp-tool
./target/debug/rp-tool register --manifest manifests/tools/rip.doc.parse.json --db rip-data/tool-runs.db

# 客户端脚本（rtk 在仓库父目录的兄弟工作区；CI/新机器无 rtk 时见第四节）
RP_MCP_TOKEN=<你的令牌> python3 -I - <<'EOF'
import sys, json
sys.path.insert(0, "../toolkit/src")
from rtk.mcp import Client

with Client("target/debug/rp-mcp", timeout=120) as c:
    # 步骤 1/2：MCP 握手 + 目录发现（initialize 由 Client.start 完成）
    names = [t["name"] for t in c.list_tools()]
    # 步骤 3/4：按需取 Schema（目录读面没有的行回退运行库登记件，source=run_db）
    schema = c.call_tool("rip.get_tool_schema", {"tool_id": "rip.doc.parse"})
    # 步骤 5/6/7：run_tool 派发 Tool Engine，拿结构化结果 + Artifact + Evidence 引用
    res = c.call_tool("rip.run_tool", {
        "tool_id": "rip.doc.parse",
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
    })
    run = json.loads(res["content"][0]["text"])
    # 步骤 8：按 run_id 查状态（外部 AI 继续规划的依据）
    st = c.call_tool("rip.get_run_status", {"run_id": run["run_id"]})
EOF
~~~

实测输出摘要（2026-11-14）：

~~~text
[1-2] 首页 20 条（limit 缺省 20，nextCursor=v1-20 真分页）；能力面：rip.list_tools /
      rip.get_tool_schema / rip.run_tool / rip.search_knowledge / rip.get_run_status / rip.cancel_run
[3-4] source = run_db | input_schema.type = object
[5-7] run_id = rn-e99d6e50db6ec23600000000 | status = SUCCEEDED
      | artifact_id = art-4e92987e4d7b86af31bc5432 | evidence_refs = ['ev-865c85279b36a17edc674527']
[8]   run_status = SUCCEEDED | artifacts 与 evidence 引用同 run_id 在库
~~~

run_tool 成功载荷的字段（外部 AI 关心的就是这几把钥匙）：

| 字段 | 含义 |
| --- | --- |
| `run_id` | 本次运行 id（tool_run 主键，审计链 correlation） |
| `status` | 终态词形（SUCCEEDED / FAILED / TIMED_OUT / CANCELLED）或等待态 WAITING_FOR_RUNNER |
| `result` | 结构化输出（仅 SUCCEEDED 有值；引擎侧已过 output_schema 校验） |
| `error` | §5.3 错误对象（category / code / message / retryable） |
| `artifact_id` / `artifact_uri` | 结果快照产物（P7-2 同一登记实现，落 `<RP_ARTIFACTS_DIR>/<run_id>/`，sha256 = `content_hash`） |
| `evidence_refs` / `evidence_id` | Evidence 行引用（evidence 表 `source_pk` = run_id，可复核） |
| `artifacts` | 信封产物列表（容器工具的 out/ 产物等，role=primary/attachment） |
| `usage` / `environment` / `transcript` | 用量、环境指纹、状态轨迹（QUEUED 起到终态） |

## 四、不用 rtk 的一次性管道形态（ROADMAP §10.4 验收命令原形）

~~~bash
# 发现
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":""}}' \
  | RP_MCP_TOKEN=<你的令牌> cargo run --offline -p mcp-server --bin rp-mcp
# 执行（2026-11-14 实测原样结论见 docs/releases/RELEASE_2026-11-14_PHASE9-P9-2.md）
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rip.run_tool","arguments":{"tool_id":"rip.http.inspect","input":{"url":"https://example.com/"}}}}' \
  | RP_MCP_TOKEN=<你的令牌> cargo run --offline -p mcp-server --bin rp-mcp
~~~

第二条命令的机器差异注记（重要，不要误读）：

- `rip.http.inspect` 是 podman 容器工具（runtime.type=podman_host）。本机有 podman 且验收镜像在位时，会真实起容器执行并返回 `status_code / final_url / body_artifact_id` 结构化结果 + Artifact；
- 本机没有 podman 时（2026-11-14 的执行机如此），Gateway 心跳探针失败，返回 `status=WAITING_FOR_RUNNER`（isError=false）且 run 行已落库：计划 §5.6:336 原文语义「探测失败不产生 FAILED」，run 等 Runner 恢复后续跑，不伪造结果；
- 要在无容器机器上看「结构化结果 + Artifact + Evidence 引用」的完整回路，用 python_worker 工具（如 `rip.doc.parse`，纯本地文本解析，见第三节实测）。

## 五、T2 工具与授权引用

`permission_level=T2` 的工具在 `rip.run_tool` 必须带 `authorization_ref`（D-TC-05）：缺失时引擎提交面直接以 `POLICY_DENIED` 拒绝且不落任何 run 行（拒绝不落行，run_tool 返回 isError=true 的结构化说明）。这是引擎固有校验（`crates/tool-engine/src/lib.rs:184`），MCP 面原样保留不绕过。

~~~bash
./target/debug/rp-tool run --db rip-data/tool-runs.db --tool <T2 工具 id> \
  --input '<json>' --authorization-ref <授权引用>
~~~

## 六、CLI 面（rp-tool run/status/cancel）

`rp-tool` 的三个执行子命令是最小 MCP 客户端：spawn `rp-mcp` 子进程做一次性 stdio JSON-RPC 交换，与外部 AI 走同一执行通道（`RP_MCP_BIN` 环境变量可指路，缺省 `target/debug/rp-mcp`；令牌一次性自生成经环境变量注入，不落盘不回显）。

~~~bash
# 帮助面（2026-11-14 实测：八行用法，exit 2）
./target/debug/rp-tool run --help
# 执行 / 查状态 / 取消
./target/debug/rp-tool run --db rip-data/tool-runs.db --tool rip.doc.parse --input '{"text":"示例"}'
./target/debug/rp-tool status --db rip-data/tool-runs.db --run <run_id>
./target/debug/rp-tool cancel --db rip-data/tool-runs.db --run <run_id>
~~~

退出码：0 成功（含「取消被拒但受理面正常应答」），1 isError（如未知 run、未登记工具），2 用法错误。

## 七、对账（宿主或外部 AI 复核用）

运行库是普通 sqlite 文件，只读复核（纪律：mode=ro URI，不写生产与运行库）：

~~~bash
python3 -I -c "
import sqlite3
con = sqlite3.connect('file:rip-data/tool-runs.db?mode=ro', uri=True)
for r in con.execute('SELECT run_id, tool_id, status FROM tool_run ORDER BY created_at'):
    print(r)
con.close()"
~~~

2026-11-14 实测：tool_run / artifact / evidence 三表行与 run_tool 返回的 run_id / artifact_id / evidence_refs 逐条对上；artifact 落盘文件 sha256 与行内 `sha256`、载荷 `content_hash` 一致。

## 八、已知边界

- 能力面 `rip.search_knowledge` 尚未接线（调用返回结构化 isError，后续批次）；resources / prompts 为 P9-5 前置空占位。
- 容器工具的完整回路需要容器运行时与 digest 钉死的验收镜像（Phase 3 交付）；无 podman 机器上如实等待（见第四节注记），容器全链断言由 `crates/tool-engine/tests/web_research_e2e.rs` 在受控环境承载。
- 双 MCP 边界（phase6/DUAL-MCP-BOUNDARY.md）：本通道是平台面（rip.* 前缀）；kit 的 rtis_mcp.mjs（redteam_* 前缀）是另一套接入面，勿混用。
