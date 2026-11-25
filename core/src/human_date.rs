//! Dates shown the way people say them, in French: "Aujourd'hui", "Hier", "lundi dernier", "12 mars",
//! "il y a 5 minutes", "dans 2 jours". No dependency besides chrono: the words are here.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};

pub const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
pub const WEEKDAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];

/// "12 mars" for the year of `today`, "12 mars 2025" for any other
fn long_day(day: NaiveDate, today: NaiveDate) -> String {
    let base = format!("{} {}", if day.day() == 1 { "1er".to_string() } else { day.day().to_string() }, MONTHS[day.month0() as usize]);
    if day.year() == today.year() { base } else { format!("{base} {}", day.year()) }
}

/// A day by its name when it is near (today, yesterday, a weekday of the last or next 6 days), else by its
/// date: "Aujourd'hui", "Hier", "Demain", "lundi dernier", "vendredi prochain", "12 mars", "12 mars 2025"
pub fn human_day(day: NaiveDate, today: NaiveDate) -> String {
    let gap = (day - today).num_days();
    match gap {
        0 => "Aujourd'hui".into(),
        1 => "Demain".into(),
        -1 => "Hier".into(),
        2 => "Après-demain".into(),
        -2 => "Avant-hier".into(),
        3..=6 => format!("{} prochain", WEEKDAYS[day.weekday().num_days_from_monday() as usize]),
        -6..=-3 => format!("{} dernier", WEEKDAYS[day.weekday().num_days_from_monday() as usize]),
        _ => long_day(day, today),
    }
}

/// A date and time as the Finder says it: "à l'instant", "il y a 5 min" within the hour, then
/// "Aujourd'hui à 14:32", "Hier à 09:10", "lundi dernier à 09:10", "12 mars à 09:10", "12 mars 2025"
pub fn human_datetime(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let seconds = (now - at).num_seconds();
    if (0..60).contains(&seconds) {
        return "à l'instant".into();
    }
    if (60..3600).contains(&seconds) {
        return format!("il y a {} min", seconds / 60);
    }
    let day = at.date();
    let time = format!("{:02}:{:02}", at.hour(), at.minute());
    let today = now.date();
    if (day - today).num_days().abs() <= 6 {
        return format!("{} à {time}", human_day(day, today));
    }
    if day.year() == today.year() {
        format!("{} à {time}", long_day(day, today))
    } else {
        long_day(day, today) // a date of another year: no time
    }
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The gap between two instants in words: "il y a 5 minutes", "il y a 1 heure", "dans 2 jours",
/// "il y a 3 mois", "à l'instant". The unit is the largest that fits at least once
pub fn human_ago(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let seconds = (now - at).num_seconds();
    let past = seconds >= 0;
    let s = seconds.abs();
    if s < 45 {
        return "à l'instant".into();
    }
    let amount = if s < 3600 {
        plural((s as f64 / 60.0).round().max(1.0) as i64, "minute", "minutes")
    } else if s < 86_400 {
        plural(s / 3600, "heure", "heures")
    } else if s < 7 * 86_400 {
        plural(s / 86_400, "jour", "jours")
    } else if s < 30 * 86_400 {
        plural(s / (7 * 86_400), "semaine", "semaines")
    } else if s < 365 * 86_400 {
        plural(s / (30 * 86_400), "mois", "mois")
    } else {
        plural(s / (365 * 86_400), "an", "ans")
    };
    if past { format!("il y a {amount}") } else { format!("dans {amount}") }
}

/// The full date: "mercredi 25 novembre 2026"
pub fn human_full_date(day: NaiveDate) -> String {
    let n = if day.day() == 1 { "1er".to_string() } else { day.day().to_string() };
    format!("{} {n} {} {}", WEEKDAYS[day.weekday().num_days_from_monday() as usize], MONTHS[day.month0() as usize], day.year())
}

/// The first day of the week (Monday) that holds `day`
pub fn week_start(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_monday() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn t(y: i32, m: u32, day: u32, h: u32, min: u32) -> NaiveDateTime {
        d(y, m, day).and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn near_days_have_a_name() {
        let today = d(2026, 11, 25); // a Wednesday
        assert_eq!(human_day(today, today), "Aujourd'hui");
        assert_eq!(human_day(d(2026, 11, 24), today), "Hier");
        assert_eq!(human_day(d(2026, 11, 26), today), "Demain");
        assert_eq!(human_day(d(2026, 11, 22), today), "dimanche dernier");
        assert_eq!(human_day(d(2026, 11, 28), today), "samedi prochain");
    }

    #[test]
    fn far_days_have_a_date() {
        let today = d(2026, 11, 25);
        assert_eq!(human_day(d(2026, 3, 12), today), "12 mars");
        assert_eq!(human_day(d(2026, 3, 1), today), "1er mars");
        assert_eq!(human_day(d(2025, 3, 12), today), "12 mars 2025");
    }

    #[test]
    fn a_time_is_said_by_its_age() {
        let now = t(2026, 11, 25, 15, 0);
        assert_eq!(human_datetime(t(2026, 11, 25, 14, 59), now), "il y a 1 min");
        assert_eq!(human_datetime(t(2026, 11, 25, 15, 0), now), "à l'instant");
        assert_eq!(human_datetime(t(2026, 11, 25, 9, 10), now), "Aujourd'hui à 09:10");
        assert_eq!(human_datetime(t(2026, 11, 24, 9, 10), now), "Hier à 09:10");
        assert_eq!(human_datetime(t(2026, 3, 12, 9, 10), now), "12 mars à 09:10");
        assert_eq!(human_datetime(t(2025, 3, 12, 9, 10), now), "12 mars 2025");
    }

    #[test]
    fn the_gap_is_in_words() {
        let now = t(2026, 11, 25, 15, 0);
        assert_eq!(human_ago(t(2026, 11, 25, 14, 55), now), "il y a 5 minutes");
        assert_eq!(human_ago(t(2026, 11, 25, 14, 0), now), "il y a 1 heure");
        assert_eq!(human_ago(t(2026, 11, 20, 15, 0), now), "il y a 5 jours");
        assert_eq!(human_ago(t(2026, 11, 27, 15, 0), now), "dans 2 jours");
        assert_eq!(human_ago(t(2026, 8, 25, 15, 0), now), "il y a 3 mois");
        assert_eq!(human_ago(t(2024, 11, 25, 15, 0), now), "il y a 2 ans");
    }

    #[test]
    fn a_full_date_is_written_out() {
        assert_eq!(human_full_date(d(2026, 11, 25)), "mercredi 25 novembre 2026");
        assert_eq!(week_start(d(2026, 11, 25)), d(2026, 11, 23));
    }
}
