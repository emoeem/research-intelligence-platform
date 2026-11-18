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

/// 13 件 fixture sha256 逐个钉值（2026-11-19 入库实测值；漂移即红，
/// 与 three_channel_e2e/sniff_boundary/sniff lib/native_rust_dispatch 的
/// 行为断言共用同一字节基线）。
#[test]
fn fixture_corpus_sha256_pinned() {
    let pins: [(&str, &str); 13] = [
        (
            "clean.md",
            "249e984b16ca28b1c67523109bc1c30e579cb71aa591fa6af72cd73a910680be",
        ),
        (
            "evil.zip",
            "c3e582a5de7e384c7253630bb9582c7040263c40f6702f0904135d581d9119cb",
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
