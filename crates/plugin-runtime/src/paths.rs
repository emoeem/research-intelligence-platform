//! rip-data 目录布局与位置配置（PLUGIN-CONTRACT §8；ROADMAP P6-3）。
//!
//! 配置面（计划 409 行原文语义：目录位置通过配置指定，不硬编码到源码）：
//! - `rip.data_dir`：数据根，缺省随部署根（部署根缺省取进程 CWD），
//!   环境变量 `RIP_DATA_DIR` 覆盖；
//! - `rip.plugins.dir`：插件根，缺省 `{data_dir}/plugins`；单独配置时只改
//!   插件根，data_dir 其余子目录不受影响（互不干扰，测试锁定）。
//!
//! 目录骨架（§8）：
//! ~~~text
//! <data_dir>/
//!   <plugins_dir>/
//!     registry/                  来源目录与索引（catalog.json、registry-index.json），只存元数据
//!     installed/<id>/<version>/  manifest.yaml、capabilities/、docs/、data/、
//!                                checksums.sha256、plugin.lock
//!     disabled/                  计划原文目录；默认实现用状态标记而非物理移动（草案 D3）
//!     cache/                     下载、解压、索引构建缓存
//!     state/                     plugin-state.json + transitions.jsonl
//!     logs/                      插件运行日志
//! ~~~
//!
//! 红线对齐：源码零 rip-data 绝对路径字面量（缺省目录名
//! [`DEFAULT_DATA_DIR_NAME`] 是相对名，拼接部署根或配置给出根）；测试一律
//! 临时目录与内存库。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// 配置键：数据根（v1.json/文档口径统一引用本常量，防键名漂移）。
pub const CONFIG_KEY_DATA_DIR: &str = "rip.data_dir";

/// 配置键：插件根。
pub const CONFIG_KEY_PLUGINS_DIR: &str = "rip.plugins.dir";

/// 数据根环境变量覆盖。
pub const ENV_DATA_DIR: &str = "RIP_DATA_DIR";

/// 数据根缺省目录名（相对部署根；非绝对路径字面量）。
pub const DEFAULT_DATA_DIR_NAME: &str = "rip-data";

/// state 快照文件名（§8）。
pub const STATE_FILE_NAME: &str = "plugin-state.json";

/// 跃迁流水文件名（§8）。
pub const TRANSITIONS_FILE_NAME: &str = "transitions.jsonl";

/// 安装目录内 Manifest 统一落名（stage_package 写入）。
pub const MANIFEST_FILE_NAME: &str = "manifest.yaml";

/// 布局错误（机器可读）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PathsError {
    /// 稳定错误码：io / id_unsafe / manifest_missing。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl PathsError {
    /// 构造（模块内与相邻 state_store 共用词表）。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"code\":\"io\"}".to_string())
    }
}

impl std::fmt::Display for PathsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plugin paths error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for PathsError {}

fn io_err(context: &str, e: std::io::Error) -> PathsError {
    PathsError::new("io", format!("{context}: {e}"))
}

/// env 查找闭包类型（测试注入 mock 保持封闭；缺省 std::env::var）。
pub type EnvLookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// 解析参数（显式路径与 env 注入点；env 注入为闭包保持测试封闭）。
#[derive(Clone, Default)]
pub struct ResolveOptions<'a> {
    /// 部署根（缺省 CWD；仅 data_dir 与 plugins_dir 均未给定时参与缺省推导）。
    pub deployment_root: Option<&'a Path>,
    /// rip.data_dir 显式值。
    pub data_dir: Option<&'a Path>,
    /// rip.plugins.dir 显式值。
    pub plugins_dir: Option<&'a Path>,
    /// env 查找闭包（缺省 std::env::var；测试注入 mock 保持封闭）。
    pub env: Option<EnvLookup<'a>>,
}

/// 解析后的插件布局路径集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginPaths {
    /// 数据根（rip.data_dir）。
    pub data_dir: PathBuf,
    /// 插件根（rip.plugins.dir）。
    pub plugins_dir: PathBuf,
}

impl PluginPaths {
    /// 按优先级解析：显式值 > `RIP_DATA_DIR` > 部署根/`rip-data`；
    /// plugins_dir：显式值 > `{data_dir}/plugins`。
    pub fn resolve(options: ResolveOptions<'_>) -> Result<PluginPaths, PathsError> {
        let env_lookup = |key: &str| -> Option<String> {
            match options.env {
                Some(f) => f(key),
                None => std::env::var(key).ok().filter(|v| !v.is_empty()),
            }
        };
        let data_dir = match (
            options.data_dir,
            env_lookup(ENV_DATA_DIR).map(PathBuf::from),
        ) {
            (Some(p), _) => p.to_path_buf(),
            (None, Some(p)) => p,
            (None, None) => {
                let root: PathBuf = match options.deployment_root {
                    Some(p) => p.to_path_buf(),
                    None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                };
                root.join(DEFAULT_DATA_DIR_NAME)
            }
        };
        let plugins_dir = match options.plugins_dir {
            Some(p) => p.to_path_buf(),
            None => data_dir.join("plugins"),
        };
        Ok(PluginPaths {
            data_dir,
            plugins_dir,
        })
    }

    // ------------------------------------------------------------------
    // §8 子目录视图
    // ------------------------------------------------------------------

    /// 来源目录与索引（只存元数据）。
    pub fn registry_dir(&self) -> PathBuf {
        self.plugins_dir.join("registry")
    }

    /// 已安装根。
    pub fn installed_root(&self) -> PathBuf {
        self.plugins_dir.join("installed")
    }

    /// 物理隔离目录（草案 D3：默认状态标记，不移动；目录保留为可选隔离位）。
    pub fn disabled_dir(&self) -> PathBuf {
        self.plugins_dir.join("disabled")
    }

    /// 下载/解压/索引缓存。
    pub fn cache_dir(&self) -> PathBuf {
        self.plugins_dir.join("cache")
    }

    /// 状态目录。
    pub fn state_dir(&self) -> PathBuf {
        self.plugins_dir.join("state")
    }

    /// 运行日志目录。
    pub fn logs_dir(&self) -> PathBuf {
        self.plugins_dir.join("logs")
    }

    /// 版本安装目录 installed/<id>/<version>/。
    pub fn installed_plugin_dir(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<PathBuf, PathsError> {
        safe_component(plugin_id, "plugin_id")?;
        safe_component(version, "version")?;
        Ok(self.installed_root().join(plugin_id).join(version))
    }

    /// state/plugin-state.json。
    pub fn state_file(&self) -> PathBuf {
        self.state_dir().join(STATE_FILE_NAME)
    }

    /// state/transitions.jsonl。
    pub fn transitions_file(&self) -> PathBuf {
        self.state_dir().join(TRANSITIONS_FILE_NAME)
    }

    // ------------------------------------------------------------------
    // 布局建立与拉取落位
    // ------------------------------------------------------------------

    /// 建立 §8 骨架（幂等）：plugins 根 + registry/installed/disabled/cache/
    /// state/logs 六子目录；已存在的跳过（报告区分 created/existed）。
    pub fn ensure_layout(&self) -> Result<LayoutReport, PathsError> {
        let mut report = LayoutReport {
            plugins_dir: self.plugins_dir.to_string_lossy().to_string(),
            created: Vec::new(),
            existed: Vec::new(),
        };
        let dirs = [
            self.plugins_dir.clone(),
            self.registry_dir(),
            self.installed_root(),
            self.disabled_dir(),
            self.cache_dir(),
            self.state_dir(),
            self.logs_dir(),
        ];
        for dir in dirs {
            if dir.exists() {
                report.existed.push(dir.to_string_lossy().to_string());
            } else {
                fs::create_dir_all(&dir)
                    .map_err(|e| io_err(&format!("create_dir {:?}", dir.display()), e))?;
                report.created.push(dir.to_string_lossy().to_string());
            }
        }
        Ok(report)
    }

    /// 拉取落位：把来源 Manifest 与包内容复制进
    /// installed/<id>/<version>/（§5.2 install 行的落盘半边）。
    ///
    /// - Manifest 文件统一落名为 `manifest.yaml`（.json 来源亦同，内容不变）；
    /// - 包内容递归复制，跳过 checksums.sha256 与 plugin.lock（安装时由
    ///   plugin-registry 重新生成，防旧清单遮蔽新内容）；
    /// - 同版本目录已存在时先清空重建（重装语义）；
    /// - plugin_id / version 做路径分量安全检查（防穿越）。
    pub fn stage_package(
        &self,
        plugin_id: &str,
        version: &str,
        manifest_src: &Path,
        package_src: Option<&Path>,
    ) -> Result<PathBuf, PathsError> {
        let target = self.installed_plugin_dir(plugin_id, version)?;
        if !manifest_src.is_file() {
            return Err(PathsError::new(
                "manifest_missing",
                format!("manifest {:?} is not a file", manifest_src.display()),
            ));
        }
        if target.exists() {
            fs::remove_dir_all(&target)
                .map_err(|e| io_err(&format!("reset {:?}", target.display()), e))?;
        }
        fs::create_dir_all(&target)
            .map_err(|e| io_err(&format!("create_dir {:?}", target.display()), e))?;
        fs::copy(manifest_src, target.join(MANIFEST_FILE_NAME))
            .map_err(|e| io_err("copy manifest", e))?;
        if let Some(src) = package_src {
            if !src.is_dir() {
                return Err(PathsError::new(
                    "io",
                    format!("package {:?} is not a directory", src.display()),
                ));
            }
            copy_tree(src, &target)?;
        }
        Ok(target)
    }
}

/// ensure_layout 结果（机器可读）。
#[derive(Debug, Clone, Serialize)]
pub struct LayoutReport {
    /// 插件根。
    pub plugins_dir: String,
    /// 本次新建的目录。
    pub created: Vec<String>,
    /// 已存在的目录。
    pub existed: Vec<String>,
}

impl LayoutReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"created\":[]}".to_string())
    }
}

/// 路径分量安全检查：拒绝空、绝对、父跳与分隔符（防路径穿越，与
/// plugin-registry rule 7 entry 检查同款 fail-closed 规则）。
fn safe_component(component: &str, field: &str) -> Result<(), PathsError> {
    let bad = component.is_empty()
        || component.starts_with('/')
        || component.starts_with('\\')
        || component
            .split(['/', '\\'])
            .any(|seg| seg == ".." || seg == ".");
    if bad {
        return Err(PathsError::new(
            "id_unsafe",
            format!("{field} {component:?} is not a safe path component (traversal rejected)"),
        ));
    }
    Ok(())
}

/// 递归复制目录树（跳过 checksums.sha256 与 plugin.lock，安装时重新生成）。
fn copy_tree(src: &Path, dst: &Path) -> Result<(), PathsError> {
    for entry in
        fs::read_dir(src).map_err(|e| io_err(&format!("read_dir {:?}", src.display()), e))?
    {
        let entry = entry.map_err(|e| io_err("dir entry", e))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == plugin_registry::CHECKSUMS_FILENAME
            || name == plugin_registry::PLUGIN_LOCK_FILENAME
        {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| io_err(&format!("create_dir {:?}", target.display()), e))?;
            copy_tree(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| io_err(&format!("copy {:?}", path.display()), e))?;
        }
    }
    Ok(())
}
