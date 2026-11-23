//! OOXML stored-ZIP 容器（PHASE-6B D3③——XLSX/PPTX 第二批格式复用底座）。
//!
//! D3 裁决背景：六 DOCX crate 全停批（A12 probe 实录）后，手写 OOXML
//! 序列化 + stored（无压缩）ZIP 容器。stored = ZIP 规范最小合法面
//! （local file header + central directory + EOCD），Word/WPS 接受 stored
//! 条目；零压缩零 flate2 依赖。CRC32 经 crc32fast =1.5.5（D3——标准实现
//! 零手写算法正确性负担）。
//!
//! 纯函数面零 I/O：`OoxmlZip` 只做内存组装——`build()` 消费自身产出完整
//! ZIP 字节。时间字段取固定 DOS 时刻（1980-01-01 00:00——ZIP 内部字段词形
//! 恒定 = 同条目两次 build 字节一致，确定性判据面）。
//!
//! 本模块同时提供 `qc_zip_entries` 对账面（crc32 校验 + 结构完整性——
//! qc_docx「表格不溢出/资产正确性」面的容器侧承载）。

use crc32fast;

/// CRC32 单点（crc32fast 封装——D3 容器 CRC 单点词形）。
pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// 固定 DOS 时间字段（1980-01-01 00:00：time = 0x0000，date = 0x0021）——
/// ZIP 字节确定性钉。
const DOS_TIME: u16 = 0x0000;
const DOS_DATE: u16 = 0x0021;
/// 本容器 flags：bit 11（UTF-8 文件名）。
const UTF8_FLAG: u16 = 0x0800;
/// stored 方法号（无压缩）。
const METHOD_STORED: u16 = 0;
/// ZIP 版本需求（2.0 = 20）。
const VERSION_NEEDED: u16 = 20;

/// 一个 ZIP 条目（文件名 + 数据；保持插入序——[Content_Types].xml 恒首位
/// 惯例由调用面保证）。
struct Entry {
    name: Vec<u8>,
    data: Vec<u8>,
    crc: u32,
    /// local file header 在产物中的偏移（build 期计算）。
    offset: u32,
}

/// OOXML 容器 builder（内存组装零 I/O）。
#[derive(Default)]
pub struct OoxmlZip {
    entries: Vec<Entry>,
}

impl OoxmlZip {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加条目（CRC32 即时计算——单点承载）。
    pub fn add(&mut self, name: &str, data: impl Into<Vec<u8>>) {
        let data = data.into();
        let crc = crc32(&data);
        self.entries.push(Entry {
            name: name.as_bytes().to_vec(),
            data,
            crc,
            offset: 0,
        });
    }

    /// 条目数（qc 对账面）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 组装完整 stored-ZIP 字节（local file header×n + central directory×n
    /// + EOCD；确定性——同条目两次 build 字节一致）。
    pub fn build(mut self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(1024);
        // local file headers + data。
        for entry in &mut self.entries {
            entry.offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&VERSION_NEEDED.to_le_bytes());
            out.extend_from_slice(&UTF8_FLAG.to_le_bytes());
            out.extend_from_slice(&METHOD_STORED.to_le_bytes());
            out.extend_from_slice(&DOS_TIME.to_le_bytes());
            out.extend_from_slice(&DOS_DATE.to_le_bytes());
            out.extend_from_slice(&entry.crc.to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(&entry.name);
            out.extend_from_slice(&entry.data);
        }
        // central directory。
        let cd_offset = out.len() as u32;
        for entry in &self.entries {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            out.extend_from_slice(&VERSION_NEEDED.to_le_bytes()); // made by
            out.extend_from_slice(&VERSION_NEEDED.to_le_bytes()); // needed
            out.extend_from_slice(&UTF8_FLAG.to_le_bytes());
            out.extend_from_slice(&METHOD_STORED.to_le_bytes());
            out.extend_from_slice(&DOS_TIME.to_le_bytes());
            out.extend_from_slice(&DOS_DATE.to_le_bytes());
            out.extend_from_slice(&entry.crc.to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(&0u16.to_le_bytes()); // comment len
            out.extend_from_slice(&0u16.to_le_bytes()); // disk start
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            out.extend_from_slice(&entry.offset.to_le_bytes());
            out.extend_from_slice(&entry.name);
        }
        let cd_size = out.len() as u32 - cd_offset;
        // EOCD。
        let n = self.entries.len() as u16;
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }
}

/// ZIP 容器逐条目对账（qc 面：CRC32 全对 + 结构完整性——D6②「DOCX zip
/// central directory + CRC32 全对」承载）。返回 Err(说明) = 结构损坏。
///
/// 实现为纯函数：输入完整 ZIP 字节，解析 local headers/central directory/
/// EOCD 并逐条目对账（判据 6 自解析断言面 + 负例 CRC 篡改 fail 面共用）。
pub fn qc_zip_entries(bytes: &[u8]) -> Result<Vec<(String, u32, usize)>, String> {
    // EOCD 定位（从尾部反向搜 0x06054b50——注释长度固定 0，允许安全尾搜）。
    let eocd_sig = 0x0605_4b50u32.to_le_bytes();
    let eocd_pos = bytes
        .windows(4)
        .rposition(|w| w == eocd_sig)
        .ok_or("EOCD signature missing")?;
    if eocd_pos + 22 > bytes.len() {
        return Err("EOCD truncated".to_string());
    }
    let n_entries = u16::from_le_bytes([bytes[eocd_pos + 10], bytes[eocd_pos + 11]]) as usize;
    let cd_size = u32::from_le_bytes([
        bytes[eocd_pos + 12],
        bytes[eocd_pos + 13],
        bytes[eocd_pos + 14],
        bytes[eocd_pos + 15],
    ]);
    let cd_offset = u32::from_le_bytes([
        bytes[eocd_pos + 16],
        bytes[eocd_pos + 17],
        bytes[eocd_pos + 18],
        bytes[eocd_pos + 19],
    ]);
    if cd_offset as usize + cd_size as usize > bytes.len() {
        return Err("central directory out of bounds".to_string());
    }
    // central directory 逐条目。
    let mut entries: Vec<(String, u32, usize)> = Vec::with_capacity(n_entries);
    let mut pos = cd_offset as usize;
    for _ in 0..n_entries {
        if pos + 46 > bytes.len() {
            return Err("central directory entry truncated".to_string());
        }
        let sig = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        if sig != 0x0201_4b50 {
            return Err(format!("bad central directory signature at {pos}"));
        }
        let crc = u32::from_le_bytes([
            bytes[pos + 16],
            bytes[pos + 17],
            bytes[pos + 18],
            bytes[pos + 19],
        ]);
        let compressed = u32::from_le_bytes([
            bytes[pos + 20],
            bytes[pos + 21],
            bytes[pos + 22],
            bytes[pos + 23],
        ]) as usize;
        let uncompressed = u32::from_le_bytes([
            bytes[pos + 24],
            bytes[pos + 25],
            bytes[pos + 26],
            bytes[pos + 27],
        ]) as usize;
        if compressed != uncompressed {
            return Err("non-stored entry (compressed) in stored-only container".to_string());
        }
        let name_len =
            u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let extra_len =
            u16::from_le_bytes([bytes[pos + 30], bytes[pos + 31]]) as usize;
        let comment_len =
            u16::from_le_bytes([bytes[pos + 32], bytes[pos + 33]]) as usize;
        let local_offset = u32::from_le_bytes([
            bytes[pos + 42],
            bytes[pos + 43],
            bytes[pos + 44],
            bytes[pos + 45],
        ]) as usize;
        let name_start = pos + 46;
        if name_start + name_len > bytes.len() {
            return Err("central directory name out of bounds".to_string());
        }
        let name = String::from_utf8(bytes[name_start..name_start + name_len].to_vec())
            .map_err(|_| "entry name not UTF-8".to_string())?;
        // local file header 对账（signature + CRC + size + name）。
        let l = local_offset;
        if l + 30 > bytes.len() {
            return Err(format!("local header of {name:?} out of bounds"));
        }
        let l_sig =
            u32::from_le_bytes([bytes[l], bytes[l + 1], bytes[l + 2], bytes[l + 3]]);
        if l_sig != 0x0403_4b50 {
            return Err(format!("bad local header signature for {name:?}"));
        }
        let l_crc = u32::from_le_bytes([
            bytes[l + 14],
            bytes[l + 15],
            bytes[l + 16],
            bytes[l + 17],
        ]);
        if l_crc != crc {
            return Err(format!("local/central CRC mismatch for {name:?}"));
        }
        let l_size = u32::from_le_bytes([
            bytes[l + 22],
            bytes[l + 23],
            bytes[l + 24],
            bytes[l + 25],
        ]) as usize;
        if l_size != uncompressed {
            return Err(format!("local size mismatch for {name:?}"));
        }
        let l_name_start = l + 30;
        let l_name = bytes
            .get(l_name_start..l_name_start + name_len)
            .ok_or_else(|| format!("local name out of bounds for {name:?}"))?;
        if l_name != &bytes[name_start..name_start + name_len] {
            return Err(format!("local/central name mismatch for {name:?}"));
        }
        // 数据面 CRC 重算（判据 6「逐条目 CRC32 全对」+ 负例篡改 fail）。
        let data_start = l_name_start + name_len;
        let data_end = data_start + uncompressed;
        let Some(data) = bytes.get(data_start..data_end) else {
            return Err(format!("entry data out of bounds for {name:?}"));
        };
        let actual = crc32(data);
        if actual != crc {
            return Err(format!(
                "CRC32 mismatch for {name:?}: header {crc:#010x} actual {actual:#010x}"
            ));
        }
        entries.push((name, crc, uncompressed));
        pos = name_start + name_len + extra_len + comment_len;
    }
    Ok(entries)
}
