# RT-0.5-F：Capability Provider 契约设计

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-F）。**只定义接口、数据结构、缓存/失效策略；本批不实现生产 Adapter**（设计 §6 步骤 0.5-F / §8）；实现与消费在 RT-1
- 依据：[RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §3.1/§3.2/§3.3；[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3/§12
- 证据来源：rt05-probes/ 五份 2026-11-18 只读实测（引用格式：实测见 rt05-probes/probe-*.md #N）
- 数据实况：[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md)（目录即本契约的首个数据快照）；路由与隔离：[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md)

## 1. 定位与边界

1. Capability Provider 是 **环境事实的读取与探测层**：回答"这台宿主机有什么软件、什么版本、什么子能力、是否仍可信"。它是 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) 目录的运行时载体，不是第二套 Tool Registry（设计 §3.1 末段）。
2. 不承载工具语义：无 input_schema/output_schema/ToolRun/Artifact 职责；这些仍归 Tool Registry / Tool Engine（策略 §4.1）。
3. 探测只读：不处理用户文件、不访问网络、不触发插件下载或自动更新（设计 §3.2）。
4. 不信任调用方路径：探测与执行一律走受控 search roots + 绝对路径归一 + 身份验证（设计 §3.2；实测见 rt05-probes/probe-path-audit.md verdict）。

## 2. CapabilityEntry 数据结构（设计 §3.1 全字段映射）

```rust
/// 稳定能力标识，如 "media.ffmpeg"、"document.libreoffice"、"ocr.tesseract"
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityId(Arc<str>);          // 域.名，两段小写

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {                 // 设计 §3.1 五态
    Available,
    Missing,
    Incompatible,
    ProbeFailed,
    Disabled,
}

/// 探测失败五态（设计 §3.2"探测失败不等于软件不存在"），与 ProbeError 一一对应
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeErrorKind { Missing, PermissionDenied, Timeout, UnsupportedOutput, ProbeFailed }

#[derive(Debug, Clone)]
pub struct BinaryIdentity {
    pub sha256_hex: String,                 // 全量 sha256（取证登记口径为先 16 位，见 C §5.1）
    pub file_type: String,                  // "elf-pie" | "shell-script" | "perl-script" | "symlink"
    pub owner_mode: String,                 // "root:root 755" 等
    pub wrapper_target: Option<Box<BinaryIdentity>>, // wrapper 场景：exec 目标身份（7z/soffice）
}

#[derive(Debug, Clone)]
pub enum PackageProvenance {
    Pacman { name: String, version: String },   // 实测口径：pacman -Qo
    Unknown,                                     // 无法确认时显式 unknown，不猜测
}

#[derive(Debug, Clone)]
pub struct ProbedFeature {                  // L2 能力集合的元素
    pub kind: FeatureKind,                  // Encoder | Muxer | Language | Format | Delegate | Policy | Component
    pub name: String,                       // "libx264" / "chi_sim" / "mp4" / "gs" / ...
    pub flags: Vec<String>,                 // 如 ["experimental"]（ffmpeg 原生 opus/vorbis）
}

#[derive(Debug, Clone)]
pub struct Constraint {                     // 已知限制，供 Policy Resolver 读取
    pub code: ConstraintCode,               // ExperimentalEncoder | LanguageMissing | PolicyEmptyShell
    pub detail: String,                     // 人类可读；对应 C §3 各条 constraints
}

#[derive(Debug, Clone)]
pub struct SecurityProfile {                // 允许的调用方式、输入风险等级、隔离要求
    pub allowed_call_modes: Vec<CallMode>,  // RestrictedNativeSubprocess | IsolatedContainer | VmIsolated
    pub max_input_risk: InputRisk,          // TrustedLowRisk | Untrusted | HighRisk
    pub notes: Vec<String>,                 // E 文档 §4 本机特化裁决的引用锚
}

#[derive(Debug, Clone)]
pub struct CapabilityEntry {                // 设计 §3.1 十二字段
    pub capability_id: CapabilityId,
    pub provider: String,                          // 软件名 + 发行项目
    pub executable_path: PathBuf,                  // 经验证绝对路径
    pub version: VersionInfo,                      // { normalized, raw }
    pub binary_identity: BinaryIdentity,
    pub package_provenance: PackageProvenance,
    pub capabilities: Vec<ProbedFeature>,          // L2 能力集合（按 FeatureKind 分组）
    pub status: CapabilityStatus,
    pub probe_timestamp: OffsetDateTime,
    pub fingerprint: Fingerprint,                  // 见 §5 公式
    pub constraints: Vec<Constraint>,
    pub security_profile: SecurityProfile,
}

/// 缓存读出的包装：携带新鲜度，stale 条目不可供 Runtime Policy 决策使用
#[derive(Debug, Clone)]
pub struct CachedEntry {
    pub entry: CapabilityEntry,
    pub freshness: Freshness,               // Fresh | Stale { reason: StaleReason }
    pub last_probe_level: ProbeLevel,       // 该条目上次实际探到的层级
}
```

字段与 RT-0.5-C 目录逐条对齐：capability_id/provider/executable_path/version/binary_identity/package_provenance/capabilities/status/probe_timestamp/fingerprint/constraints/security_profile 十二字段一个不缺（见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §2 映射表）。

## 3. CapabilityProvider trait（Rust 伪码，签名冻结、实现归 RT-1）

```rust
/// 探测分层（设计 §3.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProbeLevel {
    L0Static,       // 受控路径查找、文件元信息、包数据库归属（无子进程执行目标二进制）
    L1Version,      // 固定版本参数（--version / -v / -ver），短超时
    L2Capability,   // 只读能力清单（-encoders / --list-langs / 7z i / -list delegate / --help 面等）
    L3Smoke,        // 内置 fixture 无副作用验证；仅安装/升级验收或工具首次启用，禁入启动路径
}

/// 探测预算：探测命令固定、参数固定、无 Shell 拼接；短超时 + 输出上限 + 取消/回收（设计 §3.2）
#[derive(Debug, Clone)]
pub struct ProbeBudget {
    pub timeout: Duration,
    pub max_output_bytes: u64,
    pub cancel: CancelToken,                // 协作式取消；进程组回收由实现保证
}
// 建议初值（RT-1 定稿；本批实测口径为每条 timeout 5s，见 rt05-probes/* 头注）：
//   L0 ≤ 1s（纯 stat/pacman 查询）、L1 ≤ 5s、L2 ≤ 10s、L3 按工具登记。

#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    Entry(CapabilityEntry),                          // 探测成功（含 available/missing 等状态判定）
    NotProbed { level_reached: ProbeLevel },         // 请求层级超出预算/被策略禁用（如 L3 非验收场景）
}

#[derive(Debug, Clone)]
pub enum ProbeError {                    // 错误五态（设计 §3.2）
    Missing { searched_roots: Vec<PathBuf> },
    PermissionDenied { path: PathBuf },
    Timeout { level: ProbeLevel, elapsed: Duration },
    UnsupportedOutput { detail: String },        // 命令执行了但输出形态不符（版本行无法解析等）
    ProbeFailed { detail: String },              // 其余失败（退出码异常、进程崩溃等）
}

pub trait CapabilityProvider: Send + Sync {
    /// 分层探测：level 即请求层级；实现按 §4 语义逐层聚合，返回该层级完整 Entry
    fn probe(&self, id: &CapabilityId, level: ProbeLevel, budget: ProbeBudget)
        -> Result<ProbeOutcome, ProbeError>;

    /// 缓存读：命中 Fresh 条目直接返回；Stale 返回 Stale 条目并由调用方决定是否重探
    fn get(&self, id: &CapabilityId) -> Option<CachedEntry>;

    /// 目录检索（按域前缀/状态过滤），如 list(filter) -> 全部 available 能力
    fn list(&self, filter: &CapabilityFilter) -> Vec<CachedEntry>;

    /// 当前指纹（无条目时 None）
    fn fingerprint(&self, id: &CapabilityId) -> Option<Fingerprint>;

    /// 失效事件入队（见 §5.3）；返回被标 stale 的条目数
    fn invalidate(&mut self, event: &InvalidationEvent) -> usize;

    /// 后台/按需重探（L2 层），受 ProbeBudget 约束；不阻塞服务启动（设计 §3.3）
    fn refresh(&mut self, id: &CapabilityId, level: ProbeLevel, budget: ProbeBudget)
        -> Result<CachedEntry, ProbeError>;
}

#[derive(Debug, Clone)]
pub struct CapabilityFilter {
    pub domain_prefix: Option<String>,      // "pdf." / "archive." / "media." ...
    pub status: Option<CapabilityStatus>,
    pub feature: Option<FeatureQuery>,      // 如 "language=chi_tra" / "muxer=webm"
}
```

## 4. probe(level) 分层语义（设计 §3.3 落地）

| 层级 | 实现动作 | 读目标二进制？ | 预算 | 本批实测对应 |
|---|---|---|---|---|
| L0Static | 受控 search roots 查找（受信前缀优先，用户可写目录与 /tmp 挂载元素排除）→ 绝对路径归一（readlink -f 等价逻辑）→ file 类型/stat 权限/sha256 → pacman -Qo 包归属 | 否（不执行目标） | ≤1s | 五份 probe 全部 L0 字段（实测见 rt05-probes/probe-path-audit.md #2 #3、rt05-probes/probe-av-ocr.md #1 #2 #10 #12） |
| L1Version | 固定参数版本探测（--version/-v/-ver/banner），输出解析为 normalized+raw | 是（只读执行） | ≤5s | 实测见 rt05-probes/probe-av-ocr.md #3 #7 #8；rt05-probes/probe-pdf-image.md #4 |
| L2Capability | 只读清单命令（ffmpeg -encoders/-muxers、tesseract --list-langs、pandoc --list-*、7z i、magick -list delegate/-list policy、soffice --help 面） | 是（只读执行） | ≤10s，按需或后台 | 实测见 rt05-probes/probe-av-ocr.md #4 #5 #6 #9；rt05-probes/probe-archive-meta.md #6 #7 #8 #10；rt05-probes/probe-pdf-image.md #8 #9 #10 |
| L3Smoke | 内置小型 fixture 无副作用验证（如单页 PDF 抽取、1 秒静音转码），产物写任务临时区后清理 | 是 | 按工具登记；**仅在安装/升级验收或工具首次启用时执行，不得在每次服务启动时运行** | 本批未执行（设计 §3.3） |

分层聚合规则：请求 L2 时实现必须先完成 L0+L1（身份/版本先于能力面）；任一层失败即返回对应 ProbeError，不返回半截 Entry。L0/L1 失败不等于软件不存在（五态区分，设计 §3.2）。

search roots 规则（设计 §3.2 + 实测依据）：候选目录 = 受信前缀（/usr/local/bin、/usr/bin 及其 usrmerge 别名 /usr/sbin、/sbin、/bin）+ 包管理器数据库归属校验；显式排除当前工作目录、/tmp、用户可写目录（~/.local/bin、~/.cargo/bin、pnpm、flatpak exports）优先级——这些目录中的同名程序不得当作系统可信能力（实测见 rt05-probes/probe-path-audit.md verdict、#2）。

## 5. 缓存 TTL 与失效事件

### 5.1 指纹公式（与 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5.1 一致）

```text
fingerprint = sha256( canonical_json({
    bin_sha256,        // 全量 sha256；wrapper 条目另含 wrapper_target.sha256
    version,           // normalized 版本串
    capset,            // capabilities 排序后 canonical 序列化
}) ) 前 16 位十六进制
```

本批取证登记为二进制 sha256 前 16 位（仅 archive.7z wrapper 有全值，实测见 rt05-probes/probe-archive-meta.md #4），完整指纹由 Provider 首探时计算；目录文档不预生成、不编造。

### 5.2 TTL（建议值，RT-1 定稿；设计 §3.2"启动轻量校验 + 定期/文件变化重探"）

| 层级 | 缓存内容 | 生命周期 |
|---|---|---|
| L0/L1 | 路径/身份/包归属/版本 | 启动时轻量指纹校验（仅 stat+sha256，不执行目标）；命中即用；TTL 24h |
| L2 | 能力集合 | 按需或后台重探；TTL 7d；绝不阻塞启动（设计 §3.3） |
| L3 | smoke 结论 | 不进常规缓存；验收/首次启用一次性执行 |

### 5.3 失效事件（变化即 stale 重探；设计 §3.2）

```rust
pub enum InvalidationEvent {
    BinaryChanged { capability_id: CapabilityId, old: Fingerprint, new: Fingerprint },
    PackageChanged { package: String, old_version: String, new_version: String },
    CapabilitySetChanged { capability_id: CapabilityId },     // 语言包/编码器/格式清单增删
    ConfigFileChanged { path: PathBuf },                     // /etc/ImageMagick-7/{policy,delegates}.xml 等
    ProbeFailed { capability_id: CapabilityId, kind: ProbeErrorKind },
    ManualInvalidate { capability_id: Option<CapabilityId> },
}
```

- 任一事件命中即把旧条目标记 `Freshness::Stale { reason }`；stale 条目仅供诊断展示，**不可供 Runtime Policy 决策与路由使用**。
- 重探沿用 stale 前的 last_probe_level；重探失败回退为对应错误态条目（status=ProbeFailed 或 Missing），不沿用旧结果。
- 触发源：包管理器事务事件、config 文件 watch（mtime+sha256）、运行中探测输出与缓存指纹不符、外部手动失效。

## 6. 与 Tool Engine / Tool Registry 的集成点（只定义，不实现）

```text
Agent/Workflow
   -> Tool Registry：工具事实（Manifest：id/input_schema/output_schema/runtime.type/permission/network）
        Manifest 新增可选字段（RT-4 迁移时逐项评估，本批不改 Manifest）：
        capability_refs: ["pdf.pdftotext", "pdf.pdftoppm", "ocr.tesseract"]
   -> Tool Engine 派发前：
        for cap in manifest.capability_refs:
            match provider.get(cap):
                Some(entry) if entry.freshness == Fresh and entry.status == Available -> 继续
                Some(entry) if stale -> provider.refresh(cap, entry.last_probe_level, budget) 后重判
                None / Missing -> 派发拒绝，错误信封注明 capability 与五态原因
   -> Runtime Policy Resolver（RT-3）：
        输入 = Manifest(runtime.type, permission_level, network)
             + CapabilityEntry(status/version/constraints/security_profile)
             + 输入风险分类（E §1）
        输出 = 执行后端 + 资源预算 + 隔离要求；拒绝不安全降级（策略 §9 RT-3 行）
   -> Adapter 执行（RT-1）：
        原生路径用 entry.executable_path 绝对派发（allowlist 校验 binary_identity.fingerprint）
        执行参数模板由 Manifest/Policy 承载，Agent 不得拼接任意命令（策略 §3.1）
```

集成纪律：

1. **单一事实源**：Registry 管工具、Provider 管环境，通过 capability_refs 单向引用连接；Provider 不注册工具、Registry 不探测环境（见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §6）。
2. 现有 MCP dispatch 的 python_worker / podman_host 分支语义不变（RT-0 §4 代码证据：crates/mcp-server/src/dispatch.rs）；Provider 接入发生在派发前置校验层，不改既有协议。
3. 健康检查统一暴露：Runtime 健康检查接口增加 capability 摘要（id/status/version/fingerprint），与策略 §4.1"Runtime 健康检查、能力声明和版本信息统一暴露"对齐。
4. 本批不实现：无 Provider struct 实现、无 Adapter 接线、无 Cargo 变更；接口由 RT-1 消费（设计 §6/§8；策略 §9）。

## 7. 错误信封与五态映射

| ProbeErrorKind | 判定条件 | Catalog status | 路由行为（E §3） |
|---|---|---|---|
| Missing | search roots 内无该名；包库无归属 | Missing | 拒绝派发 + 报告成熟替代（如 mutool→poppler，见 D §4.1） |
| PermissionDenied | 路径存在但 stat/执行被拒 | Incompatible（环境异常） | 拒绝派发 + 报告权限事实，不自动提权 |
| Timeout | 超过 ProbeBudget.timeout | ProbeFailed | 拒绝派发 + 标 stale 重探 |
| UnsupportedOutput | 执行成功但输出形态不符（无法解析版本/清单） | ProbeFailed | 拒绝派发 + 记录原始输出摘要（截断） |
| ProbeFailed | 其余失败（非零退出、崩溃） | ProbeFailed | 拒绝派发 + ToolRun 审计 |

- 五态区分是硬要求：探测失败不等于软件不存在，禁止把 timeout/probe_failed 报成 missing（设计 §3.2）。
- 本批实测分布：仅 Missing 命中（pdf.mutool、chi_tra），其余四态无实测案例（见 [RT-0.5-B-CAPABILITY-PROBES.md](RT-0.5-B-CAPABILITY-PROBES.md) §11）。

## 8. RT-1 消费清单（后续实现任务书输入）

1. 实现 `CapabilityProvider`（进程内缓存 + pacman 归属校验 + 受控 search roots），落地 C §5 缓存/失效与 F §5 事件。
2. 以 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) 16 条为种子数据做首次 L0/L1 全量校验（对照本批取证 hash/版本）。
3. 为 Manifest 增加 capability_refs 可选字段解析（不改现有 19 个 Manifest 的既有字段）。
4. Native CLI Adapter（RT-1 主体）按 entry.executable_path + security_profile.allowed_call_modes 执行，落实 E §2 资源限制清单（原生档 CPU/内存强制机制锚与"机制不可用即拒绝派发"行为见 E §2 注 A）。
5. L3 smoke 用例随工具首次启用逐个补充，不进启动路径（设计 §3.3）。
