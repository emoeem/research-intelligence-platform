# SEC-0 advisories 裁决书：三条 unmaintained 逐条裁决 + index-failure 登记

日期：2026-11-20
裁决人：SEC-0 依赖安全裁决员
配套任务书：`docs/phase0/SEC-0-TASK.md`（rusqlite/libsqlite3-sys bump 与 vendor 流程）
遗留出处：`docs/rip3/wsw/WSW-1-BATCH-REPORT.md` §4
基线：`cargo deny check advisories`（cargo-deny 0.20.2）实测 4 error + 1 warning，见任务书 §0

裁决口径：`deny.toml ignore`（带理由注释 + 替代评估结论 + 复查日期）为默认选项；
每条替代面必须实际评估并记录，不允许无脑 ignore；ignore ≠ 永久豁免，复查日期到期未处理
视为欠账。RUSTSEC-2026-0459（vulnerability，有修复版本）不在本裁决书范围，走任务书 bump。

---

## 1. RUSTSEC-2026-0543（chrono 0.4.45 unmaintained）→【deny.toml ignore】

### 1.1 消费面实测

- `cargo tree -i chrono`：全仓仅两条入边，均为外部传递依赖——
  `rmcp v3.10.0 → mcp-server`、`schemars v1.2.3 → rmcp`。
- workspace 源码 `chrono::`/`use chrono` grep（crates/ + tests/）：**0 文件命中**。
  chrono 不是 workspace 直接依赖，`Cargo.toml` 全仓 grep 无 chrono 声明。
- 唯一触发路径：mcp-server 经 rmcp（MCP SDK）间接引入；rmcp 侧 chrono 消费我们无法改。

### 1.2 替代评估（结论：不可替换，替代成本在仓外）

- 公告推荐替代：jiff、time（公告原文 + rustsec.org 页面核实，issue chronotope/chrono#1768）。
- 替代对象不存在于本仓：workspace 没有 chrono 使用代码可改；换替代 = 等 rmcp 生态切换
  （rmcp 去 chrono 或换 jiff），属跨仓上游依赖演进，不是本批可执行的改动。
- 无 pinned/patch 类低成本手段（unmaintained 无修复版本，公告 patched = none）。
- 风险定性：INFO 级，无具体漏洞；rmcp 是活跃 SDK，生态切换是时间问题不是风险敞口扩大问题。

### 1.3 裁决与登记

**deny.toml ignore**，注释模板（任务书 §5.1 已含）：

```toml
"RUSTSEC-2026-0543", # chrono 0.4.45 unmaintained：纯传递依赖（rmcp 3.10.0 ← mcp-server），workspace 源码零直用；替代=jiff/time，需 rmcp 生态切换（跨仓大改）不属本批。替代评估见 SEC-0-ADVISORIES-DECISION.md §1
```

- **复查日期：2027-05-20**。触发条件任一：rmcp 发新版声明去 chrono/换 jiff；
  chrono 出现实质漏洞公告（届时 unmaintained + 漏洞组合不可 ignore，必须升级处理）。
- 复查动作：重跑 `cargo tree -i chrono` 看 rmcp 侧是否已解；解则删除该 ignore 行并确认
  `cargo deny check advisories` 仍 exit 0。

---

## 2. RUSTSEC-2024-0436（paste 1.0.15 unmaintained）→【deny.toml ignore】

### 2.1 消费面实测

- `cargo tree -i paste`：全仓仅一条入边——`tokenizers v0.21.4 → context-broker`
  （tokenizers 为 HF 分词库，context-broker 经 TokenizerAdapter 使用）。
- workspace 源码 `paste::`/`use paste` grep：**0 文件命中**。paste 为 tokenizers 的
  proc-macro 内部依赖，workspace 不直接消费。
- paste 是 proc-macro，编译期使用，不进入运行时二进制行为面。

### 2.2 替代评估（结论：不可替换，替代点在 tokenizers 内部）

- 公告推荐替代：pastey（drop-in fork）、with_builtin_macros。公告 patched = none。
- vendor/ 实测已有 `pastey/` 目录（0.21.x 线部分链已在用），但 tokenizers 0.21.4 本体
  仍声明 paste——替换 = 给 tokenizers 打补丁/换分词库，两者都不在本批能力内
  （tokenizer 是 P10/autoresearch 计量链关键组件，动它属独立批次且无安全收益：
  proc-macro 编译期工具，运行时零暴露）。
- 风险定性：INFO 级，仓库归档（不再修复），无已知漏洞。

### 2.3 裁决与登记

**deny.toml ignore**：

```toml
"RUSTSEC-2024-0436", # paste 1.0.15 unmaintained：纯传递依赖（tokenizers 0.21.4 ← context-broker），workspace 源码零直用，proc-macro 编译期无运行时暴露；替代=pastey（vendor 已有）需动 tokenizers 本体，不属本批。替代评估见 SEC-0-ADVISORIES-DECISION.md §2
```

- **复查日期：2027-05-20**。触发条件：tokenizers 升级批次（任一）落地时顺带核对其
  依赖是否去 paste；升级批次与复查合并执行。

---

## 3. RUSTSEC-2024-0449（unsafe-libyaml 0.2.11 unmaintained）→【deny.toml ignore】（三条中唯一有低成本替换路径，标记候选）

### 3.1 消费面实测（与其他两条不同：这是 workspace 直接依赖链）

- `cargo tree -i unsafe-libyaml`：唯一入边 `serde_yaml_ng v0.10.0`。
- **serde_yaml_ng 是 workspace 直接依赖**（workspace 根 `Cargo.toml:50` 钉
  `serde_yaml_ng = "0.10"`，P6-1 选型：dtolnay serde_yaml 0.9 的维护续作）。
- 源码使用面（grep 实测）：**10 文件 / 20 行，5 个 crate + tests**——
  `intel-db/src/parser.rs`、`mcp-server/src/dispatch.rs`、`plugin-registry/src/{lib,manifest}.rs`、
  `tool-engine/src/bin/{rp-tool-health,rp-image-pin}.rs`、`tool-registry/src/strict_json.rs`、
  `tool-registry/src/bin/rp-tool.rs` + `tool-registry/tests/sk3_tool_manifests.rs`、
  `mcp-server/tests/run_tool_e2e.rs`。用途：插件/工具 Manifest 的 YAML 登记件解析。

### 3.2 替代评估（结论：升级无效、替换可做但成本超本批，ignore + 候选标记）

- **升级路径无效**：crates.io sparse index 实测 `serde_yaml_ng` 最新线 0.9.35 / 0.9.36 /
  0.10.0 全部声明 `unsafe-libyaml ^0.2.11`——没有任何已发布版本换掉 unsafe-libyaml。
  bump serde_yaml_ng 解除不了本公告。
- **公告推荐的真替代**：saphyr / serde-saphyr（安全 Rust YAML 解析系）；
  公告明确不把 serde_yaml 各 fork 列为替代（它们都挂 unsafe-libyaml）。
- **替换成本实测**：API 面 10 文件 / 20 行，看似小；但替换的是解析器语义面——
  Manifest YAML 的严格解析行为（重复键、类型强制、锚点/别名、多文档）需跨
  plugin-registry / tool-registry / tool-engine / intel-db / mcp-server 五处登记件解析
  全量回归 + fixtures 核对，且推翻 P6-1 选型决策。属独立批次（非本批）。
- **风险定性**：INFO 级，crate 本体无已知漏洞（公告原文：c2rust 机翻 unsafe Rust，
  "bugs found will not be fixed upstream"；archive 2024-03-25）。收益/成本比不支持本批替换。

### 3.3 裁决与登记

**deny.toml ignore**，并标记为本批三条中唯一具备低成本替换路径的候选
（API 面仅 10 文件/20 行）——若后续 YAML 消费面继续扩张（新增解析点）或出现任何
unsafe-libyaml 实体漏洞公告，优先升级为替换批次而不是继续 ignore：

```toml
"RUSTSEC-2024-0449", # unsafe-libyaml 0.2.11 unmaintained：经 serde_yaml_ng 0.10.0（workspace 直接依赖，P6-1 选型）；实测 serde_yaml_ng 全版本仍挂 unsafe-libyaml，bump 无效；真替代=serde-saphyr 系，API 面 10 文件/20 行但解析语义需跨 5 crate 回归，独立批次候选。替代评估见 SEC-0-ADVISORIES-DECISION.md §3
```

- **复查日期：2027-05-20**。触发条件（任一提前）：新增 YAML 解析消费点；
  unsafe-libyaml 出现实体漏洞公告（unmaintained + 漏洞组合不可 ignore）；
  serde_yaml_ng 发布去 unsafe-libyaml 的版本（届时低成本 bump 解除）。

---

## 4. deny.toml [advisories] 段最终建议文本（合并三条裁决）

`deny.toml` 末尾追加（本批已实测语法可被 cargo-deny 0.20.2 接受、ignore 生效——
临时追加运行后 3 条 unmaintained error 消失，仅剩 RUSTSEC-2026-0459 vulnerability
与 index-failure warning，随后 deny.toml 已复原）：

```toml
[advisories]
version = 2
ignore = [
  # 三条 unmaintained 裁决（SEC-0，docs/phase0/SEC-0-ADVISORIES-DECISION.md，2026-11-20）：
  # 均为 INFO 级无修复版本；替代面已逐条实测，见各条尾注。复查统一 2027-05-20。
  "RUSTSEC-2026-0543", # chrono unmaintained：纯传递（rmcp 3.10.0），workspace 零直用；替代=跨 rmcp 生态（§1）
  "RUSTSEC-2024-0436", # paste unmaintained：纯传递（tokenizers 0.21.4），proc-macro 无运行时暴露（§2）
  "RUSTSEC-2024-0449", # unsafe-libyaml unmaintained：经 serde_yaml_ng（直接依赖）；bump 无效，serde-saphyr 替换属独立批次候选（§3）
]
# RUSTSEC-2026-0459（libsqlite3-sys / CVE-2026-119375）不 ignore：vulnerability 有修复线，
# 由 SEC-0-TASK.md rusqlite 0.41.1 bump 消除，验收 = cargo deny check advisories exit 0。
```

---

## 5. index-failure 登记项（email_address / filetime，非 ignore 对象）

cargo-deny 对 `email_address 0.2.10`、`filetime 0.2.30` 报
`warning[index-failure]: unable to check for yanked crates`——yanked 状态核验因
registry/index 查询不可达而未完成，**不得视为已核验**（WSW-1 §4 原文口径）。实测消费链：
`jsonschema 0.56.0 → email_address`（Cargo.lock:1362/1370）、`tar 0.4.46 → filetime`
（Cargo.lock:2800）。两者均为传递依赖，workspace 无直接声明。

**登记为 index 恢复后复查项**（不计入本批验收，但验收判据允许该 warning 存在）：
1. 触发条件：advisory-db / registry index 查询恢复可达（本批实测 github.com TLS 偶发
   ECONNRESET，任务书 §6）。
2. 动作：`cargo deny check advisories` 复跑 → 确认 index-failure warning 消失；
   核验 email_address 0.2.10 / filetime 0.2.30 无 yanked 命中。
3. 若核验发现 yanked：按依赖审计流程为上游 bump 开批次（不在本批）。
4. 登记位置：批次报告 SEC-0 遗留段 + 本节；下次依赖安全批次（SEC-1 或等价）开场必核。

---

## 6. 裁决汇总

| 公告 | crate | 类型 | 裁决 | 依据一句话 |
|---|---|---|---|---|
| RUSTSEC-2026-0459 | libsqlite3-sys 0.30.1 | vulnerability | **不 ignore，走 bump** | 有修复线（0.39.x），rusqlite 0.41.1 带动，任务书 §1 |
| RUSTSEC-2026-0543 | chrono 0.4.45 | unmaintained | ignore，复查 2027-05-20 | 纯传递（rmcp），workspace 零直用，替代在仓外（§1） |
| RUSTSEC-2024-0436 | paste 1.0.15 | unmaintained | ignore，复查 2027-05-20 | 纯传递（tokenizers），proc-macro 无运行时暴露（§2） |
| RUSTSEC-2024-0449 | unsafe-libyaml 0.2.11 | unmaintained | ignore（替换候选标记），复查 2027-05-20 | 直接依赖链但 bump 无效，真替换跨 5 crate 语义回归，独立批次候选（§3） |
| （登记项） | email_address / filetime | index-failure | 复查项，不进 ignore | yanked 核验不可达 ≠ 已核验，恢复后复跑（§5） |
