//! A date typed by a person, in French: "12/03/2026", "12 mars", "demain", "lundi prochain", "dans 3 jours",
//! "fin mars", "il y a 2 semaines". The French rules that people expect (day before month, weekdays, end
//! of month) are here; the rest is left to the `rosetta-date` crate (relative phrases, dates with the
//! name of the month, ISO dates).

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};
use rosetta_date::{i18n, parse_with_options, ParseOptions, RosettaDateTime};

use crate::human_date::{MONTHS, WEEKDAYS};

/// The date a text says, `today` being the day it is said. None when it is not understood
pub fn parse_date(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    let t = text.trim().to_lowercase().replace('’', "'");
    if t.is_empty() {
        return None;
    }
    numeric(&t, today).or_else(|| french(&t, today)).or_else(|| rosetta(&t, today))
}

/// A range typed as two dates around "au", "à", "-" or "→": "12/03/2026 au 20/03/2026". An end that is
/// empty or says "∞" is open
pub fn parse_range(text: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
    let t = text.trim();
    for sep in [" → ", "→", " au ", " jusqu'au ", " jusqu'à ", " à ", " - "] {
        if let Some((a, b)) = t.split_once(sep) {
            let open = |s: &str| matches!(s.trim(), "" | "∞" | "..." | "…");
            let side = |s: &str| if open(s) { Some(None) } else { parse_date(s, today).map(Some) };
            if let (Some(start), Some(end)) = (side(a), side(b)) {
                return Some(match (start, end) {
                    (Some(s), Some(e)) if s > e => (Some(e), Some(s)),
                    other => other,
                });
            }
        }
    }
    None
}

/// Digits with a separator: day first, as in France ("12/03/2026" is the 12th of March)
fn numeric(t: &str, today: NaiveDate) -> Option<NaiveDate> {
    // two digits for the year: "12/03/26" is 2026
    for (format, sep) in [("%d/%m/%y", '/'), ("%d-%m-%y", '-'), ("%d.%m.%y", '.')] {
        if t.split(sep).count() == 3 && t.rsplit(sep).next().is_some_and(|y| y.len() == 2) {
            if let Ok(d) = NaiveDate::parse_from_str(t, format) {
                return Some(d);
            }
        }
    }
    for format in ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%Y-%m-%d", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, format) {
            return Some(d);
        }
    }
    // no year: the one of today
    for sep in ['/', '-', '.'] {
        let parts: Vec<&str> = t.split(sep).collect();
        if let [day, month] = parts.as_slice() {
            if let (Ok(day), Ok(month)) = (day.parse::<u32>(), month.parse::<u32>()) {
                return NaiveDate::from_ymd_opt(today.year(), month, day);
            }
        }
    }
    None
}

fn month_number(word: &str) -> Option<u32> {
    let word = word.trim_end_matches('.');
    MONTHS.iter().position(|m| *m == word).or_else(|| {
        const SHORT: [&str; 12] = ["janv", "févr", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov", "déc"];
        SHORT.iter().position(|m| *m == word)
    }).map(|i| i as u32 + 1)
}

fn last_day(year: i32, month: u32) -> Option<NaiveDate> {
    let next = if month == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1)? } else { NaiveDate::from_ymd_opt(year, month + 1, 1)? };
    Some(next - Duration::days(1))
}

/// The rules that people expect and the crate does not know: weekdays, "fin"/"début" of a month
fn french(t: &str, today: NaiveDate) -> Option<NaiveDate> {
    match t {
        "aujourd'hui" | "ajd" => return Some(today),
        "demain" => return Some(today + Duration::days(1)),
        "après-demain" | "apres-demain" => return Some(today + Duration::days(2)),
        "hier" => return Some(today - Duration::days(1)),
        "avant-hier" => return Some(today - Duration::days(2)),
        "fin du mois" | "fin de mois" | "fin de ce mois" => return last_day(today.year(), today.month()),
        "début du mois" | "debut du mois" | "début de mois" => return today.with_day(1),
        "fin de l'année" | "fin d'année" | "fin de l'an" => return NaiveDate::from_ymd_opt(today.year(), 12, 31),
        "début de l'année" | "début d'année" | "debut de l'année" => return NaiveDate::from_ymd_opt(today.year(), 1, 1),
        _ => {}
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    // "fin mars", "fin de mars 2025", "début avril", "debut d'avril"
    if let Some((&first, rest)) = words.split_first() {
        if matches!(first, "fin" | "début" | "debut") {
            let rest: Vec<&str> = rest.iter().copied().filter(|w| !matches!(*w, "de" | "du" | "d'") ).collect();
            let rest: Vec<String> = rest.iter().map(|w| w.trim_start_matches("d'").to_string()).collect();
            if let Some(month) = rest.first().and_then(|w| month_number(w)) {
                let year = rest.get(1).and_then(|y| y.parse::<i32>().ok()).unwrap_or(today.year());
                return if first == "fin" { last_day(year, month) } else { NaiveDate::from_ymd_opt(year, month, 1) };
            }
        }
    }
    // a weekday: "lundi" (the next one, today included), "lundi prochain", "lundi dernier", "ce lundi"
    let words: Vec<&str> = words.into_iter().filter(|w| !matches!(*w, "ce" | "le" | "ce.")).collect();
    let weekday = words.first().and_then(|w| WEEKDAYS.iter().position(|d| d == w))? as i64;
    let current = today.weekday().num_days_from_monday() as i64;
    let ahead = (weekday - current).rem_euclid(7);
    match words.get(1).copied() {
        None => Some(today + Duration::days(ahead)),
        Some("prochain") => Some(today + Duration::days(if ahead == 0 { 7 } else { ahead })),
        Some("dernier" | "passé" | "precedent" | "précédent") => {
            let back = (current - weekday).rem_euclid(7);
            Some(today - Duration::days(if back == 0 { 7 } else { back }))
        }
        _ => None,
    }
}

/// The rest: the relative phrases and the dates written with a month name, read by `rosetta-date` with the
/// day of `today` at noon as the reference (so that the answer does not depend on the time zone)
fn rosetta(t: &str, today: NaiveDate) -> Option<NaiveDate> {
    let noon = today.and_time(NaiveTime::from_hms_opt(12, 0, 0)?);
    let utc = chrono::FixedOffset::east_opt(0)?;
    let base = RosettaDateTime::Chrono(noon.and_local_timezone(utc).single()?);
    let options = ParseOptions { base_time: Some(base), languages: Some(vec![&i18n::fr::FRENCH, &i18n::en::ENGLISH]), ..ParseOptions::default() };
    let parsed = parse_with_options(t, &options).ok()?;
    NaiveDate::from_ymd_opt(parsed.year(), parsed.month() as u32, parsed.day() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    const TODAY: (i32, u32, u32) = (2026, 11, 25); // a Wednesday

    fn parse(text: &str) -> Option<NaiveDate> {
        parse_date(text, d(TODAY.0, TODAY.1, TODAY.2))
    }

    #[test]
    fn the_day_comes_before_the_month() {
        assert_eq!(parse("12/03/2026"), Some(d(2026, 3, 12)));
        assert_eq!(parse("12-03-2026"), Some(d(2026, 3, 12)));
        assert_eq!(parse("12.03.26"), Some(d(2026, 3, 12)));
        assert_eq!(parse("12/03"), Some(d(2026, 3, 12)));
        assert_eq!(parse("2026-03-12"), Some(d(2026, 3, 12)));
    }

    #[test]
    fn relative_words() {
        assert_eq!(parse("demain"), Some(d(2026, 11, 26)));
        assert_eq!(parse("Après-demain"), Some(d(2026, 11, 27)));
        assert_eq!(parse("hier"), Some(d(2026, 11, 24)));
        assert_eq!(parse("aujourd'hui"), Some(d(2026, 11, 25)));
        assert_eq!(parse("dans 3 jours"), Some(d(2026, 11, 28)));
        assert_eq!(parse("il y a 2 semaines"), Some(d(2026, 11, 11)));
    }

    #[test]
    fn weekdays() {
        assert_eq!(parse("lundi prochain"), Some(d(2026, 11, 30)));
        assert_eq!(parse("lundi dernier"), Some(d(2026, 11, 23)));
        assert_eq!(parse("mercredi"), Some(d(2026, 11, 25)));
        assert_eq!(parse("mercredi prochain"), Some(d(2026, 12, 2)));
        assert_eq!(parse("mercredi dernier"), Some(d(2026, 11, 18)));
        assert_eq!(parse("vendredi"), Some(d(2026, 11, 27)));
    }

    #[test]
    fn months() {
        assert_eq!(parse("12 mars 2026"), Some(d(2026, 3, 12)));
        assert_eq!(parse("fin mars"), Some(d(2026, 3, 31)));
        assert_eq!(parse("fin février 2024"), Some(d(2024, 2, 29)));
        assert_eq!(parse("début avril"), Some(d(2026, 4, 1)));
        assert_eq!(parse("fin du mois"), Some(d(2026, 11, 30)));
        assert_eq!(parse("fin d'année"), Some(d(2026, 12, 31)));
    }

    #[test]
    fn what_is_not_a_date() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("bonjour"), None);
        assert_eq!(parse("32/13/2026"), None);
    }

    #[test]
    fn a_range_may_be_open() {
        let today = d(2026, 11, 25);
        assert_eq!(parse_range("12/03/2026 au 20/03/2026", today), Some((Some(d(2026, 3, 12)), Some(d(2026, 3, 20)))));
        assert_eq!(parse_range("20/03/2026 → 12/03/2026", today), Some((Some(d(2026, 3, 12)), Some(d(2026, 3, 20)))));
        assert_eq!(parse_range("12/03/2026 → ∞", today), Some((Some(d(2026, 3, 12)), None)));
        assert_eq!(parse_range("∞ → 12/03/2026", today), Some((None, Some(d(2026, 3, 12)))));
    }
}

// ---- periods and times of day -------------------------------------------------------------------------

/// -1, 0 or 1 for "dernier", "ci" (this one), "prochain"; None for another word
fn direction(word: &str) -> Option<i64> {
    match word {
        "prochain" | "prochaine" => Some(1),
        "dernier" | "dernière" | "derniere" | "passé" | "passée" => Some(-1),
        "ci" | "courant" | "courante" | "actuel" | "actuelle" => Some(0),
        _ => None,
    }
}

fn add_months(first: NaiveDate, months: i64) -> Option<NaiveDate> {
    let index = first.year() as i64 * 12 + first.month0() as i64 + months;
    NaiveDate::from_ymd_opt(index.div_euclid(12) as i32, index.rem_euclid(12) as u32 + 1, 1)
}

/// A whole week, month or year, for a range: "cette semaine" (Monday to Sunday), "semaine prochaine",
/// "la semaine dernière", "ce mois-ci", "le mois dernier", "mois prochain", "cette année", "l'année prochaine"
pub fn parse_period(text: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let t = text.trim().to_lowercase().replace('’', "'").replace("mois-ci", "mois ci").replace("semaine-ci", "semaine ci");
    let words: Vec<String> = t
        .split_whitespace()
        .map(|w| w.trim_start_matches("l'").to_string())
        .filter(|w| !matches!(w.as_str(), "" | "la" | "le" | "cette" | "ce" | "cet" | "de" | "du"))
        .collect();
    let (unit, rest) = words.split_first()?;
    let step = match rest {
        [] => 0,
        [one] => direction(one)?,
        _ => return None,
    };
    match unit.as_str() {
        "semaine" => {
            let start = crate::human_date::week_start(today) + Duration::days(7 * step);
            Some((start, start + Duration::days(6)))
        }
        "mois" => {
            let start = add_months(today.with_day(1)?, step)?;
            Some((start, add_months(start, 1)? - Duration::days(1)))
        }
        "année" | "annee" | "an" => {
            let year = today.year() + step as i32;
            Some((NaiveDate::from_ymd_opt(year, 1, 1)?, NaiveDate::from_ymd_opt(year, 12, 31)?))
        }
        _ => None,
    }
}

/// "15h", "15h30", "15:30", "midi", "minuit"
fn clock(word: &str) -> Option<NaiveTime> {
    match word {
        "midi" => return NaiveTime::from_hms_opt(12, 0, 0),
        "minuit" => return NaiveTime::from_hms_opt(0, 0, 0),
        _ => {}
    }
    let (hours, minutes) = word.split_once(['h', ':'])?;
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = if minutes.is_empty() { 0 } else { minutes.parse().ok()? };
    NaiveTime::from_hms_opt(hours, minutes, 0)
}

/// A date and, when the text gives them, a time of day and the end of the slot ("de 15h à 16h")
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTimeInput {
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
}

/// A date with a time of day or a slot: "demain à 15h", "lundi prochain 9h30", "12/03/2026 à 14:00",
/// "demain de 15h à 16h", "lundi 9h-10h30", "entre 14h et 15h", "midi". "dans 2 heures" and "il y a 30 minutes"
/// count from `now`
pub fn parse_datetime(text: &str, now: chrono::NaiveDateTime) -> Option<DateTimeInput> {
    let t = text.trim().to_lowercase().replace('’', "'").replace(['–', '—'], "-");
    let words: Vec<&str> = t.split_whitespace().collect();
    // a delay in hours or minutes, from now
    let delay = |n: &str, unit: &str| -> Option<Duration> {
        let n: i64 = n.parse().ok()?;
        match unit.trim_end_matches('s') {
            "heure" | "h" => Some(Duration::hours(n)),
            "minute" | "min" => Some(Duration::minutes(n)),
            _ => None,
        }
    };
    let moved = match words.as_slice() {
        ["dans", n, unit] => delay(n, unit).map(|d| now + d),
        ["il", "y", "a", n, unit] => delay(n, unit).map(|d| now - d),
        _ => None,
    };
    if let Some(at) = moved {
        let time = at.time().with_second(0)?;
        return Some(DateTimeInput { date: at.date(), time: Some(time), end_time: None });
    }
    // the words that are not a time of day, and the times found: "9h-10h30" is two times
    let mut rest: Vec<&str> = vec![];
    let mut times: Vec<NaiveTime> = vec![];
    for word in words {
        if let Some(time) = clock(word) {
            times.push(time);
        } else if let Some(pair) = word.split_once('-').and_then(|(a, b)| Some((clock(a)?, clock(b)?))) {
            times.extend([pair.0, pair.1]);
        } else {
            rest.push(word);
        }
    }
    if times.len() > 2 {
        return None;
    }
    // the words that tie the times to the date: "à 15h", "de 15h à 16h", "entre 14h et 15h", "vers 9h"
    let connectors: &[&str] = if times.is_empty() { &[] } else { &["à", "a", "de", "entre", "et", "jusqu'à", "jusqu'a", "vers", "@", "-", "au"] };
    rest.retain(|w| !connectors.contains(w));
    let text = rest.join(" ");
    let date = if text.is_empty() && !times.is_empty() { now.date() } else { parse_date(&text, now.date())? };
    let end_time = times.get(1).copied();
    let time = times.first().copied();
    if end_time.zip(time).is_some_and(|(end, start)| end <= start) {
        return None; // a slot ends after it starts
    }
    Some(DateTimeInput { date, time, end_time })
}

#[cfg(test)]
mod more_tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn periods() {
        let today = d(2026, 11, 25); // a Wednesday
        assert_eq!(parse_period("cette semaine", today), Some((d(2026, 11, 23), d(2026, 11, 29))));
        assert_eq!(parse_period("semaine prochaine", today), Some((d(2026, 11, 30), d(2026, 12, 6))));
        assert_eq!(parse_period("la semaine dernière", today), Some((d(2026, 11, 16), d(2026, 11, 22))));
        assert_eq!(parse_period("ce mois-ci", today), Some((d(2026, 11, 1), d(2026, 11, 30))));
        assert_eq!(parse_period("le mois dernier", today), Some((d(2026, 10, 1), d(2026, 10, 31))));
        assert_eq!(parse_period("mois prochain", today), Some((d(2026, 12, 1), d(2026, 12, 31))));
        assert_eq!(parse_period("l'année prochaine", today), Some((d(2027, 1, 1), d(2027, 12, 31))));
        assert_eq!(parse_period("cette année", today), Some((d(2026, 1, 1), d(2026, 12, 31))));
        assert_eq!(parse_period("demain", today), None);
    }

    #[test]
    fn a_time_of_day_and_a_slot() {
        let now = d(2026, 11, 25).and_hms_opt(10, 30, 0).unwrap();
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        let input = |date, time, end_time| Some(DateTimeInput { date, time, end_time });
        assert_eq!(parse_datetime("demain à 15h", now), input(d(2026, 11, 26), at(15, 0), None));
        assert_eq!(parse_datetime("demain 15h30", now), input(d(2026, 11, 26), at(15, 30), None));
        assert_eq!(parse_datetime("lundi prochain à 9:00", now), input(d(2026, 11, 30), at(9, 0), None));
        assert_eq!(parse_datetime("12/03/2026 à 14:00", now), input(d(2026, 3, 12), at(14, 0), None));
        assert_eq!(parse_datetime("midi", now), input(d(2026, 11, 25), at(12, 0), None));
        assert_eq!(parse_datetime("demain", now), input(d(2026, 11, 26), None, None));
        // a slot
        assert_eq!(parse_datetime("de 15h à 16h", now), input(d(2026, 11, 25), at(15, 0), at(16, 0)));
        assert_eq!(parse_datetime("demain de 15h à 16h", now), input(d(2026, 11, 26), at(15, 0), at(16, 0)));
        assert_eq!(parse_datetime("lundi prochain 9h-10h30", now), input(d(2026, 11, 30), at(9, 0), at(10, 30)));
        assert_eq!(parse_datetime("demain entre 14h et 15h", now), input(d(2026, 11, 26), at(14, 0), at(15, 0)));
        assert_eq!(parse_datetime("demain de 16h à 15h", now), None);
        // a delay
        assert_eq!(parse_datetime("dans 2 heures", now), input(d(2026, 11, 25), at(12, 30), None));
        assert_eq!(parse_datetime("il y a 30 minutes", now), input(d(2026, 11, 25), at(10, 0), None));
        assert_eq!(parse_datetime("n'importe quoi à 15h", now), None);
    }
}

/// What a person types in the field of a date range picker: a period ("cette semaine", "ce mois-ci"), two dates
/// ("12/03 au 20/03", "lundi → ∞"), an open range ("depuis lundi", "jusqu'au 20/03"), one day ("demain"),
/// or "tout" for every date. The ends are `None` when open
pub fn parse_any_range(text: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
    let t = text.trim().to_lowercase().replace('’', "'");
    if matches!(t.as_str(), "tout" | "toutes" | "toutes les dates" | "∞" | "") {
        return Some((None, None));
    }
    for prefix in ["à partir du ", "à partir de ", "a partir du ", "à partir d'", "depuis le ", "depuis ", "dès le ", "dès "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return parse_date(rest, today).map(|d| (Some(d), None));
        }
    }
    for prefix in ["jusqu'au ", "jusqu'à ", "jusqu'a ", "avant le ", "avant "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return parse_date(rest, today).map(|d| (None, Some(d)));
        }
    }
    parse_period(&t, today)
        .map(|(s, e)| (Some(s), Some(e)))
        .or_else(|| parse_range(&t, today))
        .or_else(|| parse_date(&t, today).map(|d| (Some(d), Some(d))))
}

#[cfg(test)]
mod any_range_tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn what_a_person_types_in_the_field() {
        let today = d(2026, 11, 25);
        assert_eq!(parse_any_range("cette semaine", today), Some((Some(d(2026, 11, 23)), Some(d(2026, 11, 29)))));
        assert_eq!(parse_any_range("depuis lundi dernier", today), Some((Some(d(2026, 11, 23)), None)));
        assert_eq!(parse_any_range("à partir du 12/03/2026", today), Some((Some(d(2026, 3, 12)), None)));
        assert_eq!(parse_any_range("jusqu'au 20/03/2026", today), Some((None, Some(d(2026, 3, 20)))));
        assert_eq!(parse_any_range("12/03/2026 au 20/03/2026", today), Some((Some(d(2026, 3, 12)), Some(d(2026, 3, 20)))));
        assert_eq!(parse_any_range("demain", today), Some((Some(d(2026, 11, 26)), Some(d(2026, 11, 26)))));
        assert_eq!(parse_any_range("tout", today), Some((None, None)));
        assert_eq!(parse_any_range("n'importe quoi", today), None);
    }
}
