//! 사용량: how much of each subscription's rate limits is used (docs/features/ui/usage.md).
//!
//! Claude: Claude Code's own `/usage` source, `GET https://api.anthropic.com/api/oauth/usage` with the OAuth
//! access token from `.credentials.json` (read only; an expired token is reported, never refreshed — refreshing
//! would rewrite the user's login). The token goes to curl on stdin (`-H @-`), never on its command line.
//! Codex: the `rate_limits` the CLI writes into its session logs (`~/.codex/sessions/…/rollout-*.jsonl`) with
//! every `token_count` event — no network; as fresh as the last Codex turn.

use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One limit window: how full it is and when it empties.
#[derive(Serialize, Debug, PartialEq)]
pub struct Window {
    /// "5시간", "7일", "7일 · Opus"
    pub label: String,
    /// 0–100
    pub percent: f64,
    /// RFC 3339 (Claude)
    pub resets_at: Option<String>,
    /// unix seconds (Codex)
    pub resets_unix: Option<i64>,
}

/// One CLI's windows, or why there are none.
#[derive(Serialize, Debug, Default)]
pub struct Source {
    pub windows: Vec<Window>,
    /// shown instead of the windows
    pub note: Option<String>,
    /// Codex: when the numbers were written (RFC 3339), they are as old as the last turn
    pub as_of: Option<String>,
    /// Claude: the subscription ("Max", "Pro"), from the login file
    pub plan: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Report {
    pub claude: Source,
    pub codex: Source,
}

pub fn report(home: &Path) -> Report {
    Report { claude: claude(home), codex: codex(home) }
}

fn note(s: &str) -> Source {
    Source { note: Some(s.into()), ..Default::default() }
}

fn claude(home: &Path) -> Source {
    let path = crate::claude::config_dir(home).join(".credentials.json");
    let Some(auth) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return note("Claude 구독 로그인 정보가 없어요 (API 키로 쓰거나 로그인 전이에요)");
    };
    let oauth = &auth["claudeAiOauth"];
    let Some(token) = oauth["accessToken"].as_str().filter(|t| !t.is_empty()) else {
        return note("Claude 구독 로그인 정보가 없어요");
    };
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    if oauth["expiresAt"].as_i64().is_some_and(|e| e <= now_ms) {
        return note("로그인 토큰이 만료됐어요. Claude 에이전트를 한 번 실행하면 갱신돼요");
    }
    let plan = oauth["subscriptionType"].as_str().map(plan_label);
    let source = match fetch_claude(token) {
        Ok(body) => claude_windows(&body),
        Err(e) => note(&format!("사용량을 불러오지 못했어요 · {e}")),
    };
    Source { plan, ..source }
}

/// "max" → "Max", "pro" → "Pro"; anything else as it is.
fn plan_label(s: &str) -> String {
    match s.to_ascii_lowercase().as_str() {
        "pro" => "Pro".into(),
        "max" => "Max".into(),
        "team" => "Team".into(),
        "enterprise" => "Enterprise".into(),
        "free" => "Free".into(),
        _ => s.into(),
    }
}

fn fetch_claude(token: &str) -> Result<Value, String> {
    let mut cmd = Command::new("curl");
    crate::cli::quiet(&mut cmd)
        .args(["-s", "-m", "10", "-H", "@-", "https://api.anthropic.com/api/oauth/usage"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|_| "curl을 찾지 못했어요".to_string())?;
    let headers = format!("Authorization: Bearer {token}\nanthropic-beta: oauth-2025-04-20\nContent-Type: application/json\n");
    child.stdin.take().ok_or("curl")?.write_all(headers.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("네트워크 오류".into());
    }
    serde_json::from_slice(&out.stdout).map_err(|_| "응답을 읽지 못했어요".into())
}

/// The response's windows, in this order; any that is null (no such limit on the plan) is left out.
fn claude_windows(body: &Value) -> Source {
    if let Some(msg) = body["error"]["message"].as_str() {
        return note(&format!("사용량을 불러오지 못했어요 · {msg}"));
    }
    let window = |label: String, w: &Value| Some(Window { label, percent: w["utilization"].as_f64()?, resets_at: w["resets_at"].as_str().map(String::from), resets_unix: None });
    let mut windows: Vec<Window> = [("five_hour", "5시간"), ("seven_day", "7일")].iter().filter_map(|(k, label)| window(label.to_string(), &body[*k])).collect();
    // per-model weekly limits: `model_scoped` [{display_name, utilization, resets_at}] when present, else the older keys
    match body["model_scoped"].as_array() {
        Some(models) => windows.extend(models.iter().filter_map(|m| window(format!("7일 · {}", m["display_name"].as_str()?), m))),
        None => windows.extend([("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")].iter().filter_map(|(k, name)| window(format!("7일 · {name}"), &body[*k]))),
    }
    if windows.is_empty() {
        return note("이 계정의 사용량 정보가 없어요");
    }
    Source { windows, ..Default::default() }
}

fn codex(home: &Path) -> Source {
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex")).join("sessions");
    // newest first: sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl sort by name
    let mut files = Vec::new();
    for year in newest(&dir, 2) {
        for month in newest(&year, 12) {
            for day in newest(&month, 31) {
                files.extend(newest(&day, 50));
                if files.len() >= 30 {
                    break;
                }
            }
        }
    }
    for f in files.iter().take(30) {
        if let Some(found) = last_rate_limits(f) {
            return found;
        }
    }
    note("Codex 기록에 사용량이 아직 없어요. Codex로 한 번 대화하면 보여요")
}

/// The entries of `dir`, newest name first, at most `n`.
fn newest(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v.reverse();
    v.truncate(n);
    v
}

/// The last non-null `rate_limits` in a session log.
fn last_rate_limits(file: &Path) -> Option<Source> {
    let reader = BufReader::new(std::fs::File::open(file).ok()?);
    let mut last = None;
    for line in reader.lines().map_while(Result::ok) {
        if line.contains("\"rate_limits\":{") {
            last = Some(line);
        }
    }
    codex_windows(&serde_json::from_str(&last?).ok()?)
}

fn codex_windows(line: &Value) -> Option<Source> {
    let limits = &line["payload"]["rate_limits"];
    let at = line["timestamp"].as_str();
    let at_unix = at.and_then(parse_rfc3339);
    let windows: Vec<Window> = ["primary", "secondary"]
        .iter()
        .filter_map(|k| {
            let w = &limits[*k];
            let percent = w["used_percent"].as_f64()?;
            let label = match w["window_minutes"].as_i64() {
                Some(300) => "5시간".to_string(),
                Some(10080) => "7일".to_string(),
                Some(m) if m % 1440 == 0 => format!("{}일", m / 1440),
                Some(m) if m % 60 == 0 => format!("{}시간", m / 60),
                Some(m) => format!("{m}분"),
                None => k.to_string(),
            };
            let resets_unix = w["resets_at"].as_i64().or_else(|| Some(at_unix? + w["resets_in_seconds"].as_i64()?));
            Some(Window { label, percent, resets_at: None, resets_unix })
        })
        .collect();
    (!windows.is_empty()).then(|| Source { windows, note: None, as_of: at.map(String::from), plan: None })
}

/// "2026-11-17T01:15:58.123Z" → unix seconds (UTC only, which is what Codex writes).
fn parse_rfc3339(s: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    // days from civil (Howard Hinnant)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_keeps_the_windows_the_plan_has() {
        let body = json!({"five_hour": {"utilization": 12.0, "resets_at": "2026-11-18T10:00:00Z"}, "seven_day": {"utilization": 40.5, "resets_at": null}, "seven_day_opus": null});
        let s = claude_windows(&body);
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.windows[0], Window { label: "5시간".into(), percent: 12.0, resets_at: Some("2026-11-18T10:00:00Z".into()), resets_unix: None });
        assert!(claude_windows(&json!({"error": {"message": "invalid token"}})).note.unwrap().contains("invalid token"));
        assert!(claude_windows(&json!({})).note.is_some());
        let scoped = claude_windows(&json!({"five_hour": {"utilization": 1.0}, "model_scoped": [{"display_name": "Opus", "utilization": 9.0, "resets_at": null}]}));
        assert_eq!(scoped.windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["5시간", "7일 · Opus"]);
        assert_eq!(plan_label("max"), "Max");
    }

    #[test]
    fn codex_reads_both_reset_forms() {
        let line = json!({"timestamp": "2026-11-17T01:00:00.000Z", "payload": {"type": "token_count", "rate_limits": {
            "primary": {"used_percent": 25.0, "window_minutes": 300, "resets_in_seconds": 600},
            "secondary": {"used_percent": 3.0, "window_minutes": 10080, "resets_at": 1800000000}}}});
        let s = codex_windows(&line).unwrap();
        assert_eq!(s.windows[0].label, "5시간");
        assert_eq!(s.windows[0].resets_unix, Some(parse_rfc3339("2026-11-17T01:10:00Z").unwrap()));
        assert_eq!(s.windows[1].label, "7일");
        assert_eq!(s.windows[1].resets_unix, Some(1800000000));
        assert!(codex_windows(&json!({"payload": {"rate_limits": null}})).is_none());
    }

    #[test]
    fn rfc3339_to_unix() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-11-17T01:15:58.123Z"), Some(1794878158));
    }
}
