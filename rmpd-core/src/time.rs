/// Shared time utilities: Unix timestamp conversion and ISO 8601 formatting.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Convert a SystemTime to Unix timestamp (seconds since epoch).
///
/// Pre-epoch times are clamped to `0` and a warning is logged.
pub fn system_time_to_unix_secs(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| {
            tracing::warn!("system time before UNIX_EPOCH, using 0");
            Duration::ZERO
        })
        .as_secs() as i64
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    // Convert civil days since 1970-01-01 to Gregorian Y-M-D.
    // Based on Howard Hinnant's civil_from_days algorithm.
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Convert Unix timestamp to ISO 8601 format (RFC 3339).
pub fn format_iso8601(timestamp: i64) -> String {
    const SECONDS_PER_MINUTE: i64 = 60;
    const SECONDS_PER_HOUR: i64 = 3600;
    const SECONDS_PER_DAY: i64 = 86400;

    let days = timestamp.div_euclid(SECONDS_PER_DAY);
    let remaining = timestamp.rem_euclid(SECONDS_PER_DAY);
    let hours = remaining / SECONDS_PER_HOUR;
    let minutes = (remaining % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let seconds = remaining % SECONDS_PER_MINUTE;

    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::{format_iso8601, system_time_to_unix_secs};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn unix_secs_from_epoch_and_after() {
        assert_eq!(system_time_to_unix_secs(UNIX_EPOCH), 0);
        assert_eq!(system_time_to_unix_secs(UNIX_EPOCH + Duration::from_secs(123)), 123);
    }

    #[test]
    fn unix_secs_pre_epoch_is_clamped_to_zero() {
        assert_eq!(system_time_to_unix_secs(UNIX_EPOCH - Duration::from_secs(1)), 0);
    }

    #[test]
    fn iso8601_epoch_and_boundaries() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_iso8601(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(format_iso8601(86_400), "1970-01-02T00:00:00Z");
    }

    #[test]
    fn iso8601_handles_negative_timestamps() {
        assert_eq!(format_iso8601(-1), "1969-12-31T23:59:59Z");
        assert_eq!(format_iso8601(-86_400), "1969-12-31T00:00:00Z");
    }

    #[test]
    fn iso8601_leap_day_examples() {
        assert_eq!(format_iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_iso8601(1_582_934_400), "2020-02-29T00:00:00Z");
    }
}
