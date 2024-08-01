use pragma_entities::EntryError;
use serde::{Deserialize, Deserializer};
use std::ops::RangeInclusive;
use utoipa::ToSchema;

/// The number of seconds since the Unix epoch (00:00:00 UTC on 1 Jan 1970). The timestamp is
/// always positive, but represented as a signed integer because that's the standard on Unix
/// systems and allows easy subtraction to compute durations.
pub type UnixTimestamp = i64;

/// Represents
#[derive(Debug, Clone, ToSchema)]
pub enum TimestampParam {
    Single(UnixTimestamp),
    Range(RangeInclusive<UnixTimestamp>),
}

impl From<UnixTimestamp> for TimestampParam {
    fn from(ts: UnixTimestamp) -> Self {
        TimestampParam::Single(ts)
    }
}

impl Default for TimestampParam {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp();
        TimestampParam::Single(now)
    }
}

impl TimestampParam {
    pub fn assert_time_is_valid(self) -> Result<Self, EntryError> {
        let now = chrono::Utc::now().timestamp();
        match &self {
            Self::Single(ts) => {
                if *ts > now {
                    return Err(EntryError::InvalidTimestamp(
                        "Timestamp is in the future.".into(),
                    ));
                }
            }
            Self::Range(range) => {
                if range.start() > range.end() {
                    return Err(EntryError::InvalidTimestamp(
                        "Range timestamp first date is greater than the second date.".into(),
                    ));
                }
                if *range.end() > now {
                    return Err(EntryError::InvalidTimestamp(
                        "Range timestamp end is in the future.".into(),
                    ));
                }
            }
        }
        Ok(self)
    }
}

impl<'de> Deserialize<'de> for TimestampParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        if s.is_empty() {
            return Ok(TimestampParam::default());
        }

        if let Some((start, end)) = s.split_once(',') {
            let start = start.parse().map_err(serde::de::Error::custom)?;
            let end = end.parse().map_err(serde::de::Error::custom)?;
            Ok(TimestampParam::Range(start..=end))
        } else {
            let ts = s.parse().map_err(serde::de::Error::custom)?;
            Ok(TimestampParam::Single(ts))
        }
    }
}
