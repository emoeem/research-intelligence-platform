# ADDING-A-TOOL：新增工具一条线扩展指南

| 项     | 值                                                                     |
| ------ | ---------------------------------------------------------------------- |
| 状态   | 生效中（P6-9 交付）                                                    |
| 日期   | 2026-11-11                                                             |
| 适用   | python_worker 类工具（Worker Protocol v1）；其他 runtime 类型见 §五    |
| 相关   | TOOL-CONTRACT-DRAFT、schemas/tool-manifest/v1.json、scripts/rip_new_tool.py |

## 一、前置条件

- `cargo build --offline -p tool-registry -p tool-engine` 已产出
  `target/debug/rp-tool` 与 `target/debug/rp-tool-health`；
- 注册目标库为开发副本（**禁止写 platform*.db**）：
  `--db migrations/dev-p6-tool.db` 或 `/tmp/rp_p6_tool_e2e.db`。

## 二、一条线命令（复制粘贴执行）

以下命令以新增 `rip.example.text-clean` 为例。将尖括号占位替换为你的工具参数。

### 步骤 1：脚手架一条线（生成 + 校验 + 注册 + 健康检查 + 露出核验）

```bash
python3 -I scripts/rip_new_tool.py \
  --id rip.example.text-clean \
  --name "Text Cleaner" \
  --family utility \
  --description "去除文本首尾空白与多余空行" \
  --db migrations/dev-p6-tool.db
```

预期输出全 PASS。脚本自动完成：

1. D-TC-02 正则校验 tool_id（小写点分 namespace.name）；
2. 生成 Tool Manifest → `manifests/tools/<id>.yaml`；
3. 生成 worker stub → `workers/python/<last_seg>_worker.py`；
4. `rp-tool validate`（七条静态校验）；
5. `rp-tool register --db <db>`（tool + tool_version 落库）；
6. `rp-tool-health ping`（Worker Protocol v1 initialize 握手）；
7. 查 DB 确认 tool 行在位（= /api/tools 露出的数据面证据）。

### 步骤 2：确认 /api/tools 露出

```bash
# 起 rp_server（RP_AUX_DB 指向注册库）
RP_PORT=7800 RP_AUTH_TOKEN=test RIP_AUX_DB=migrations/dev-p6-tool.db \
  python3 apps/server/rp_server.py &
sleep 2
curl -s -H "Authorization: Bearer test" "http://127.0.0.1:7800/api/tools?limit=200" \
  | python3 -c "import sys,json; d=json.load(sys.stdin); \
    print([i['tool_id'] for i in d['items'] if 'example' in i['tool_id']])"
```

预期输出包含你的 tool_id。

### 步骤 3：Tool Center 页面确认

浏览器打开 `http://127.0.0.1:7800/`（Tool Center 页 = 执行工作区 → 工具中心），
新工具按 `updated_at` 排序出现在目录列表顶部。

## 三、工具语义实现

脚手架生成的 worker stub 是一个可运行的 Worker Protocol v1 回显服务。
在 `execute(ctx, task)` 方法中实现你的工具语义：

```python
def execute(self, ctx: WorkerContext, task: dict) -> dict:
    text = task.get("text", "")
    # 你的工具逻辑
    result = text.strip()
    return {"result": result}
```

修改后重跑健康检查验证：

```bash
./target/debug/rp-tool-health ping --manifest manifests/tools/<id>.yaml
```

## 四、复用面（不手写的部分）

| 层               | 复用实现                                        | 位置                                         |
| ---------------- | ----------------------------------------------- | -------------------------------------------- |
| 七条静态校验     | `validate_static` + `jsonschema_lite`           | crates/tool-registry/src/validate.rs          |
| 冻结契约         | `schemas/tool-manifest/v1.json`（draft 2020-12） | schemas/tool-manifest/v1.json                |
| tool/tool_version 落库 | `Registry::register`（事务 + rule 2 版本可比） | crates/tool-registry/src/lib.rs             |
| 健康检查         | Worker Protocol v1 握手（initialize + worker_name 校验） | crates/tool-engine/src/bin/rp-tool-health.rs |
| Tool Center 露出 | /api/tools（rp_server.py handle_tools）         | apps/server/rp_server.py                     |

## 五、其他 runtime 类型

| runtime      | Manifest runtime 块要点                                     | 健康检查                              |
| ------------ | ------------------------------------------------------------ | ------------------------------------- |
| python_worker | `command: ["python3", "-I", <worker.py>]` + `worker_name` | rp-tool-health ping ✓                |
| podman_vm    | `image @sha256:<digest>` + `entrypoint` + `provenance.image_digest` | 容器健康检查（归 Phase 2 Gateway） |
| mcp_client   | `endpoint` + `tool_name`                                     | MCP 握手（归 P6-6 project_endpoint） |
| cli_adapter  | `argv_template`                                              | 白名单探测（M6 ETL 口径）             |

## 六、版本升级

```bash
# 修改 manifest 中的 version 字段（须严格大于当前版本）
vim manifests/tools/<id>.yaml
# 重新注册（rule 2：新版本须 > 全部既有版本）
./target/debug/rp-tool register --manifest manifests/tools/<id>.yaml --db migrations/dev-p6-tool.db
```

## 七、红线

1. **禁止写 platform*.db**：注册目标一律用开发库副本（`--db` 参数）。
2. **禁止手写 SQL 落库**：工具注册必须经 rp-tool register（tool-registry 状态机）。
3. **runtime.command 不经 shell**：固定 argv，不拼接用户输入。
4. **network.mode = none 缺省**：需要出网的工具须改 Manifest 并过网络策略评审。
