//! `maestro` — lets an agent running inside Maestro drive Maestro (worktrees, other agents).
//! Maestro starts each agent with MAESTRO_URL, MAESTRO_TOKEN and MAESTRO_WORKTREE and puts this
//! binary on its PATH as `maestro`. Talks plain HTTP to Maestro's local server; std only.
//!
//!   maestro <words…> [--flag value…]   →  POST /api/<words joined by '/'>  {"flag": "value", …}

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;

/// ["worktree", "create", "--name", "x", "--prompt", "y"] → ("worktree/create", {"name":"x","prompt":"y"})
fn parse(args: &[String]) -> Result<(String, serde_json::Map<String, serde_json::Value>), String> {
    let mut words = vec![];
    let mut flags = serde_json::Map::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(k) = a.strip_prefix("--") {
            let v = it.next().ok_or(format!("--{k} 에 값이 필요해요"))?;
            flags.insert(k.into(), v.clone().into());
        } else {
            words.push(a.as_str());
        }
    }
    Ok((words.join("/"), flags))
}

fn call(route: &str, body: &str) -> Result<(u16, String), String> {
    let env = |k: &str| std::env::var(k).map_err(|_| format!("{k} 가 없어요. Maestro가 띄운 에이전트 터미널에서 실행하세요."));
    let (addr, token) = (env("MAESTRO_URL")?, env("MAESTRO_TOKEN")?);
    let caller = std::env::var("MAESTRO_WORKTREE").unwrap_or_default();
    let mut s = TcpStream::connect(&addr).map_err(|e| format!("Maestro에 연결하지 못했어요 ({addr}): {e}"))?;
    let req = format!(
        "POST /api/{route} HTTP/1.1\r\nHost: {addr}\r\nX-Maestro-Token: {token}\r\nX-Maestro-Worktree: {caller}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    let status = resp.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let body = resp.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
    Ok((status, body))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let help = args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h" || a == "help");
    let (route, flags) = if help { ("help".to_string(), Default::default()) } else {
        match parse(&args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        }
    };
    match call(&route, &serde_json::Value::Object(flags).to_string()) {
        Ok((200, body)) => {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(body.into());
            if help {
                println!("maestro — Maestro를 조작하는 명령 (워크트리, 다른 에이전트)\n");
                for c in v.as_array().into_iter().flatten() {
                    println!("  maestro {:<18} {}", c["command"].as_str().unwrap_or(""), c["usage"].as_str().unwrap_or(""));
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                // done, with something to know (e.g. a worker made but its first prompt not delivered)
                if let Some(w) = v["warning"].as_str() {
                    eprintln!("주의: {w}");
                }
            }
            ExitCode::SUCCESS
        }
        Ok((_, body)) => {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            eprintln!("오류: {}", v["error"].as_str().unwrap_or(&body));
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_become_route_and_flags_become_body() {
        let a: Vec<String> = ["worktree", "create", "--name", "리뷰", "--prompt", "봐줘"].iter().map(|s| s.to_string()).collect();
        let (route, flags) = parse(&a).unwrap();
        assert_eq!(route, "worktree/create");
        assert_eq!(flags["name"], "리뷰");
        assert_eq!(flags["prompt"], "봐줘");
        assert!(parse(&["send".into(), "--to".into()]).is_err());
        // the worker as a word: it joins the route (a branch like feat/x stays whole on the server)
        let a: Vec<String> = ["merge-request", "api", "--verified", "ok"].iter().map(|s| s.to_string()).collect();
        let (route, flags) = parse(&a).unwrap();
        assert_eq!(route, "merge-request/api");
        assert_eq!(flags["verified"], "ok");
    }
}
