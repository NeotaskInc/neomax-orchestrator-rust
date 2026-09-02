use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local, Utc};

pub(crate) fn local_day_path(directory: &Path, at: DateTime<Utc>) -> PathBuf {
    let day = at.with_timezone(&Local).date_naive();
    directory.join(format!(
        "{:04}-{:02}-{:02}.jsonl",
        day.year(),
        day.month(),
        day.day()
    ))
}

pub(crate) fn local_day_path_from_timestamp(directory: &Path, timestamp: i64) -> PathBuf {
    DateTime::<Utc>::from_timestamp(timestamp, 0).map_or_else(
        || local_day_path(directory, Utc::now()),
        |at| local_day_path(directory, at),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn partitions_by_the_machine_local_day() {
        let at = DateTime::<Utc>::from_timestamp(1_787_488_123, 0).unwrap();
        let expected_day = at.with_timezone(&Local).date_naive();
        assert_eq!(
            local_day_path(Path::new("/events"), at),
            PathBuf::from(format!(
                "/events/{:04}-{:02}-{:02}.jsonl",
                expected_day.year(),
                expected_day.month(),
                expected_day.day()
            ))
        );
    }
}
