# 批次任务书：P10-2（Phase 10 第 2/4 批，插件生态收口：信任级驱动确认与更新策略、双轨升级回滚实演、目录发布形态）

| 项 | 值 |
| --- | --- |
| 状态 | 已实施并验收（见 P10-2-BATCH-REPORT.md） |
| 日期 | 2026-11-14 |
| 判据来源 | docs/phase0/PHASE-ROADMAP.md §11.3 P10-1/P10-2 两行；docs/phase0/PLUGIN-CONTRACT-DRAFT.md §5.3（升级回滚与窗口）、§6（信任级四档）、§12 草案 D6/D10；docs/phase0/TOOL-RUNTIME-ADR.md §6:178（镜像回滚语义） |
| 相关 | P10-1-BATCH-REPORT.md（selftest 与文档格式先例）；crates/plugin-registry/src/lock.rs:50（TrustLevel 四档既有面）；crates/plugin-runtime/src/lib.rs:692（upgrade 回滚语义既有实现） |

## 一、批次目标与范围

插件生态收口（覆盖 ROADMAP P10-1 剩余项与 P10-2 全部判据）：

1. 信任级四档驱动生效（P10-1 判据）：安装确认强度四级分级（official 常规确认到 remote 输入插件 id 二次确认）+ 自动更新默认策略 D10（仅 official 允许 update_check 自动提示）。
2. 插件升级失败自动回滚真实实演一次（P10-2 判据；回滚语义 P6-2 已实现，本批补实演与门控面）。
3. 镜像 digest 回滚真实实演一次（ADR §6：镜像固定版本与 digest，更新先验证再切换，升级失败回滚旧 digest）。
4. 回滚窗口清理（§5.3 第 5 步 + D6：窗口结束且用户确认后清理旧目录）。
5. 插件目录发布形态落地（P10-1「目录发布形态」：可发布、可校验、可消费的目录文档）。

非目标：Tool Pack 目录条目扩充（manifests/packs 现为 planned，本批的 catalog-publish 以 --manifests-dir 参数承载任意 Manifest 目录，工具包目录沿用同一机械）；Web 端回滚窗口列表页（CLI 承载）；update_check 的 manual 路径门控（合同口径：manual 检查全档可用）。

## 二、设计：信任级策略面（plugin-registry::trust_policy，单一事实来源）

1. `InstallConfirmation` 四级：`single`（official）/ `confirm`（community）/ `confirm_with_provenance`（local，展示来源核对）/ `double_confirm_typed`（remote，输入插件 id）。每级携带 `extra_steps`、`requires_provenance`、`requires_typed_id`、`required_confirm_fields`（服务端强制字段面：无 / [confirm] / [confirm, confirm_id]）。
2. `auto_update_prompt_allowed(level)`：D10 唯一判定面（仅 official 为 true）。`update_check` 的 manual 路径不经过本判定（合同 §6：其余须手动检查，检查本身不禁）。
3. `policy_json()`：四档策略表机器可读序列化，rp-plugin `trust-policy` 子命令承载，rp-server `/api/plugins/trust-policy` 端点进程内缓存一次，web 对话框经该端点消费（三方同源，不各自硬编码）。
4. 门控执行面：`PluginRuntime::update_check_auto`（新）按 active 行 trust_level 判定，拒绝时 Emit `plugin.update_check`（mode=auto, result=blocked）留痕后返回机器可读 `update_check_auto_not_allowed`；通过时与 manual 同语义并 Emit result=prompted | no_new_version。审计词形常量 `plugin_actions::UPDATE_CHECK` 为本批在 audit-engine 新增（既有 Emit 出口，不新增 JSONL 分叉）。
5. `TrustLevel::parse` 补齐（lock.rs，P10-2 起策略面与目录形态共用）。

服务端强制（apps/server/rp_server.py，安装与升级同一闸）：`_enforce_confirmation` 按策略表 `required_confirm_fields` 执行，community/local 要求 `confirm=true`，remote 另要求 `confirm_id` 与 Manifest id 一致（服务端经 rp-plugin `manifest-inspect` 预检解析，fail-closed）；策略不可得时 `_rp_plugin_json` 直接 502，不静默放行。升级信任级沿用当前 active 插件信任级（修掉 CLI 缺省 local 静默降档），确认强度随该级走。web 侧 `resolveTrustTier` 在策略不可得时 fail-closed 到最强档（double_confirm_typed），绝不静默降档。

## 三、设计：回滚窗口清理（upgrade.rs::cleanup_rollback_window）

1. 语义（§5.3 第 5 步 + D6）：非 active 旧版本行跃迁 UNINSTALLED（LOADED|ACTIVE 先 deactivate 复合步，与 uninstall 同款纪律），installed 目录物理删除仅对 data_root 之下执行（红线：绝不删调用方源目录，路径守卫 `path.starts_with(data_root)`）；active 版本与历史 UNINSTALLED 行不动；每次清理 Emit `plugin.uninstall`（detail result=rollback_window_cleanup）。
2. 未确认直接拒绝（`cleanup_requires_confirmation`），CLI `--confirm` 与 server body.confirm 同一口径。
3. 报告机器可读：`CleanupReport { cleaned, retained, dirs_removed, dirs_skipped: [{version, path, reason}], confirmed }`，skip 原因枚举 outside_data_root / illegal_transition / dir_missing / no_installed_dir。
4. 幂等：已是 UNINSTALLED 的行记 retained，二次清理 cleaned 为空。

## 四、设计：镜像 digest 台账（tool-engine::image_ledger + rp-image-pin CLI）

ADR §6:178「镜像回滚：镜像固定版本与 digest，更新先验证再切换，升级失败回滚旧 digest」的落地形态：

1. 台账为调用方给定路径的 JSON 文件（镜像状态属运行时数据；红线：源码零 rip-data 绝对路径字面量，路径一律显式传入）。记录 `{image, current_digest, previous_digest, updated_at_unix, history}`，history 有界（50 条，完整留痕走 audit 面）。
2. `pin`（初始入账，幂等）/ `update`（`verified=false` 时指针绝不切换，attempt 记 rolled_back，旧 digest 保持 current，即升级失败回滚旧 digest；`verified=true` 时切换并把旧 digest 移入回滚位）/ `rollback`（指针拨回 previous_digest，无回滚位拒绝）。
3. 「先验证」的真实链：digest 严格格式（`sha256:<64 小写 hex>`）+ 给 `--manifest` 时经 tool-registry `validate_static`（第 1/3/4/5/6/8 条，含 rule 5 的 provenance.image_digest 与 runtime.image digest 一致性）+ runtime.image digest 部分、provenance.image_digest、入参 digest 三方对账。验证失败即 `verified=false` 进自动回滚路径。
4. 成熟库优先：零新增 vendor（serde/serde_json/serde_yaml_ng/tool-registry 均为 tool-engine 既有依赖面），`[[bin]] rp-image-pin` 纯注册。

## 五、设计：插件目录发布形态（plugin-registry::catalog.rs）

1. 发布 = `build_catalog`（扫描 Manifest 目录 *.json，逐条目 trust_level + 由 trust_policy 推导的 install_confirmation 与 D10 位 + manifest_path 消费面）+ `write_catalog`（同目录临时文件 rename 原子落盘）+ `read_and_verify_catalog`（sha 完整性 + 排序 + schema 版本，任何不过即拒绝）。
2. 确定性：entries 按 plugin_id 排序，同 id 多版本拒绝（目录是发布态不是版本历史）；content_sha256 覆盖 schema_version + default_trust + entries 的规范化 JSON（generated_at_unix 不参与完整性），发布物可独立复核。
3. trust 语义：default_trust 由发布方显式给定（平台自带目录发布为 official，平台对目录内逐条 Manifest 的 pin 与许可政策负责），`--trust-override id=trust` 可逐条降档（外部来源混入时按实际来源）。
4. 消费闭环：条目 manifest_path 即 `rp-plugin install --manifest` 消费面；selftest 实演 publish → verify → 篡改拒绝 → 按条目安装。

## 六、实施步骤与文件清单

1. `crates/plugin-registry/src/trust_policy.rs`（新增）：确认强度四级 + D10 判定 + policy_json + 四组单测。
2. `crates/plugin-registry/src/catalog.rs`（新增）：目录发布形态三入口 + 四组单测（真实 manifests/plugins 载具）。
3. `crates/plugin-registry/src/lock.rs`：TrustLevel::parse 补齐。
4. `crates/plugin-registry/src/lib.rs`：模块登记与重导出。
5. `crates/audit-engine/src/lib.rs`：`plugin_actions::UPDATE_CHECK` 常量。
6. `crates/plugin-runtime/src/lib.rs`：`update_check_auto`（D10 门控 + Emit）、`cleanup_rollback_window` 委托方法、`trust_to_registry` 唯一映射面（build_lock 复用）。
7. `crates/plugin-runtime/src/upgrade.rs`：CleanupReport/SkippedDir + cleanup_rollback_window 执行面。
8. `crates/plugin-runtime/src/bin/rp-plugin.rs`：no-db 早分发四子命令（trust-policy / manifest-inspect / catalog-publish / catalog-verify）+ DB 面三子命令（update-check --auto / rollback-window / cleanup-rollback）。
9. `crates/plugin-runtime/tests/trust_gate_and_window.rs`（新增）：D10 门控、分级面一致性、窗口清理三红线五用例。
10. `crates/tool-engine/src/image_ledger.rs` + `src/bin/rp-image-pin.rs`（新增）+ Cargo.toml [[bin]]：镜像 digest 台账与 CLI。
11. `apps/server/rp_server.py`：`_rp_plugin_json`/`load_trust_policy`/`trust_tier`/`_enforce_confirmation`/`_active_plugin_trust`/`_resolve_repo_manifest`、install/upgrade 确认闸、cleanup-rollback 进写面白名单、POST manifest-inspect 与 GET trust-policy 两端点（本文件零直连 SQL 写不变，selftest_phase6 静态断言兼容）。
12. `web/src/lib/api.ts`：TrustPolicy/Tier 类型 + fetchTrustPolicy + resolveTrustTier（fail-closed）+ pluginManifestInspect + pluginInstall/pluginUpgrade 确认字段 + pluginCleanupRollback。
13. `web/src/components/TrustConfirmDialog.tsx`（新增）：四级确认对话框（typed id / provenance 勾选 / 禁用门）。
14. `web/src/routes/PluginsPage.tsx`：安装面板（Manifest 预检 → 分级确认 → 安装，D10 提示位展示）。
15. `web/src/routes/PluginDetailPage.tsx`：install/upgrade 一律走分级对话框（强度按当前信任级）+ 清理旧版本按钮。
16. `web/tests/trust-confirm.test.tsx`（新增）：五用例（official/local/remote/fail-closed/详情页升级闸）。
17. `web/tests/plugins-trust-confirm.spec.ts` + `web/playwright.plugins-trust.config.ts`（新增）：真实浏览器四级实测 + PW_CHROMIUM 三级探测配置。
18. `apps/server/selftest_phase10_plugin_eco.sh`（新增）：八步实演（镜像回滚 / D10 双向 / 升级失败回滚 / 窗口清理 / official 放行 / 目录发布 / server 强制闸 / 生产库指纹）。
19. `docs/phase10/P10-2-TASK.md`（本文件）与 `P10-2-BATCH-REPORT.md`、`docs/phase10/browser-e2e/`（5 截图 + RUN-LOG.md）。

## 七、验收命令与隔离纪律

验收命令（全部实测，输出见批次报告）：

```
cargo test --offline -p plugin-runtime -p plugin-registry        # 点名门禁（信任级门控 update_check 用例在列）
cargo test --offline --workspace                                 # 全仓回归
cargo clippy --offline --workspace --all-targets -- -D warnings  # clippy 门禁
bash apps/server/selftest_phase10_plugin_eco.sh                  # 两次实演回显判据
cd web && npx tsc --noEmit && npx vitest run && npm run build    # web 三件套
PW_CHROMIUM=<用户缓存 chrome> npx playwright test -c playwright.plugins-trust.config.ts   # 真实浏览器分级确认
```

隔离纪律：selftest 全部走 mktemp 工作目录与隔离插件库（plugin-eco.db / eco2 / eco3 三库隔离），生产 `migrations/platform*.db` 只读指纹核对（脚本前后各一次，diff 为空即零写）；image ledger 与 catalog 落盘全部在 WORK 内。

## 八、风险、已知限制与回滚

1. server 安装确认闸放行后的来源拉取在离线环境失败（git clone TLS 拒绝）：selftest 对该步取形态无关断言（响应不再是 install_confirmation_required 即证闸放行；HTTP 200 或 502 均可），400 闸前态由前置断言钉死。确认闸本身与信任级策略表是本批判据，来源可达性不是。
2. CLI stage_package 的 copy_tree 按设计跳过包内 checksums.sha256 与 plugin.lock（安装链重新生成），来源包清单篡改不进升级链：升级失败实演的破坏面取 Manifest 声明 entry 缺失（validation_failed）；checksum 篡改失败路径由单测 lifecycle_full_path::upgrade_switch_pointer_and_rollback_on_failure 固化（直调 runtime.upgrade，不经 staging）。
3. 目录条目安装面（selftest 第 6 步）用 --package 本地整树复制避开 git 拉取，离线可达；server 无 --package 形态按 P6-4 设计始终走来源拉取。
4. Playwright 分级确认为离线 mock 模式（P6-8 模式 A 同款）：route-mock 策略表与 manifest-inspect，写端点拦截回包；真实后端链路（含 server 400 闸）由 selftest 第 7 步以真实 rp_server 实测。
5. rustfmt 版本漂移沿用 P10-1 记录口径：本批新文件按本机 rustfmt 归一（10 文件实跑），未触碰文件不重排。
6. 回滚：`git revert <批次 commit>` 整批撤销；新增文件（trust_policy.rs、catalog.rs、image_ledger.rs、rp-image-pin.rs、trust_gate_and_window.rs、selftest、两 web 测试、对话框组件、playwright 配置、docs/phase10/browser-e2e）随 revert 移除；audit-engine UPDATE_CHECK 常量与 rp-server 两端点为纯增量。
7. 本批未 commit（工作流批次惯例由协调方收口）；生产库五库指纹零写（selftest 第 8 步实测）。
