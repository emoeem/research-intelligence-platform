//! UTC ISO8601 时间戳（纯 std 实现）。
//!
//! 说明：为保持 domain 零外部依赖，不自引 chrono；按 Howard Hinnant 的
//! civil_from_days 算法从 Unix epoch 推算日历日期。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 UTC 时间，ISO8601 格式（秒级，`2026-01-01T00:00:00Z`）。
pub fn now_iso() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_epoch_secs(dur.as_secs())
}

/// 当前 UTC 时间，紧凑格式（`YYYYMMDDHHMMSSffffff`，8+6+6=20 位），
/// 用于 query_id 生成（与 eri_p2 的 `erq-<ts>-<hash>` 形状对齐）。
pub fn now_compact_micros() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let micros = dur.subsec_micros();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}{micros:0>6}",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Unix 秒 -> `YYYY-MM-DDTHH:MM:SSZ`。
pub fn format_epoch_secs(secs: u64) -> String {
    let (date, time) = split_epoch(secs);
    format!("{date}T{time}Z")
}

/// Unix 秒 -> (日期 `YYYY-MM-DD`，时间 `HH:MM:SS`)。
fn split_epoch(secs: u64) -> (String, String) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!(
            "{:02}:{:02}:{:02}",
            rem / 3_600,
            (rem % 3_600) / 60,
            rem % 60
        ),
    )
}

/// Howard Hinnant civil_from_days：epoch 天数 -> (y, m, d)。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_timestamps() {
        assert_eq!(format_epoch_secs(0), "1970-01-01T00:00:00Z");
        // 2026-11-06T00:00:00Z = 1793923200（datetime.utc 校准）
        assert_eq!(format_epoch_secs(1_793_923_200), "2026-11-06T00:00:00Z");
        assert_eq!(format_epoch_secs(86_399), "1970-01-01T23:59:59Z");
    }

    #[test]
    fn compact_micros_shape() {
        let s = now_compact_micros();
        assert_eq!(s.len(), 8 + 6 + 6);
        assert!(s.chars().all(|c| c.is_ascii_digit()));
    }
}
