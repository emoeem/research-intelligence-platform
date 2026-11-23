//! 系统字体发现面（PHASE-6B D2——零安装红线：字体从系统目录取，零打包入库）。
//!
//! 分层纪律（D2①）：本模块是 report-engine **唯一 I/O 面**——渲染函数
//! （render_pdf/render_docx/render_attack_path_svg/qc_*）全部纯函数零 I/O，
//! 字体字节经 [`FontSource`] 参数注入。判据 4 零 I/O 反证按渲染函数口径
//! grep（本模块不在反证清单——D7 词形）。
//!
//! 扫描目标 = 标准系统字体目录四件（D2① 词形）：`/usr/share/fonts`、
//! `/usr/local/share/fonts`、`~/.local/share/fonts`、`~/.fonts`。遍历用
//! `walkdir`（基线在位——D2① 词形点名；Cargo.lock 零增量复用）。收集
//! .ttf/.otf/.ttc；.ttc 按 skrifa 可解析面拆多份 [`FontSource`]（Noto CJK =
//! .ttc collection——D2②）。解析失败面 fail-soft：跳过不可解析文件不
//! panic（红线 1）。
//!
//! 结果排序 = 路径字典序（确定性——同环境两次发现同序）。

use std::path::PathBuf;
use walkdir::WalkDir;

/// 一个字体面：原始字节 + TrueType collection 内的 index（.ttc 面经 index
/// 选位；单字体文件恒 index = 0——D2②）。
#[derive(Debug, Clone)]
pub struct FontSource {
    /// 字体文件原始字节（.ttf/.otf 单字体或 .ttc 全集合）。
    pub data: Vec<u8>,
    /// collection 内字面索引（FontRef::from_index 语义）。
    pub index: u32,
}

/// 系统字体标准目录四件（D2① 词形；前两件绝对路径，后两件按 `$HOME` 展开）。
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    dirs
}

/// 发现系统字体（唯一 I/O 面——D2①/D7）：扫描 [`system_font_dirs`] 四目录，
/// walkdir 递归收集 .ttf/.otf/.ttc 并经 skrifa 可解析性过滤（.ttc 拆面）。
///
/// fail-soft 口径：目录不存在跳过、不可解析文件跳过、零字体时返回空集
/// （调用方渲染降级 .notdef + qc fail——D2④，禁 panic）。
pub fn discover_system_fonts() -> Vec<FontSource> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for dir in system_font_dirs() {
        for entry in WalkDir::new(&dir).max_open(8).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let is_font = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e, "ttf" | "otf" | "ttc" | "otc"))
                .unwrap_or(false);
            if is_font {
                paths.push(entry.into_path());
            }
        }
    }
    // 确定性：walkdir 无序——路径字典序排序。
    paths.sort();
    paths.dedup();

    let mut fonts = Vec::new();
    for path in paths {
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "ttc" | "otc" => {
                // collection：按 skrifa 可解析面拆 index（fail-soft——零可
                // 解析面时整个文件跳过）。
                for (index, face) in skrifa::FontRef::fonts(&data).enumerate() {
                    if face.is_ok() {
                        fonts.push(FontSource {
                            data: data.clone(),
                            index: index as u32,
                        });
                    }
                }
            }
            _ => {
                if skrifa::FontRef::new(&data).is_ok() {
                    fonts.push(FontSource { data, index: 0 });
                }
            }
        }
    }
    fonts
}
