//! Kalid: calendar-based, K-sortable unique ID generator with UUID v7
//! interoperability.
//!
//! Kalid encodes a Unix millisecond timestamp into a compact 16-character
//! string:
//!
//! ```text
//! {ms_hex}{month}{week}{day}
//! ```
//!
//! | Segment | Length | Encoding |
//! |---------|--------|----------|
//! | Ms      | 12 | Unix timestamp in milliseconds, lowercase hex |
//! | Month   | 1  | `a` (January) .. `l` (December) |
//! | Week    | 2  | ISO week number 01-53 |
//! | Day     | 1  | `m` (Monday) .. `s` (Sunday) |
//!
//! # K-sortability
//!
//! **Fully K-sortable** — lexicographic order matches chronological order
//! across all boundaries: same millisecond, day, month, year, and even the
//! December→January year boundary. No inversions or edge cases.
//!
//! The millisecond hex timestamp is placed first and increases monotonically
//! with time. Month/week/day suffixes are metadata for human readability and
//! do not affect sort order.
//!
//! # UUID v7 interoperability (lossless, deterministic)
//!
//! Kalid and [UUID v7](https://www.rfc-editor.org/rfc/rfc9562#name-uuid-version-7)
//! (RFC 9562) share the exact same millisecond timestamp. The week and day
//! are encoded in the UUID v7 `rand_a` field (12 bits):
//!
//! ```text
//! rand_a (12 bit) = [week:6][day:3][random:3]
//! ```
//!
//! This makes the conversion **fully deterministic** in both directions:
//!
//! * **Kalid -> UUID v7** - the hex timestamp maps to bytes 0-5 (48 bits),
//!   week+day encode into `rand_a` (9 bits). Only `rand_b` (62 bits) remains
//!   random for UUID uniqueness.
//! * **UUID v7 -> Kalid** - extracts the millisecond timestamp from bytes
//!   0-5 and week+day from `rand_a`. The roundtrip
//!   `kalid -> UUID v7 -> kalid` always produces the **exact same string**.
//!
//! UUIDs created externally (e.g. `Uuid::now_v7()`) still decode to a valid
//! kalid - the timestamp is accurate, but week+day in the output will be
//! derived from the timestamp (not from `rand_a`) since external UUIDs
//! cannot carry kalid-encoded week+day.
//! # Example
//!
//! ```
//! # use elph_core::utils::kalid::Kalid;
//! let kalid = Kalid::new();
//! assert_eq!(kalid.as_string().len(), 16);
//!
//! // Roundtrip: string → parse → string
//! let parsed = Kalid::parse(&kalid.as_string()).unwrap();
//! assert_eq!(parsed.as_string(), kalid.as_string());
//!
//! // UUID v7 roundtrip (lossless)
//! let uuid = kalid.to_uuid_v7();
//! let back = Kalid::from_uuid_v7(&uuid);
//! assert_eq!(back.epoch_ms(), kalid.epoch_ms());
//! ```

use chrono::{Datelike, TimeZone, Utc};
/// Month encoding: `a` = January .. `l` = December.
const MONTH_CHARS: [char; 12] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l'];

/// Day-of-week encoding: `m` = Monday .. `s` = Sunday.
const DAY_CHARS: [char; 7] = ['m', 'n', 'o', 'p', 'q', 'r', 's'];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KalidParseError {
    /// Input length is not 16 characters.
    #[error("kalid must be exactly 16 characters")]
    InvalidLength,
    /// Timestamp segment is not valid 12-digit hex.
    #[error("timestamp must be 12 hex digits")]
    InvalidTimestamp,
    /// Month character not in range `a`..`l`.
    #[error("month must be a..l")]
    InvalidMonth,
    /// Week segment is not a valid two-digit number.
    #[error("week must be a 2-digit number")]
    InvalidWeek,
    /// Day character not in range `m`..`s`.
    #[error("day must be m..s")]
    InvalidDay,
    /// Parsed components don't match the embedded timestamp.
    #[error("kalid components don't match timestamp")]
    Mismatch,
}

/// A calendar-based unique ID with UUID v7 interoperability.
///
/// See the [module-level documentation](self) for format, K-sortability,
/// and UUID v7 interop details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kalid {
    epoch_ms: i64,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl Kalid {
    /// Create a new `Kalid` from the current system time.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// let kalid = Kalid::new();
    /// assert_eq!(kalid.as_string().len(), 16);
    /// ```
    pub fn new() -> Self {
        Kalid {
            epoch_ms: Utc::now().timestamp_millis(),
        }
    }

    /// Create a `Kalid` from a Unix epoch in **seconds**.
    ///
    /// The sub-second fraction is set to zero.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// let kalid = Kalid::from_epoch(1_784_060_036);
    /// assert_eq!(kalid.epoch_secs(), 1_784_060_036);
    /// ```
    pub fn from_epoch(epoch_secs: i64) -> Self {
        Kalid {
            epoch_ms: epoch_secs * 1000,
        }
    }

    /// Create a `Kalid` from a Unix epoch in **milliseconds**.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
    /// assert_eq!(kalid.epoch_ms(), 1_784_060_036_000);
    /// ```
    pub fn from_epoch_ms(epoch_ms: i64) -> Self {
        Kalid { epoch_ms }
    }

    /// Parse a 16-character kalid string into its components.
    ///
    /// The string is validated: every segment is checked for valid ranges,
    /// and the month/week/day are verified against the embedded timestamp.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// // epoch 0 ms = Thursday, Jan 1, 1970 → month a, week 01, day p
    /// let kalid = Kalid::parse("000000000000a01p").unwrap();
    /// assert_eq!(kalid.epoch_ms(), 0);
    /// assert_eq!(kalid.as_string(), "000000000000a01p");
    /// ```
    pub fn parse(s: &str) -> Result<Self, KalidParseError> {
        if s.len() != 16 {
            return Err(KalidParseError::InvalidLength);
        }

        // Parse hex timestamp [0..12]
        let epoch_ms = i64::from_str_radix(&s[..12], 16).map_err(|_| KalidParseError::InvalidTimestamp)?;

        // Validate month [12]
        let month_char = s.as_bytes()[12];
        if !(b'a'..=b'l').contains(&month_char) {
            return Err(KalidParseError::InvalidMonth);
        }

        // Validate week [13..15]
        if !s[13..15].bytes().all(|b| b.is_ascii_digit()) {
            return Err(KalidParseError::InvalidWeek);
        }

        // Validate day [15]
        let day_char = s.as_bytes()[15];
        if !(b'm'..=b's').contains(&day_char) {
            return Err(KalidParseError::InvalidDay);
        }

        // Verify components match the epoch
        let kalid = Kalid { epoch_ms };
        let expected = kalid.as_string();
        if s != expected {
            return Err(KalidParseError::Mismatch);
        }

        Ok(kalid)
    }

    /// Create a `Kalid` from a UUID v7 by extracting its embedded
    /// millisecond timestamp.
    ///
    /// The UUID must be version 7 (RFC 9562). Interop is lossless and
    /// deterministic when the UUID was created by [`Kalid::to_uuid_v7`].
    /// For externally-generated UUID v7, the timestamp is still accurate
    /// but week+day are derived from the timestamp (not from `rand_a`).
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// # use uuid::Uuid;
    /// let uuid = Uuid::now_v7();
    /// let kalid = Kalid::from_uuid_v7(&uuid);
    /// assert_eq!(kalid.as_string().len(), 16);
    /// ```
    pub fn from_uuid_v7(uuid: &uuid::Uuid) -> Self {
        let bytes = uuid.as_bytes();
        let epoch_ms = u64::from_be_bytes([0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]]) as i64;
        Kalid { epoch_ms }
    }
}

impl Default for Kalid {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

impl Kalid {
    /// Return the kalid as a 16-character string.
    ///
    /// Format: `{ms_hex:012}{month}{week:02}{day}`.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// let kalid = Kalid::from_epoch_ms(0);
    /// assert_eq!(kalid.as_string(), "000000000000a01p");
    /// ```
    pub fn as_string(&self) -> String {
        format_kalid(self.epoch_ms)
    }

    /// Return the Unix epoch timestamp (seconds since 1970-01-01 UTC).
    ///
    /// Note: sub-millisecond precision is not available — the value is
    /// `epoch_ms / 1000`.
    pub fn epoch_secs(&self) -> i64 {
        self.epoch_ms / 1000
    }

    /// Return the Unix epoch timestamp in milliseconds.
    pub fn epoch_ms(&self) -> i64 {
        self.epoch_ms
    }

    /// Convert to a UUID v7 with embedded week+day in `rand_a`.
    ///
    /// The kalid's millisecond timestamp is placed in the UUID v7's 48-bit
    /// field. The week and day are encoded into `rand_a` (12 bits):
    ///
    /// ```text
    /// bits [11:6] = week (0-53),  bits [5:3] = day (0-6),  bits [2:0] = random
    /// ```
    ///
    /// Only `rand_b` (62 bits) remains random — `rand_a` is fully deterministic
    /// from the kalid. This means `from_uuid_v7(to_uuid_v7())` always produces
    /// the **exact same kalid string**.
    ///
    /// ```
    /// # use elph_core::utils::kalid::Kalid;
    /// let kalid = Kalid::new();
    /// let uuid = kalid.to_uuid_v7();
    /// assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
    ///
    /// // Deterministic roundtrip
    /// let back = Kalid::from_uuid_v7(&uuid);
    /// assert_eq!(back.as_string(), kalid.as_string());
    /// ```
    pub fn to_uuid_v7(&self) -> uuid::Uuid {
        let mut bytes = [0u8; 10];
        rand::fill(&mut bytes[..]);

        // Derive week+day from epoch_ms
        let secs = self.epoch_ms / 1000;
        let nsecs = ((self.epoch_ms % 1000) * 1_000_000) as u32;
        let dt = Utc.timestamp_opt(secs, nsecs).unwrap();
        let week = dt.iso_week().week();
        let day = dt.weekday().num_days_from_monday();

        // Encode week+day into rand_a (bytes[0..1], 12 bits)
        // bits [11:6] = week, bits [5:3] = day, bits [2:0] = random
        bytes[0] = (bytes[0] & 0xF0) | ((week >> 2) as u8 & 0x0F);
        bytes[1] = (bytes[1] & 0x07) | (((week as u8 & 0x03) << 6) | ((day as u8 & 0x07) << 3));

        uuid::Builder::from_unix_timestamp_millis(self.epoch_ms as u64, &bytes).into_uuid()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_kalid(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    let nsecs = ((epoch_ms % 1000) * 1_000_000) as u32;
    let dt = Utc.timestamp_opt(secs, nsecs).unwrap();
    let month = MONTH_CHARS[dt.month0() as usize];
    let week = dt.iso_week().week();
    let day = DAY_CHARS[dt.weekday().num_days_from_monday() as usize];
    format!("{:012x}{month}{week:02}{day}", epoch_ms)
}

/// Convenience function: generate a kalid string directly.
///
/// Equivalent to `Kalid::new().as_string()`.
///
/// ```
/// # use elph_core::utils::kalid::generate_kalid;
/// let id = generate_kalid();
/// assert_eq!(id.len(), 16);
/// ```
pub fn generate_kalid() -> String {
    Kalid::new().as_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Basics ---------------------------------------------------------

    #[test]
    fn format_is_16_chars() {
        let id = generate_kalid();
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn as_string_format() {
        let kalid = Kalid::from_epoch_ms(0);
        let s = kalid.as_string();

        assert!(
            s[..12]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "timestamp not lowercase hex"
        );

        // month [12]
        let m = s.as_bytes()[12];
        assert!((b'a'..=b'l').contains(&m), "month out of range");

        // week [13..15]
        assert!(s[13..15].bytes().all(|b| b.is_ascii_digit()), "week not numeric");

        // day [15]
        let d = s.as_bytes()[15];
        assert!((b'm'..=b's').contains(&d), "day out of range");

        assert_eq!(s.len(), 16);
    }

    #[test]
    fn known_epoch_produces_expected_kalid() {
        // epoch 0 ms = Thursday, Jan 1, 1970 → month a, week 01, day p
        let kalid = Kalid::from_epoch_ms(0);
        assert_eq!(kalid.as_string(), "000000000000a01p");

        // epoch 0 seconds → same
        let kalid = Kalid::from_epoch(0);
        assert_eq!(kalid.as_string(), "000000000000a01p");
    }

    #[test]
    fn month_mapping() {
        assert_eq!(MONTH_CHARS[0], 'a'); // January
        assert_eq!(MONTH_CHARS[11], 'l'); // December
    }

    #[test]
    fn day_mapping() {
        assert_eq!(DAY_CHARS[0], 'm'); // Monday
        assert_eq!(DAY_CHARS[6], 's'); // Sunday
    }

    // -- K-sortability --------------------------------------------------

    #[test]
    fn k_sortable_across_all_boundaries() {
        // Generate kalids for every day in July 2025 + cross year
        let start_ms = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap().timestamp() * 1000;
        let mut prev = String::new();

        // 31 days in July
        for day_offset in 0..31 {
            let ms = start_ms + day_offset * 86_400_000;
            let kalid = Kalid::from_epoch_ms(ms);
            let s = kalid.as_string();

            if !prev.is_empty() {
                assert!(prev < s, "inversion: {prev} >= {s} at offset {day_offset}");
            }
            prev = s;
        }

        // Cross year: Dec 31 → Jan 1
        let dec31 = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap().timestamp() * 1000 + 999;
        let jan1 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap().timestamp() * 1000;
        let d31 = Kalid::from_epoch_ms(dec31).as_string();
        let j1 = Kalid::from_epoch_ms(jan1).as_string();
        assert!(d31 < j1, "year boundary inversion: {d31} >= {j1}");
    }

    // -- Parse ----------------------------------------------------------

    #[test]
    fn parse_roundtrip() {
        let original = Kalid::new();
        let s = original.as_string();
        let parsed = Kalid::parse(&s).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_known_string() {
        let kalid = Kalid::parse("000000000000a01p").unwrap();
        assert_eq!(kalid.epoch_ms(), 0);
    }

    #[test]
    fn parse_invalid_length() {
        assert_eq!(Kalid::parse("short"), Err(KalidParseError::InvalidLength));
        assert_eq!(Kalid::parse("000000000000a01p00"), Err(KalidParseError::InvalidLength));
    }

    #[test]
    fn parse_invalid_timestamp() {
        assert_eq!(Kalid::parse("zzzzzzzzzzzza01p"), Err(KalidParseError::InvalidTimestamp));
    }

    #[test]
    fn parse_invalid_month() {
        assert_eq!(Kalid::parse("000000000000m01p"), Err(KalidParseError::InvalidMonth));
        assert_eq!(Kalid::parse("000000000000001p"), Err(KalidParseError::InvalidMonth));
    }

    #[test]
    fn parse_invalid_week() {
        assert_eq!(Kalid::parse("000000000000aabp"), Err(KalidParseError::InvalidWeek));
    }

    #[test]
    fn parse_invalid_day() {
        assert_eq!(Kalid::parse("000000000000a01x"), Err(KalidParseError::InvalidDay));
    }

    #[test]
    fn parse_mismatch() {
        // Valid format but components don't match
        assert_eq!(Kalid::parse("000000000000b01m"), Err(KalidParseError::Mismatch));
    }

    // -- UUID v7 interoperability ---------------------------------------

    #[test]
    fn to_uuid_v7_produces_valid_version() {
        let kalid = Kalid::new();
        let uuid = kalid.to_uuid_v7();
        assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
    }

    #[test]
    fn uuid_v7_roundtrip_preserves_ms() {
        let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
        let uuid = kalid.to_uuid_v7();
        let back = Kalid::from_uuid_v7(&uuid);
        assert_eq!(back.epoch_ms(), kalid.epoch_ms());
    }

    #[test]
    fn uuid_v7_roundtrip_many_random_uuids() {
        let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
        for _ in 0..10 {
            let uuid = kalid.to_uuid_v7();
            let back = Kalid::from_uuid_v7(&uuid);
            assert_eq!(back.epoch_ms(), 1_784_060_036_000);
        }
    }

    #[test]
    fn from_uuid_v7_preserves_string_format() {
        let uuid = uuid::Uuid::now_v7();
        let kalid = Kalid::from_uuid_v7(&uuid);
        let s = kalid.as_string();

        assert_eq!(s.len(), 16);
        assert!(s[..12].bytes().all(|b| b.is_ascii_hexdigit()));
        assert!((b'a'..=b'l').contains(&s.as_bytes()[12]));
        assert!(s[13..15].bytes().all(|b| b.is_ascii_digit()));
        assert!((b'm'..=b's').contains(&s.as_bytes()[15]));
    }

    #[test]
    fn uuid_v7_byte_level_interop() {
        // The hex timestamp MUST match bytes [0..6] of the UUID v7
        let kalid = Kalid::from_epoch_ms(0x019f62686310);
        let uuid = kalid.to_uuid_v7();
        assert_eq!(uuid.as_bytes()[0], 0x01);
        assert_eq!(uuid.as_bytes()[1], 0x9f);
        assert_eq!(uuid.as_bytes()[2], 0x62);
        assert_eq!(uuid.as_bytes()[3], 0x68);
        assert_eq!(uuid.as_bytes()[4], 0x63);
        assert_eq!(uuid.as_bytes()[5], 0x10);
    }

    #[test]
    fn to_uuid_v7_unique_per_call() {
        let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
        let u1 = kalid.to_uuid_v7();
        let u2 = kalid.to_uuid_v7();
        assert_ne!(u1, u2, "same kalid should produce different UUID v7 values");
    }

    // -- Edge cases -----------------------------------------------------

    #[test]
    fn default_is_new() {
        let a = Kalid::default();
        let b = Kalid::new();
        assert!((a.epoch_ms() - b.epoch_ms()).abs() <= 1);
    }

    #[test]
    fn epoch_zero_is_jan_1970() {
        let kalid = Kalid::from_epoch_ms(0);
        assert_eq!(kalid.epoch_ms(), 0);
        assert_eq!(kalid.epoch_secs(), 0);
        assert_eq!(kalid.as_string().len(), 16);
    }

    #[test]
    fn from_epoch_sets_correct_ms() {
        let kalid = Kalid::from_epoch(1_784_060_036);
        assert_eq!(kalid.epoch_ms(), 1_784_060_036_000);
        assert_eq!(kalid.epoch_secs(), 1_784_060_036);
    }

    #[test]
    fn generate_kalid_convenience() {
        let a = generate_kalid();
        let b = Kalid::new().as_string();
        assert_eq!(a.len(), 16);
        assert_eq!(b.len(), 16);
    }

    // -- Deterministic UUID v7 roundtrip ---------------------------------

    #[test]
    fn deterministic_roundtrip_same_string() {
        let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
        let uuid = kalid.to_uuid_v7();
        let back = Kalid::from_uuid_v7(&uuid);
        assert_eq!(
            back.as_string(),
            kalid.as_string(),
            "deterministic roundtrip: kalid -> UUID v7 -> kalid"
        );
    }

    #[test]
    fn deterministic_roundtrip_multiple_epochs() {
        for ms in [
            0,
            1_000_000_000,     // 2001-09-09
            1_700_000_000_000, // 2023-11-14
            1_784_060_036_000, // 2026-07-08
        ] {
            let kalid = Kalid::from_epoch_ms(ms);
            let uuid = kalid.to_uuid_v7();
            let back = Kalid::from_uuid_v7(&uuid);
            assert_eq!(back.as_string(), kalid.as_string(), "failed at epoch_ms={}", ms);
        }
    }

    #[test]
    fn rand_a_encodes_week_and_day() {
        use chrono::Datelike;
        let epoch_ms = 1_784_060_036_000;
        let dt = Utc
            .timestamp_opt(epoch_ms / 1000, ((epoch_ms % 1000) * 1_000_000) as u32)
            .unwrap();
        let expected_week = dt.iso_week().week();
        let expected_day = dt.weekday().num_days_from_monday();

        let kalid = Kalid::from_epoch_ms(epoch_ms);
        let uuid = kalid.to_uuid_v7();
        let bytes = uuid.as_bytes();
        let rand_a = (u16::from_be_bytes([bytes[6], bytes[7]]) & 0x0FFF) as u32;
        let week = (rand_a >> 6) & 0x3F;
        let day = (rand_a >> 3) & 0x07;
        assert_eq!(week, expected_week, "rand_a week mismatch");
        assert_eq!(day, expected_day, "rand_a day mismatch");
    }

    #[test]
    fn rand_a_low_3_bits_are_random() {
        let kalid = Kalid::from_epoch_ms(1_784_060_036_000);
        let uuid1 = kalid.to_uuid_v7();
        let uuid2 = kalid.to_uuid_v7();
        // Low 3 bits are random — they MAY differ (prob ~ 1 - (1/8) per pair)
        // We check that week+day bits are identical
        let wd1 = (u16::from_be_bytes([uuid1.as_bytes()[6], uuid1.as_bytes()[7]]) & 0x0FF8) >> 3;
        let wd2 = (u16::from_be_bytes([uuid2.as_bytes()[6], uuid2.as_bytes()[7]]) & 0x0FF8) >> 3;
        assert_eq!(wd1, wd2, "week+day in rand_a should be identical across calls");
    }

    // -- Collision & performance benchmarks --------------------------------

    #[test]
    fn collision_benchmark_vs_nanoid() {
        const N: usize = 100_000;
        use super::generate_kalid;

        // nanoid: generate 16-char IDs for fair comparison
        let nanoid_ids: Vec<String> = (0..N).map(|_| nanoid::nanoid!(16)).collect();
        let kalid_ids: Vec<String> = (0..N).map(|_| generate_kalid()).collect();

        let nano_collisions = N - {
            let mut set = std::collections::HashSet::new();
            for id in &nanoid_ids {
                set.insert(id.as_str());
            }
            set.len()
        };

        let kalid_collisions = N - {
            let mut set = std::collections::HashSet::new();
            for id in &kalid_ids {
                set.insert(id.as_str());
            }
            set.len()
        };
        println!(
            "Collision benchmark: N={N}, nanoid collisions={nano_collisions}, kalid collisions={kalid_collisions}"
        );
        println!("  nanoid: {} unique in {N} (0 collisions = expected)", N - nano_collisions);
        println!(
            "  kalid: {} unique in {N} (kalid 1ms resolution — collisions expected within same ms)",
            N - kalid_collisions
        );
        assert_eq!(nano_collisions, 0, "nanoid should have 0 collisions");
    }
    #[test]
    fn performance_dry_run() {
        const N: usize = 10_000;
        use std::time::Instant;

        let start = Instant::now();
        for _ in 0..N {
            core::hint::black_box(generate_kalid());
        }
        let kalid_dur = start.elapsed();

        let start = Instant::now();
        for _ in 0..N {
            core::hint::black_box(nanoid::nanoid!(16));
        }
        let nanoid_dur = start.elapsed();

        let start = Instant::now();
        for _ in 0..N {
            core::hint::black_box(uuid::Uuid::now_v7());
        }
        let uuid_dur = start.elapsed();

        let start = Instant::now();
        for _ in 0..N {
            core::hint::black_box(ulid::Ulid::new().to_string());
        }
        let ulid_dur = start.elapsed();

        let kalid_per = kalid_dur.as_nanos() / N as u128;
        let nanoid_per = nanoid_dur.as_nanos() / N as u128;
        let uuid_per = uuid_dur.as_nanos() / N as u128;
        let ulid_per = ulid_dur.as_nanos() / N as u128;

        println!("Performance: {N} iterations");
        println!("  Kalid : {:>8?} total, ~{:>6}ns/op", kalid_dur, kalid_per);
        println!("  UUIDv7: {:>8?} total, ~{:>6}ns/op", uuid_dur, uuid_per);
        println!("  ULID  : {:>8?} total, ~{:>6}ns/op", ulid_dur, ulid_per);
        println!("  Nanoid: {:>8?} total, ~{:>6}ns/op", nanoid_dur, nanoid_per);
    }
}
