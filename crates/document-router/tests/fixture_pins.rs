//! DI-1 fixture 钉值套件（评审修订 2026-11-19）：最小语料入库
//! crates/document-router/tests/fixtures/（此前测试语料取自
//! rip-data/parse-smoke/in/，该目录被根 .gitignore 整体排除不入库——CI
//! fresh checkout 必红，红线 2 不成立）。逐个 sha256 钉值断言
//! （env_echo.sh 先例同款：fixture 与钉值一并入库，漂移即红）。
//!
//! 语料两族：
//! - parse-smoke 既有样本 12 件（byte 级同源副本，行为断言
//!   pages=2 / 834x250 / 首条目 docProps/app.xml 等全部锚定这些钉值字节）；
//! - oversized-pixels.png（本机 magick 生成 11000x11000 = 121 MP 合成图，
//!   D7 #8 像素帽行为级负例载具：IHDR 宣称 >100 MP 且 IDAT 完整——
//!   `identify -ping` 须读完整 zlib 流才成功，仅 IHDR 头部的截断 PNG
//!   identify exit 1 走 EXECUTION 不到像素帽门，实测定形）。

use std::path::{Path, PathBuf};

use tool_registry::hash::sha256_hex;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// fixture 缺席即显式报错（入库后 CI fresh checkout 在位；panic 信息列
/// 全目录已存在文件便于诊断）。
fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "fixture {name} missing at {}: {e}（钉值语料属仓库资产，不入库即 CI 必红）",
            path.display()
        )
    })
}

/// 18 件 fixture sha256 逐个钉值（DI-1 13 件 2026-11-19 入库实测值 +
/// DI-2 5 件 2026-11-20 入库实测值；漂移即红，与 three_channel_e2e/
/// sniff_boundary/sniff lib/native_rust_dispatch 的行为断言共用同一字节
/// 基线）。DI-2 五件的三层裁决见 DI-2-TASK §4：字节确定性归入库钉值，
/// 生成器字节非确定面（7z/mp4）归运行时生成，负例放大面归测试内构造。
#[test]
fn fixture_corpus_sha256_pinned() {
    let pins: [(&str, &str); 18] = [
        (
            "bomb.zip",
            "258e1c3c064ff3e4b719371a2086d0210dd531492449b50e90703ac34eeedaf7",
        ),
        (
            "clean.md",
            "249e984b16ca28b1c67523109bc1c30e579cb71aa591fa6af72cd73a910680be",
        ),
        (
            "evil.zip",
            "c3e582a5de7e384c7253630bb9582c7040263c40f6702f0904135d581d9119cb",
        ),
        (
            "evil2.zip",
            "ea1c7fbe385beb47998a00c5aee46905a95f35f687ef31448aa52fbc5ea49305",
        ),
        (
            "oversized-pixels.png",
            "3f195b907db6de557eb3c77ed86312587b8b60fa315b4b80cd4cf291587bb42b",
        ),
        (
            "sample.docx",
            "0dbba5bfa5973ac648b3ead7df4b56a5e1ce729c005b3abd4e062b81c0dd3ccb",
        ),
        (
            "sample.epub",
            "f0eaf84fc62819a991b99e170897789265a43a9e18cfb38cd39301ac1d7c88a4",
        ),
        (
            "sample.html",
            "34cdb475373b4b4870a9827caf8dc5067ec80d3a1b42158cbe5b659585b46470",
        ),
        (
            "sample.md",
            "cb53d9c3adfa65eb6f5c2ecd98fdb3bed204dddb736a09fc80a2e0c9aa33538f",
        ),
        (
            "sample.pdf",
            "a3da305b74c5ceed157ddcd14dadd99daa061e45f750cffb9824f29288c300f8",
        ),
        (
            "sample.png",
            "5281921eb7ed90c685452097f3b02b1738470d774b69d6799aa99792488f35e5",
        ),
        (
            "sample.pptx",
            "77c9039b099c96848de90f04bfec2e56d805ddc848bb20fb9e175735d56fb3da",
        ),
        (
            "sample.txt",
            "66c40143dcc7c5abfcb1707a4a535c6dda52c74526a183bf9f6cb24bcb69da3e",
        ),
        (
            "sample.xlsx",
            "3014f815aafe1b470deda326dc936d540a16821e894c4fa66742dd7b316e700d",
        ),
        (
            "sample.zip",
            "c1eba6e333f39d7474793cf94ba1a12fc17953dfa9f3342f3cd42ba30bad81bb",
        ),
        (
            "symlink.zip",
            "063d1827a528b3ee5483f9df554e40db81e14ece0ab76373154f065207c709e7",
        ),
        (
            "text-ocr.png",
            "2c558f8d7fd25a4f7924def4c6fd5bd96e95d1e4478d2dbbe44f1f62cd05beb6",
        ),
        (
            "tone.wav",
            "fc736cfe011fb8fe0f85326a52321e803effd2d0577605e7373ffd47ec20c7ba",
        ),
    ];
    for (name, pin) in pins {
        let sha = sha256_hex(&read_fixture(name));
        assert_eq!(sha, pin, "fixture {name} sha256 钉值漂移");
    }
}

/// 像素帽负例载具的形态前提：oversized-pixels.png 为完整合法 PNG（identify
/// -ping 可读）且 IHDR 尺寸 11000x11000 = 121 MP > D7 #8 帽 100 MP。
#[test]
fn oversized_pixels_fixture_shape() {
    let bytes = read_fixture("oversized-pixels.png");
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG 签名（嗅探路由图像通道前提）"
    );
    // IHDR：长度 13 + "IHDR" + width(u32 BE) + height(u32 BE)。
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!((width, height), (11000, 11000));
    let pixels = width as u64 * height as u64;
    assert_eq!(pixels, 121_000_000);
    assert!(
        pixels > document_router::channel::MAX_PIXELS,
        "IHDR 宣称像素 121 MP 必须超过 D7 #8 帽（100 MP）"
    );
    // 完整性：以 IEND chunk 结尾（长度 0 + "IEND" + 标准 CRC ae426082）。
    // 截断 PNG 在 identify -ping 下 exit 1，走不到像素帽——完整流是前提。
    assert!(
        bytes.ends_with(&[0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]),
        "oversized-pixels.png 必须是完整 PNG（IEND chunk 在位）"
    );
}

// ---------------------------------------------------------------------------
// DI-2 五件钉值语料的形态前提（DI-2-TASK §4 第一层：断言对象 = 字节本身
// 或一次性生成后字节稳定面）
// ---------------------------------------------------------------------------

/// text-ocr.png（OCR 正例锚）：PNG 魔数 + 460x80（IHDR）——tesseract 词行/
/// 置信度断言锚定同一字节基线；字体渲染版本相关 → 不在测试内实时生成。
#[test]
fn text_ocr_fixture_shape() {
    let bytes = read_fixture("text-ocr.png");
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG 签名（嗅探路由图像通道前提）"
    );
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!((width, height), (460, 80), "IHDR 尺寸 460x80");
    let pixels = width as u64 * height as u64;
    assert!(
        pixels < document_router::channel::OCR_PIXELS_MAX,
        "36800 像素必须远低于 OCR 帽 50 MP（正例前提）"
    );
}

/// tone.wav（RIFF/WAVE 判定与 ffprobe JSON 形态锚）：头 4 字节 RIFF 且
/// 8-11 字节 WAVE；8044 B = 12 + 24 + 8 + 8000（0.5s × 8kHz × 2 字节 PCM）。
#[test]
fn tone_wav_fixture_shape() {
    let bytes = read_fixture("tone.wav");
    assert_eq!(&bytes[..4], b"RIFF", "RIFF 头");
    assert_eq!(&bytes[8..12], b"WAVE", "WAVE 子标识（嗅探判据）");
    let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize + 8;
    assert_eq!(riff_size, 8044, "RIFF 块总长 8044 B（任务书 §4 定值）");
    assert_eq!(
        &bytes[36..40],
        b"data",
        "data chunk 前为 fmt chunk（16 字节 PCM fmt）"
    );
    let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    assert_eq!(data_size, 8000, "data 8000 字节 = 4000 样本 × 2 字节");
}

/// evil2.zip（闸 1 穿越负例锚）：条目名序列 ok.txt / ../escape.txt /
/// /tmp/di2-fix/abs-escape.txt 是断言对象本身，必须逐字节稳定；7z 无法
/// 创建穿越名条目（工具自身拒绝），只能程序化构造。
#[test]
fn evil2_zip_fixture_shape() {
    let bytes = read_fixture("evil2.zip");
    assert_eq!(&bytes[..4], &ZIP_LFH_SIG, "ZIP 签名");
    // 嗅探面：无 [Content_Types].xml → ZipOther → 归档通道（DI-2 D1）。
    assert_eq!(
        document_router::sniff::sniff(&bytes),
        document_router::sniff::Magic::ZipOther
    );
    // 穿越名条目在 LFH 扫描面逐字在位。
    let corpus = bytes.to_vec();
    let needle = b"../escape.txt";
    assert!(
        corpus.windows(needle.len()).any(|w| w == needle),
        "穿越名 ../escape.txt 必须逐字在位（闸 1 判定输入）"
    );
    let abs = b"/tmp/di2-fix/abs-escape.txt";
    assert!(
        corpus.windows(abs.len()).any(|w| w == abs),
        "绝对路径条目名逐字在位（闸 1 第二判据）"
    );
}

/// bomb.zip（闸 4 解压总量负例锚）：小 zip 大 Size 是唯一可入库形态
/// （解压产物不入库）——Size = 67108864 声明在 central directory 判读。
#[test]
fn bomb_zip_fixture_shape() {
    let bytes = read_fixture("bomb.zip");
    assert_eq!(&bytes[..4], &ZIP_LFH_SIG, "ZIP 签名");
    assert!(
        bytes.len() < 100_000,
        "bomb.zip 压缩态必须小（{} B），解压 64 MiB 靠 Size 声明判定",
        bytes.len()
    );
    let corpus = bytes.to_vec();
    let needle = b"zeros.bin";
    assert!(
        corpus.windows(needle.len()).any(|w| w == needle),
        "zeros.bin 条目名在位"
    );
}

/// symlink.zip（闸 2 zip 符号链接面锚）：real.txt + link-to-etc.txt 符号
/// 链接条目（external_attr 0o120777 + create_system=3）；7z -slt 对该形态
/// 呈现 `Attributes =  lrwxrwxrwx`（2026-11-20 实测）。
#[test]
fn symlink_zip_fixture_shape() {
    let bytes = read_fixture("symlink.zip");
    assert_eq!(&bytes[..4], &ZIP_LFH_SIG, "ZIP 签名");
    let corpus = bytes.to_vec();
    let needle = b"link-to-etc.txt";
    assert!(
        corpus.windows(needle.len()).any(|w| w == needle),
        "符号链接条目名在位"
    );
    // unix 模式位 0o120777 在 external_attr 高 16 位（create_system=3）。
    // 轻量校验：central directory 内该条目 external_attr 含 0xA1FF（0o120777
    // << 16 的高半），程序化构造钉值前提下以字节序列存在性断言。
    let mode_le = (0o120777u32 << 16).to_le_bytes();
    assert!(
        corpus.windows(4).any(|w| w == mode_le),
        "符号链接 unix 模式位（0o120777 << 16）在位"
    );
}

const ZIP_LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
