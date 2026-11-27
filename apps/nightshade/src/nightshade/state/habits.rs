//! What the user searches, how often, and at what time of day: the
//! suggestion chips lead with their habits for now ("study spots" every
//! evening comes first in the evening). Kept on the phone only, with the
//! rest of the app's state.

use crate::nightshade::*;
use serde::{Deserialize, Serialize};

/// Parts of the day, by local hour: morning from 5, lunch from 11,
/// afternoon from 14, evening from 17, late night from 22.
pub(crate) const DAYPARTS: usize = 5;
const DAYPART_STARTS: [u32; DAYPARTS] = [5, 11, 14, 17, 22];
/// Searches kept; the least used go first.
const MAX_HABITS: usize = 50;
/// A search made this many times is a habit.
const HABIT_MIN: u32 = 2;
/// Ottawa's offset in winter, until the backend says otherwise.
const DEFAULT_UTC_OFFSET_MIN: i32 = -300;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Habit {
    pub(crate) text: String,
    pub(crate) count: u32,
    /// Searches in each part of the day.
    pub(crate) dayparts: [u32; DAYPARTS],
    /// When last searched: days since 1970, local.
    pub(crate) last_day: i64,
}

/// The part of the day for `minute` since local midnight.
pub(crate) fn daypart(minute: u32) -> usize {
    let hour = minute / 60;
    DAYPART_STARTS
        .iter()
        .rposition(|&start| hour >= start)
        // Before 5 is still last night.
        .unwrap_or(DAYPARTS - 1)
}

/// "-05:00" at the end of a backend time -> -300 minutes.
pub(crate) fn utc_offset_minutes(iso: &str) -> Option<i32> {
    let tail = iso.get(iso.len().checked_sub(6)?..)?;
    let sign = match tail.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, minutes) = tail[1..].split_once(':')?;
    Some(sign * (hours.parse::<i32>().ok()? * 60 + minutes.parse::<i32>().ok()?))
}

impl NightshadeApp {
    /// Local time now: (days since 1970, minute of the day).
    pub(crate) fn local_now(&self) -> (i64, u32) {
        let utc = self.now_override.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs() as i64)
        });
        let local = utc + self.utc_offset_min.unwrap_or(DEFAULT_UTC_OFFSET_MIN) as i64 * 60;
        (local.div_euclid(86_400), (local.rem_euclid(86_400) / 60) as u32)
    }

    /// The backend's local time ("…-05:00") tells the phone's offset.
    pub(crate) fn learn_utc_offset(&mut self, iso: &str) {
        if let Some(offset) = utc_offset_minutes(iso) {
            self.utc_offset_min = Some(offset);
        }
    }

    /// Count a new search, at this time of day.
    pub(crate) fn record_habit(&mut self, query: &str) {
        let (day, minute) = self.local_now();
        let part = daypart(minute);
        let habit = match self.habits.iter().position(|h| h.text.eq_ignore_ascii_case(query)) {
            Some(index) => &mut self.habits[index],
            None => {
                if self.habits.len() >= MAX_HABITS {
                    // Make room: the least used, then the longest unused.
                    if let Some(index) = (0..self.habits.len())
                        .min_by_key(|&i| (self.habits[i].count, self.habits[i].last_day))
                    {
                        self.habits.remove(index);
                    }
                }
                self.habits.push(Habit { text: query.to_string(), ..Habit::default() });
                self.habits.last_mut().expect("just pushed")
            }
        };
        habit.text = query.to_string();
        habit.count += 1;
        habit.dayparts[part] += 1;
        habit.last_day = day;
    }

    /// The user's habits for now, best first: searched often, and most of
    /// all at this time of day, and lately.
    pub(crate) fn habits_now(&self) -> Vec<&str> {
        let (today, minute) = self.local_now();
        let part = daypart(minute);
        let score = |habit: &Habit| {
            let lately = match today - habit.last_day {
                days if days <= 7 => 2,
                days if days <= 30 => 1,
                _ => 0,
            };
            habit.dayparts[part] * 3 + habit.count + lately
        };
        let mut habits: Vec<&Habit> = self.habits.iter().filter(|h| h.count >= HABIT_MIN).collect();
        // Ties: the more recent first.
        habits.sort_by(|a, b| score(b).cmp(&score(a)).then(b.last_day.cmp(&a.last_day)));
        habits.into_iter().map(|habit| habit.text.as_str()).collect()
    }
}
