//! Dates as the calendar on this computer reads them. GitHub's timestamps are UTC, so a release
//! published late on the 7th in UTC is the 8th in Tokyo: every date the launcher shows goes through
//! [`civil`], which adds the system time zone's offset at that instant (summer time included).
//!
//! Invariants: nothing here panics; when the system zone can't be read, dates fall back to UTC.
//! The desktop asks jiff, which re-reads the zone every few minutes, so a laptop that travels keeps
//! up; the web build asks the browser.

use artcraft_launcher_core::time;

/// `(year, month, day)` of the Unix time `secs` in the system's time zone.
pub(crate) fn civil(secs: i64) -> (i64, u32, u32) {
    civil_at(secs, offset(secs))
}

/// `(year, month, day)` of `secs` seen from `offset` seconds east of UTC.
fn civil_at(secs: i64, offset: i64) -> (i64, u32, u32) {
    time::civil_from_unix(secs.saturating_add(offset))
}

/// Seconds east of UTC in the system's time zone at `secs`; 0 when the zone can't be read.
#[cfg(not(target_arch = "wasm32"))]
fn offset(secs: i64) -> i64 {
    let zone = match jiff::tz::TimeZone::try_system() {
        Ok(zone) => zone,
        Err(e) => {
            log::debug!("couldn't read the system time zone, showing dates in UTC: {e}");
            return 0;
        }
    };
    jiff::Timestamp::from_second(secs).map_or(0, |at| i64::from(zone.to_offset(at).seconds()))
}

/// Seconds east of UTC in the browser's time zone at `secs`; 0 when it gives nothing sensible.
#[cfg(target_arch = "wasm32")]
fn offset(secs: i64) -> i64 {
    // Date's offset is in minutes *west* of UTC. Real zones are within a day of it.
    let at = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(secs as f64 * 1000.0));
    let west = at.get_timezone_offset();
    if west.is_finite() && west.abs() <= 24.0 * 60.0 { -(west.round() as i64) * 60 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::civil_at;

    #[test]
    fn a_date_follows_the_zone_it_is_read_in() {
        // 2026-10-07 22:30 UTC.
        let at = 1_791_412_200;
        assert_eq!(civil_at(at, 0), (2026, 10, 7));
        // Tokyo, UTC+9: already the 8th.
        assert_eq!(civil_at(at, 9 * 3600), (2026, 10, 8));
        // New York in summer, UTC−4: still the 7th.
        assert_eq!(civil_at(at, -4 * 3600), (2026, 10, 7));
        // Just after midnight UTC on the 8th, Los Angeles (UTC−7) reads the 7th.
        assert_eq!(civil_at(1_791_417_660, -7 * 3600), (2026, 10, 7));
        // Out-of-range offsets saturate rather than overflow.
        assert_eq!(civil_at(i64::MAX, 3600), civil_at(i64::MAX, 0));
    }
}
