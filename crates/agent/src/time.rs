/// Returns the current UTC timestamp in RFC 3339 format.
pub fn utc_rfc3339_now() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch");
    format_rfc3339(duration.as_secs(), duration.subsec_nanos())
}

fn format_rfc3339(secs: u64, nanos: u32) -> String {
    let (year, month, day, hour, minute, second) = unix_secs_to_utc(secs);
    if nanos == 0 {
        return format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z");
    }

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z",
        nanos = nanos
    )
}

fn unix_secs_to_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hour = (rem / 3_600) as u32;
    let minute = ((rem % 3_600) / 60) as u32;
    let second = (rem % 60) as u32;

    let mut year = 1970u32;
    let mut day_of_year = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if day_of_year < days_in_year {
            break;
        }
        day_of_year -= days_in_year;
        year += 1;
    }

    let mut month = 1u32;
    for days_in_month in month_lengths(year) {
        if day_of_year < days_in_month {
            break;
        }
        day_of_year -= days_in_month;
        month += 1;
    }

    (year, month, day_of_year as u32 + 1, hour, minute, second)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn month_lengths(year: u32) -> [u64; 12] {
    [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_rfc3339_now_ends_with_z() {
        let stamp = utc_rfc3339_now();
        assert!(stamp.ends_with('Z'));
        assert!(stamp.contains('T'));
    }
}
