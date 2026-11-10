//! The site's time zone: blog.toml `timezone` (an IANA name like "America/Chicago"), or the
//! computer's own zone when unset. Dates written without an offset (`2026-11-10`,
//! `2026-11-10 09:28`) are read in this zone, and new dates are stamped in it.

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone, Utc};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SiteTz {
    /// The computer's own time zone.
    #[default]
    Local,
    Named(chrono_tz::Tz),
}

impl SiteTz {
    pub fn parse(name: &str) -> Option<SiteTz> {
        name.trim().parse::<chrono_tz::Tz>().ok().map(SiteTz::Named)
    }

    /// A wall-clock time in this zone. In a DST gap the time does not exist; the moment
    /// just after the gap is used. In a DST overlap, the earlier of the two.
    pub fn localize(&self, n: &NaiveDateTime) -> Option<DateTime<FixedOffset>> {
        let pick = |n: &NaiveDateTime| match self {
            SiteTz::Local => Local.from_local_datetime(n).earliest().map(|d| d.fixed_offset()),
            SiteTz::Named(tz) => tz.from_local_datetime(n).earliest().map(|d| d.fixed_offset()),
        };
        pick(n).or_else(|| pick(&(*n + chrono::Duration::hours(1))))
    }

    /// The same moment, shown in this zone.
    pub fn convert(&self, d: &DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        match self {
            SiteTz::Local => d.with_timezone(&Local).fixed_offset(),
            SiteTz::Named(tz) => d.with_timezone(tz).fixed_offset(),
        }
    }

    /// Now, in this zone, to whole seconds (so stored and rendered values are identical).
    pub fn now(&self) -> DateTime<FixedOffset> {
        let now = Utc::now();
        let now = DateTime::from_timestamp(now.timestamp(), 0).unwrap_or(now);
        self.convert(&now.fixed_offset())
    }
}
