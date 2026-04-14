//! Time tools — give the LLM a clear picture of *now* and a way to reason
//! about a future wall-clock moment without actually sleeping.
//!
//! `time.now` runs on every turn the user asks "what time is it" and also
//! gets mirrored into the system prompt so the model never has to *guess*
//! the current date. `time.until` is an advisory helper: it parses a
//! target expression (ISO-8601 timestamp or `HH:MM` today/tomorrow) and
//! reports how long until that moment — useful for "wake me up at 7" style
//! phrasings that should route through the reminder tools (Phase 8.6)
//! rather than a blocking sleep.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike};
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

/// Reports the current local date and time in structured + narrated form.
pub struct TimeNow;

/// Reports the duration until a target wall-clock moment without blocking.
/// Intended as scaffolding for reminder phrasing, not as a scheduler.
pub struct TimeUntil;

/// Public helper so the LLM prompt builder can stamp the current moment
/// at the top of every turn without duplicating formatting logic.
pub fn current_prompt_stamp() -> String {
    let now = Local::now();
    format!(
        "Current local time: {} ({}). Use this as ground truth whenever the user asks about time, dates, or 'right now'.",
        now.format("%A, %-d %B %Y at %H:%M:%S"),
        now.format("%z")
    )
}

#[async_trait]
impl Tool for TimeNow {
    fn name(&self) -> &str {
        "time.now"
    }

    fn description(&self) -> &str {
        "Return the current local date, time, weekday and timezone offset. \
         Call this whenever the user asks what time or day it is, or before \
         making any statement about 'today'."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let now: DateTime<Local> = Local::now();
        let narrated = format!(
            "{}, {} at {}.",
            now.format("%A"),
            now.format("%-d %B %Y"),
            now.format("%H:%M")
        );
        let detail = json!({
            "weekday": now.format("%A").to_string(),
            "date": now.format("%Y-%m-%d").to_string(),
            "time": now.format("%H:%M:%S").to_string(),
            "iso8601": now.to_rfc3339(),
            "timezone_offset": now.format("%z").to_string(),
        });
        Ok(ToolResult::with_detail(narrated, detail.to_string()))
    }
}

#[async_trait]
impl Tool for TimeUntil {
    fn name(&self) -> &str {
        "time.until"
    }

    fn description(&self) -> &str {
        "Report how long remains until a target moment. Accepts an ISO-8601 \
         timestamp, a 24-hour 'HH:MM' (today — rolls to tomorrow if already \
         past), or 'tomorrow HH:MM'. Use this to phrase reminders; do not \
         use it as a scheduler — it does not block or fire events."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Target moment. Examples: '07:30', 'tomorrow 09:00', '2026-04-20T18:00:00+03:00'."
                }
            },
            "required": ["target"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let target = args
            .get("target")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::invalid_args(self.name(), "'target' is required and must be non-empty")
            })?;

        let target_dt = parse_target(target)
            .ok_or_else(|| ToolError::invalid_args(self.name(), format!("could not parse '{}'", target)))?;

        let now = Local::now();
        let delta = target_dt.signed_duration_since(now);
        let narrated = if delta <= Duration::zero() {
            let past = -delta;
            format!("{} is already in the past ({} ago).", target, format_duration(past))
        } else {
            format!(
                "{} in {} (at {}).",
                narrate_future(target),
                format_duration(delta),
                target_dt.format("%A %H:%M")
            )
        };
        Ok(ToolResult::new(narrated))
    }
}

fn parse_target(input: &str) -> Option<DateTime<Local>> {
    let trimmed = input.trim();

    // ISO-8601 with timezone — the unambiguous case.
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Local));
    }

    // "tomorrow HH:MM" — common phrasing the LLM is likely to emit.
    if let Some(rest) = trimmed.strip_prefix("tomorrow ").or_else(|| trimmed.strip_prefix("yarın ")) {
        let time = NaiveTime::parse_from_str(rest.trim(), "%H:%M").ok()?;
        let now = Local::now();
        let date = now.date_naive().succ_opt()?;
        return Local
            .with_ymd_and_hms(
                date.year(),
                date.month(),
                date.day(),
                time.hour(),
                time.minute(),
                0,
            )
            .single();
    }

    // Bare "HH:MM" — today, or tomorrow if already past.
    if let Ok(time) = NaiveTime::parse_from_str(trimmed, "%H:%M") {
        let now = Local::now();
        let today = Local
            .with_ymd_and_hms(
                now.year(),
                now.month(),
                now.day(),
                time.hour(),
                time.minute(),
                0,
            )
            .single()?;
        if today > now {
            return Some(today);
        }
        return Some(today + Duration::days(1));
    }

    None
}

fn narrate_future(target: &str) -> String {
    // For readability the narration repeats what the caller asked for.
    format!("Target '{}'", target)
}

fn format_duration(delta: Duration) -> String {
    let total_seconds = delta.num_seconds().max(0);
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!("{}m", minutes));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_with_offset() {
        let parsed = parse_target("2026-04-20T18:00:00+03:00").expect("valid rfc3339");
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2026-04-20");
    }

    #[test]
    fn parses_bare_hhmm_future() {
        // Pick a value that is almost always future-of-midnight so the
        // result is not sensitive to when CI happens to run.
        let now = Local::now();
        let target = (now + Duration::minutes(3)).format("%H:%M").to_string();
        let parsed = parse_target(&target).expect("parsed");
        assert!(parsed > now);
    }

    #[test]
    fn format_duration_reports_compact_units() {
        assert_eq!(format_duration(Duration::seconds(59)), "0m");
        assert_eq!(format_duration(Duration::minutes(5)), "5m");
        assert_eq!(format_duration(Duration::hours(2) + Duration::minutes(15)), "2h 15m");
        assert_eq!(format_duration(Duration::days(1) + Duration::hours(3)), "1d 3h");
    }

    #[test]
    fn current_prompt_stamp_mentions_ground_truth() {
        let stamp = current_prompt_stamp();
        assert!(stamp.contains("ground truth"));
        assert!(stamp.contains("Current local time"));
    }
}
