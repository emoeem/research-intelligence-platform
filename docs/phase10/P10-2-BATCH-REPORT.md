# 批次报告：P10-2（插件生态收口，Phase 10 第 2/4 批）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-14 |
| 判据 | ROADMAP §11.3 P10-1（信任级驱动确认强度与自动更新策略生效、目录发布形态）+ P10-2（升级失败自动回滚一次实演、镜像 digest 回滚一次实演、回滚窗口清理） |
| 结论 | 全部判据实测通过；点名门禁 138 passed 0 failed exit 0；selftest 52 PASS 0 FAIL exit 0；web typecheck/vitest 120/build 全绿；真实浏览器四级确认 4 passed；生产库指纹零写 |
| 任务书 | docs/phase10/P10-2-TASK.md（设计与文件清单） |

## 一、判据逐条对账（判据原文、命令、实测输出）

### 判据 1：信任级影响安装确认强度与自动更新默认策略（仅 official 自动提示，D10）生效

命令与输出（点名门禁含新增门控用例）：

```
cargo test --offline -p plugin-runtime -p plugin-registry
# plugin-registry lib: 50 passed（trust_policy::tests 4 用例在列：
#   confirmation_ladder_is_monotonic_strict / d10_only_official_gets_auto_prompt /
#   remote_requires_typed_id_and_server_fields / policy_json_covers_all_four_tiers_and_roundtrips）
# plugin-runtime tests/trust_gate_and_window.rs: 5 passed（d10_auto_update_prompt_gate 等）
# 汇总 15 个测试目标 138 passed 0 failed，exit 0
```

D10 门控实演（selftest 第 2/5 步，隔离库）：

```
rp-plugin update-check knowledge.rtis-redteam --new-version 0.2.0 --auto   # 信任级 local
# exit 1，stderr：{"code":"update_check_auto_not_allowed","message":"trust level \"local\" is not
#  allowed auto update prompts (D10); run manual update-check instead"}
# 同插件 manual update-check（无 --auto）：exit 0，has_compatible_new_version=true（manual 全档可用）
# 隔离库二 --trust official 安装后同命令：exit 0，has_compatible_new_version=true（prompted）
[实演] D10 门控：local 信任级拒绝自动更新提示（update_check_auto_not_allowed，audit result=blocked，manual 检查不受限）
[实演] D10 门控：official 信任级放行自动更新提示（result=prompted，audit plugin.update_check mode=auto）
```

安装确认强度分级实测（真实浏览器，docs/phase10/browser-e2e/ 五张截图）：

```
PW_CHROMIUM=$HOME/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome \
  npx playwright test -c playwright.plugins-trust.config.ts
# 4 passed (5.9s)：official 常规确认（无额外字段）/ community 确认对话框 /
# local 来源核对勾选门（勾选前确认键禁用）/ remote 输入插件 id 二次确认
# （不一致阻断 + confirm-id-mismatch 提示）；写请求体 confirm/confirm_id
# 随档位与策略表一致；零 console.error / 零 pageerror
# 浏览器探测：系统 PATH 无 chromium/chromium-browser/google-chrome-stable
# （command -v 实测空输出），用户缓存 chromium-1234 在位，PW_CHROMIUM 注入，
# 零系统级安装（P7-1 caveat 机器无关口径）
```

服务端强制闸实测（selftest 第 7 步，真实 rp_server 临时令牌临时端口）：

```
GET  /api/plugins/trust-policy → 四档在列（len(tiers)=4）
POST /api/plugins/install {trust:"local"}                      → 400 install_confirmation_required
POST /api/plugins/install {trust:"local","confirm":true}       → 确认闸放行进拉取链（见判据 4 说明）
POST /api/plugins/install {trust:"remote","confirm_id":"knowledge.wrong-id"} → 400 install_confirm_id_mismatch
[实演] server 确认分级强制：local 无 confirm 400 install_confirmation_required → confirm=true 放行闸进拉取链（离线 clone_failed，错误码已换面）；remote confirm_id 不一致 400 install_confirm_id_mismatch；trust-policy 端点四档在列
```

### 判据 2：一次插件升级失败自动回滚旧版本（实演回显原文在列）

selftest 第 3 步（隔离库，升级前 install 0.1.0 local，升级包缺 Manifest 声明的 capability entry）：

```
rp-plugin upgrade knowledge.rtis-redteam --manifest m2.json(0.2.0) --package pkg-v2-bad
# exit 1，机器可读错误码 validation_failed（VALIDATED 检查在升级链内 fail-closed）
rp-plugin list → active = knowledge.rtis-redteam@0.1.0（指针不切换）；version=0.2.0 行数 0（事务回滚）
[实演] 升级失败自动回滚旧版本（rolled_back、指针不切换）：active=knowledge.rtis-redteam@0.1.0，audit plugin.upgrade result=rolled_back
```

失败路径的代码面固化（既有单测不动，点名门禁在列）：lifecycle_full_path.rs upgrade_switch_pointer_and_rollback_on_failure 校验 checksum 篡改路径（直调 runtime.upgrade，audit 断言 upgrades[1]["result"]=="rolled_back"、"active_version_unchanged"==true）。

### 判据 3：一次镜像 digest 回滚演练（ADR §6 镜像回滚语义）

selftest 第 1 步（rp-image-pin，真实台账文件 + 真实 containerfiles/rip-tool-webresearch/tool-manifest.json 验证链）：

```
rp-image-pin pin  --image localhost/rip-tool-webresearch --digest <tool-manifest 实际 digest>
rp-image-pin update --image localhost/rip-tool-webresearch --digest <伪造 sha256:33*32> --manifest tool-manifest.json
# verified=false（digest 三方对账拒绝），outcome.result=rolled_back，
# outcome.current_digest = pin 值（指针未切换）
rp-image-pin update --digest <sha256:22*32> --manifest tool-manifest-updated.json
# 验证链通过（validate_static + 三方对账），outcome.result=switched，旧 digest 进 previous
rp-image-pin rollback --image localhost/rip-tool-webresearch
# outcome.result=rolled_back，current_digest 拨回 pin 值
[实演] 镜像 digest 回滚：升级校验失败自动回滚旧 digest（指针未切换，attempt 记 rolled_back，reason=digest 三方对账失败）
[实演] 镜像 digest 回滚：显式回滚拨回旧 digest（previous_digest 留作再滚回位，history 全程留痕）
```

单测固化：tool-engine lib image_ledger 5 用例（digest 严格格式 / pin 幂等与漂移拒绝 / 失败保旧 digest 与显式回滚 / load-save 往返与篡改 parse 拒绝 / history 有界），全仓测试在列。

### 判据 4：回滚窗口清理（D6：窗口结束且用户确认后清理）

selftest 第 4 步（0.1.0 → 0.3.0 升级成功后）：

```
rp-plugin rollback-window knowledge.rtis-redteam          # rollback_window_versions=[0.1.0]
rp-plugin cleanup-rollback knowledge.rtis-redteam         # exit 1 cleanup_requires_confirmation（未确认拒绝）
rp-plugin cleanup-rollback knowledge.rtis-redteam --confirm
# exit 0，report.cleaned=[0.1.0]，report.dirs_removed=1，
# installed/.../0.1.0 目录物理删除（清理前在位、清理后不在位双侧断言），
# active 0.3.0 不动，0.1.0 行跃迁 UNINSTALLED（历史保留），audit plugin.uninstall result=rollback_window_cleanup
[实演] 回滚窗口清理（D6）：--confirm 后 0.1.0 跃迁 UNINSTALLED，installed 目录物理删除，active 0.3.0 不动
```

红线守卫（单测固化）：rollback_window_cleanup_never_deletes_outside_data_root 断言 data_root 之外目录 cleaned 后仍存在（dirs_skipped[0].reason == "outside_data_root"），行仍跃迁 UNINSTALLED。

### 判据 5：目录发布形态（P10-1「目录发布形态」）

selftest 第 6 步：

```
rp-plugin trust-policy                                  # 四档策略表（no-db 只读面）
rp-plugin catalog-publish --out catalog.json --manifests-dir manifests/plugins \
  --trust-default official --trust-override knowledge.secskills=community
# exit 0，entries=3，content_sha256 记录；override 降档生效（secskills=community/
# awesome-hacking install_confirmation=single 并存）
rp-plugin catalog-verify --catalog catalog.json         # exit 0，sha 与发布一致
# 篡改 entries[0].description 后 catalog-verify：exit 1 catalog_verify_failed（sha_mismatch）
rp-plugin install knowledge.awesome-hacking --manifest <条目 manifest_path> --package <pkg> --trust official
# exit 0（按目录条目安装，manifest_path 即 install 消费面）
[实演] 目录发布形态：catalog-publish（3 条目 trust 分级 + sha）→ verify 一致 → 篡改拒绝（sha_mismatch）→ 按条目 manifest_path 安装受理
```

## 二、验收命令与实测输出汇总

```
cargo test --offline -p plugin-runtime -p plugin-registry
# 15 目标 138 passed 0 failed exit 0（明细：plugin-registry lib 50 / sources_five 6 /
# static_chain 13 / plugin-runtime lib 18 / trust_gate_and_window 5 / lifecycle_full_path 6 /
# deps 8 / sk1 4 / sk2 4 / sk3 6 / tool_plugin_registration 5 / knowledge_ingest 5 /
# layout 5 / endpoint 3 / rp-plugin bin 0）

cargo test --offline --workspace
# exit 0，合计 633 passed 0 failed（P10-1 基线 615 + 本批新增 18：
# trust_policy 4 + catalog 4 + trust_gate_and_window 5 + image_ledger 5）

cargo clippy --offline --workspace --all-targets -- -D warnings
# exit 0（首轮 5 处 doc list item without indentation（catalog.rs 模块注释），
# 改纯段落散文后归零）

bash apps/server/selftest_phase10_plugin_eco.sh
# exit 0，PASS=52 FAIL=0 ALL PASS；生产 platform*.db 指纹前后 diff 为空（零写红线）

cd web && npx tsc --noEmit            # exit 0
npx vitest run                        # Test Files 18 passed (18)，Tests 120 passed (120)
npm run build                         # tsc --noEmit && vite build ✓ built（exit 0）
PW_CHROMIUM=... npx playwright test -c playwright.plugins-trust.config.ts
# 4 passed (5.9s)，截图 5 张入库 docs/phase10/browser-e2e/ + RUN-LOG.md
```

## 三、成熟库优先与依赖决策

| 能力 | 选型 | 说明 |
| --- | --- | --- |
| 镜像验证链 | tool-registry `validate_static`（既有） | 不手写 Manifest 校验，rule 5 digest 一致性直接复用 |
| sha256 | tool-registry `hash::sha256_hex`（既有封装） | catalog 完整性复用同款 |
| Manifest/YAML 解析 | plugin-registry `parse_document` / serde_yaml_ng（既有） | catalog 与 manifest-inspect 复用 |
| 镜像台账存储 | 调用方路径 JSON 文件（serde_json 既有） | 零新增 vendor；镜像状态属运行时数据，路径显式传入 |
| 目录发布 | 确定性 JSON + sha256（既有栈） | 不引入 JSON Schema 发布器（口径由 verify 函数承载） |
| 前端对话框 | 既有 ui.tsx Modal + focus-trap（既有） | 不引入新组件库 |

零新增 vendor 目录（git status 实证）；Cargo.toml 变更仅 tool-engine `[[bin]]` 注册与 P10-1 已有的 rusqlite backup feature 注释。

## 四、已知限制与待验证面

1. server 安装确认闸放行后的来源拉取在本机离线失败（git clone TLS 拒绝，网络偶发可达）：selftest 对该步取形态无关断言（响应错误码不再是 install_confirmation_required 即证闸放行，HTTP 200 或 502 均可），400 闸前态由确定性断言钉死。完整「confirm 后 200 受理」的稳定路径需要真实上游可达，待网络环境复核（属环境面，非本批引入）。
2. CLI stage 的 copy_tree 按设计跳过来源包 checksums.sha256 与 plugin.lock（安装链重新生成）：升级失败实演破坏面取 Manifest entry 缺失（validation_failed）；checksum 篡改路径由单测固化（直调 runtime.upgrade）。
3. Playwright 分级确认为离线 mock 模式（route-mock 策略表与 manifest-inspect）：真实后端链路的 400 闸由 selftest 第 7 步以真实 rp_server 实测；真实后端下的浏览器端到端确认流（模式 B）不在本批（P6-8 模式 B 只覆盖 enable/disable 回路）。
4. update_check 的 manual 路径不做信任级门控（合同口径：手动检查全档可用），仅 auto 口径受 D10 约束。
5. rustfmt 版本漂移沿用 P10-1 记录：本批新增 10 文件按本机 rustfmt 归一实跑，未触碰文件不重排（HEAD 由 CI 侧 rustfmt 校验）。
6. 本批未 commit（工作流批次惯例由协调方收口）。批次 diff 面：17 个跟踪文件修改（audit-engine 1、plugin-registry 2、plugin-runtime 3、tool-engine 2、根 Cargo.toml/Cargo.lock 为 P10-1 已有改动、apps/server/rp_server.py、web 3）+ 新增路径（trust_policy.rs、catalog.rs、image_ledger.rs、rp-image-pin.rs、trust_gate_and_window.rs、selftest_phase10_plugin_eco.sh、web 5 文件、docs/phase10/ 两文档与 browser-e2e/）；git status 实证无越界文件（crates/database 与 search-engine 改动属 P10-1 批次既有工作树）。

## 五、生产红线核对

- `migrations/platform*.db` 五库 sha256 指纹 selftest 前后各留底一次，diff 为空（零写）。
- rp_server.py 零直连 SQL 写不变（selftest_phase6 静态断言口径兼容：本批仅新增 SELECT 与子进程调用）。
- 红线「不在宿主机安装任何系统级依赖」：浏览器走用户缓存（PW_CHROMIUM 三级探测实测），npm 依赖零新增，cargo 零新增 vendor。
