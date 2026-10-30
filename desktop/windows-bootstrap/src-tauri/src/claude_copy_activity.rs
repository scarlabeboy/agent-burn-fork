//! Folder-local raw claims by UTC day. No I/O, discovery, clock or deduplication.
use abf_collection_boundary::{
    ClaudeBatch, ClaudeComponents, ClaudeField, ClaudeLineOutcome, summarize_claude_batch,
};
use std::collections::BTreeMap;

const MAX_ROWS: usize = 32; // Includes the undated bucket; never truncate rows.
const UNAVAILABLE: &str = "{\"status\":\"budget-exhausted\"}";

pub(super) fn folder_payload_json(batch: &ClaudeBatch, diagnostic: String) -> String {
    let versioned = diagnostic.replacen("{\"schemaVersion\":1,", "{\"schemaVersion\":2,", 1);
    let prefix = format!("{},\"activity\":", &versioned[..versioned.len() - 1]);
    let activity = activity_json(batch, 16384usize.saturating_sub(prefix.len() + 1));
    let result = format!("{prefix}{activity}}}");
    // Even an otherwise valid diagnostic occupying the entire cap remains available.
    // A legacy folder response renders activity as unavailable in the frontend.
    if result.len() <= 16384 {
        result
    } else {
        diagnostic
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ActivityRow {
    pub day: Option<String>,
    pub observations: u64,
    pub components: ClaudeComponents,
}

pub(super) fn project(batch: &ClaudeBatch) -> Result<Vec<ActivityRow>, ()> {
    if batch.files.len() > 16 || batch.files.iter().map(|f| f.lines.len()).sum::<usize>() > 4096 {
        return Err(());
    }
    let mut groups = BTreeMap::<String, Vec<(usize, usize)>>::new();
    for (file_index, file) in batch.files.iter().enumerate() {
        for (line_index, line) in file.lines.iter().enumerate() {
            if let ClaudeLineOutcome::Observation(observation) = &line.outcome {
                let day = match &observation.timestamp {
                    ClaudeField::Valid(value) => utc_day(value),
                    _ => None,
                };
                groups
                    .entry(day.unwrap_or_default())
                    .or_default()
                    .push((file_index, line_index));
                if groups.len() > MAX_ROWS {
                    return Err(());
                }
            }
        }
    }
    // Keep input untouched. Reuse the actual raw summary for each subset, including its
    // cache-creation resolution, field-state counts and checked overflow behavior.
    // At most one extra batch clone and one bucket's retained lines exist at a time.
    let mut scratch = batch.clone();
    for file in &mut scratch.files {
        file.lines.clear();
    }
    let undated = groups.remove("");
    let mut rows = Vec::new();
    for (day, positions) in groups
        .into_iter()
        .chain(undated.map(|p| (String::new(), p)))
    {
        for file in &mut scratch.files {
            file.lines.clear();
        }
        for (file, line) in positions {
            scratch.files[file]
                .lines
                .push(batch.files[file].lines[line].clone());
        }
        let summary = summarize_claude_batch(&scratch);
        rows.push(ActivityRow {
            day: (!day.is_empty()).then_some(day),
            observations: summary.records.observations,
            components: summary.components,
        });
    }
    Ok(rows)
}

pub(super) fn activity_json(batch: &ClaudeBatch, byte_budget: usize) -> String {
    let Ok(rows) = project(batch) else {
        return UNAVAILABLE.to_owned();
    };
    let mut result = String::from("{\"status\":\"available\",\"rows\":[");
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            result.push(',');
        }
        let day = row
            .day
            .as_ref()
            .map_or_else(|| "null".to_owned(), |d| format!("\"{d}\""));
        let component = crate::fictional_file_payload::component;
        result.push_str(&format!(
            "{{\"day\":{day},\"observations\":{},\"components\":{{\"input\":{},\"output\":{},\"cacheCreation\":{},\"cacheRead\":{}}}}}",
            row.observations,
            component(&row.components.input), component(&row.components.output),
            component(&row.components.cache_creation), component(&row.components.cache_read),
        ));
        if result.len() + 2 > byte_budget {
            return UNAVAILABLE.to_owned();
        }
    }
    result.push_str("]}");
    if result.len() > byte_budget {
        UNAVAILABLE.to_owned()
    } else {
        result
    }
}

// Strict supported RFC3339 subset: years 0001..9999, seconds 00..59, optional
// 1..9 fractional digits, Z or a known numeric offset. Unknown -00:00 is undated.
// Normalize the calendar day using minute arithmetic; never export the original string.
fn utc_day(value: &str) -> Option<String> {
    let b = value.as_bytes();
    if !(20..=35).contains(&b.len())
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let digits = |part: &[u8]| -> Option<i32> {
        part.iter().try_fold(0, |n, c| {
            c.is_ascii_digit().then(|| n * 10 + i32::from(c - b'0'))
        })
    };
    let mut year = digits(&b[..4])?;
    let mut month = digits(&b[5..7])?;
    let mut day = digits(&b[8..10])?;
    let hour = digits(&b[11..13])?;
    let minute = digits(&b[14..16])?;
    let second = digits(&b[17..19])?;
    if year == 0
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut tz = 19;
    if b[tz] == b'.' {
        tz += 1;
        let start = tz;
        while tz < b.len() && b[tz].is_ascii_digit() {
            tz += 1;
        }
        if !(1..=9).contains(&(tz - start)) {
            return None;
        }
    }
    let offset = match &b[tz..] {
        [b'Z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let h = digits(&[*h1, *h2])?;
            let m = digits(&[*m1, *m2])?;
            if h > 23 || m > 59 || (*sign == b'-' && h == 0 && m == 0) {
                return None;
            }
            (h * 60 + m) * if *sign == b'-' { -1 } else { 1 }
        }
        _ => return None,
    };
    let utc_minute = hour * 60 + minute - offset;
    if utc_minute < 0 {
        day -= 1;
        if day == 0 {
            month -= 1;
            if month == 0 {
                year -= 1;
                month = 12;
            }
            day = days_in_month(year, month);
        }
    } else if utc_minute >= 1440 {
        day += 1;
        if day > days_in_month(year, month) {
            day = 1;
            month += 1;
            if month == 13 {
                year += 1;
                month = 1;
            }
        }
    }
    (1..=9999)
        .contains(&year)
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
