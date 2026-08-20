use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const HOUR_SECONDS: f64 = 60.0 * 60.0;
pub const DAY_SECONDS: f64 = 24.0 * HOUR_SECONDS;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Serialize)]
pub struct QuotaUsage {
    pub weekly: Option<i64>,
    pub monthly: Option<i64>,
    pub extra: Option<i64>,
}

pub fn classify_window(limit_window_seconds: Option<f64>) -> &'static str {
    let Some(seconds) = limit_window_seconds else {
        return "unknown";
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        return "unknown";
    }
    if seconds >= 25.0 * DAY_SECONDS {
        return "monthly";
    }
    if seconds >= 6.0 * DAY_SECONDS {
        return "weekly";
    }
    if seconds <= 6.0 * HOUR_SECONDS {
        return "hourly";
    }
    "custom"
}

pub fn parse_quota(raw: &Value) -> QuotaUsage {
    let Some(root) = raw.as_object() else {
        return QuotaUsage::default();
    };

    let primary = get_object(root, &["rate_limit", "rateLimit"])
        .and_then(|rate_limit| get_object(rate_limit, &["primary_window", "primaryWindow"]));
    let extra = get_extra(root);

    let Some(primary) = primary else {
        return QuotaUsage {
            extra,
            ..QuotaUsage::default()
        };
    };

    let used_percent = get_number(primary, &["used_percent", "usedPercent"]);
    let used = used_percent.map(normalize_percent);
    let period = classify_window(get_number(
        primary,
        &["limit_window_seconds", "limitWindowSeconds"],
    ));

    QuotaUsage {
        weekly: (period == "weekly").then_some(used).flatten(),
        monthly: (period == "monthly").then_some(used).flatten(),
        extra,
    }
}

fn get_extra(root: &Map<String, Value>) -> Option<i64> {
    let items = get_value(root, &["additional_rate_limits", "additionalRateLimits"])?.as_array()?;

    items.iter().find_map(|item| {
        let object = item.as_object()?;
        let rate_limit = get_object(object, &["rate_limit", "rateLimit"])?;
        let primary = get_object(rate_limit, &["primary_window", "primaryWindow"])?;
        get_number(primary, &["used_percent", "usedPercent"]).map(normalize_percent)
    })
}

fn get_object<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    get_value(object, keys)?.as_object()
}

fn get_value<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn get_number(object: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    get_value(object, keys)?.as_f64()
}

fn normalize_percent(used_percent: f64) -> i64 {
    clamp_percent(round_half_even(used_percent))
}

fn clamp_percent(value: i64) -> i64 {
    value.clamp(0, 100)
}

fn round_half_even(value: f64) -> i64 {
    let lower = value.floor();
    let fraction = value - lower;
    let rounded = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else if (lower as i64) % 2 == 0 {
        lower
    } else {
        lower + 1.0
    };
    rounded as i64
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{classify_window, parse_quota};

    #[test]
    fn parses_weekly_and_extra_quota() {
        let result = parse_quota(&json!({
            "rate_limit": {"primary_window": {"used_percent": 28, "limit_window_seconds": 604800}},
            "additional_rate_limits": [{"rate_limit": {"primary_window": {"used_percent": 17}}}]
        }));
        assert_eq!(result.weekly, Some(28));
        assert_eq!(result.monthly, None);
        assert_eq!(result.extra, Some(17));
    }

    #[test]
    fn parses_monthly_camel_case_quota() {
        let result = parse_quota(&json!({
            "rateLimit": {"primaryWindow": {"usedPercent": 22, "limitWindowSeconds": 2592000}},
            "additionalRateLimits": null
        }));
        assert_eq!(result.weekly, None);
        assert_eq!(result.monthly, Some(22));
        assert_eq!(result.extra, None);
    }

    #[test]
    fn handles_missing_and_invalid_values() {
        assert_eq!(parse_quota(&json!({})), Default::default());
        assert_eq!(
            parse_quota(&json!({"rate_limit":{"primary_window":{}}})),
            Default::default()
        );
        assert_eq!(parse_quota(&json!(null)), Default::default());
        let result = parse_quota(&json!({
            "rate_limit": {"primary_window": {"used_percent": 140, "limit_window_seconds": 604800}}
        }));
        assert_eq!(result.weekly, Some(100));
    }

    #[test]
    fn classifies_windows_by_duration() {
        assert_eq!(classify_window(Some(2_592_000.0)), "monthly");
        assert_eq!(classify_window(Some(604_800.0)), "weekly");
        assert_eq!(classify_window(Some(21_600.0)), "hourly");
        assert_eq!(classify_window(Some(172_800.0)), "custom");
        assert_eq!(classify_window(None), "unknown");
    }

    #[test]
    fn matches_python_round_half_even_boundaries() {
        let lower = parse_quota(&json!({
            "rate_limit": {"primary_window": {"used_percent": 28.5, "limit_window_seconds": 604800}}
        }));
        let upper = parse_quota(&json!({
            "rate_limit": {"primary_window": {"used_percent": 27.5, "limit_window_seconds": 604800}}
        }));
        assert_eq!(lower.weekly, Some(28));
        assert_eq!(upper.weekly, Some(28));
    }
}
