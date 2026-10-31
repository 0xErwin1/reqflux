//! The day group and the time of day a History row shows, in the user's local time.
//!
//! The arithmetic is pure: it takes the moment, now and the UTC offset, so tests never depend
//! on the machine's time zone. [`local_offset`] reads the offset through `chrono`, which reads
//! the system time zone safely in a multithreaded process.

use chrono::{DateTime, Datelike, FixedOffset, Local, Offset, TimeDelta, Utc};

/// The UTC offset of the local time zone at `millis` since the Unix epoch; UTC when the
/// moment is out of range.
pub(crate) fn local_offset(millis: i64) -> FixedOffset {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|moment| *moment.with_timezone(&Local).offset())
        .unwrap_or_else(|| Utc.fix())
}

/// The `HH:MM:SS` time of `sent_at`, in milliseconds since the Unix epoch, at `offset`.
pub(crate) fn seconds(sent_at: i64, offset: FixedOffset) -> String {
    DateTime::<Utc>::from_timestamp_millis(sent_at)
        .map(|sent| sent.with_timezone(&offset).format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// The day group and the `HH:MM` time of `sent_at` seen at `now`, both in milliseconds since
/// the Unix epoch, at `offset`: `Today`, `Yesterday`, `Thu, Oct 29` within the year of `now`,
/// else `Thu, Oct 29, 2025`.
pub(crate) fn day_and_time(sent_at: i64, now: i64, offset: FixedOffset) -> (String, String) {
    let Some(sent) = DateTime::<Utc>::from_timestamp_millis(sent_at) else {
        return (String::new(), String::new());
    };
    let sent = sent.with_timezone(&offset);
    let time = sent.format("%H:%M").to_string();
    let today = DateTime::<Utc>::from_timestamp_millis(now)
        .map(|now| now.with_timezone(&offset).date_naive());
    let day = sent.date_naive();
    let label = match today {
        Some(today) if day == today => "Today".to_owned(),
        Some(today) if today.checked_sub_signed(TimeDelta::days(1)) == Some(day) => {
            "Yesterday".to_owned()
        }
        Some(today) if today.year() == day.year() => sent.format("%a, %b %-d").to_string(),
        _ => sent.format("%a, %b %-d, %Y").to_string(),
    };
    (label, time)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::{day_and_time, seconds};

    const HOUR: i64 = 3_600_000;
    /// 2026-10-31T14:32:08Z, a Saturday.
    const NOW: i64 = 1_793_457_128_000;

    fn east(hours: i32) -> FixedOffset {
        match FixedOffset::east_opt(hours * 3600) {
            Some(offset) => offset,
            None => panic!("offset {hours}"),
        }
    }

    #[test]
    fn today_and_yesterday_follow_the_local_date() {
        assert_eq!(
            day_and_time(NOW, NOW, east(0)),
            ("Today".to_owned(), "14:32".to_owned())
        );
        assert_eq!(
            day_and_time(NOW - 20 * HOUR, NOW, east(0)),
            ("Yesterday".to_owned(), "18:32".to_owned())
        );
    }

    #[test]
    fn the_offset_moves_the_day_boundary() {
        // Now is 01:32 on Nov 1 at UTC+11, so two hours earlier is yesterday there, while at
        // UTC both are today.
        assert_eq!(
            day_and_time(NOW, NOW, east(11)),
            ("Today".to_owned(), "01:32".to_owned())
        );
        assert_eq!(
            day_and_time(NOW - 2 * HOUR, NOW, east(11)),
            ("Yesterday".to_owned(), "23:32".to_owned())
        );
        assert_eq!(day_and_time(NOW - 2 * HOUR, NOW, east(0)).0, "Today");
        // At UTC-3 the same moment is still the 31st, 11:32.
        assert_eq!(
            day_and_time(NOW, NOW, east(-3)),
            ("Today".to_owned(), "11:32".to_owned())
        );
    }

    #[test]
    fn seconds_show_the_local_time_of_day() {
        assert_eq!(seconds(NOW, east(0)), "14:32:08");
        assert_eq!(seconds(NOW, east(-3)), "11:32:08");
    }

    #[test]
    fn older_days_show_the_weekday_and_date() {
        assert_eq!(
            day_and_time(NOW - 2 * 24 * HOUR, NOW, east(0)).0,
            "Thu, Oct 29"
        );
        assert_eq!(
            day_and_time(NOW - 365 * 24 * HOUR, NOW, east(0)).0,
            "Fri, Oct 31, 2025"
        );
    }
}
