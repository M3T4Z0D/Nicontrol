use std::process::Command;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_PAGE_URL: &str = "https://github.com/M3T4Z0D/Nicontrol/releases/latest";
const LATEST_API_URL: &str = "https://api.github.com/repos/M3T4Z0D/Nicontrol/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_url: String,
}

pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let clean = s.trim().trim_start_matches(['v', 'V']);
    let core = clean.split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let rest = &json[pos + pattern.len()..];
    let colon = rest.find(':')?;
    let after_colon = rest[colon + 1..].trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}

pub fn check_for_update() -> Option<UpdateInfo> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: Nicontrol",
            LATEST_API_URL,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let tag = extract_json_string(&body, "tag_name")?;
    if !is_newer(&tag, CURRENT_VERSION) {
        return None;
    }

    let url = extract_json_string(&body, "html_url")
        .unwrap_or_else(|| RELEASES_PAGE_URL.to_string());

    Some(UpdateInfo {
        latest_version: tag,
        release_url: url,
    })
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison() {
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
    }

    #[test]
    fn json_field_extraction() {
        let sample = r#"{"url":"https://api.github.com","tag_name":"v0.3.0","html_url":"https://github.com/M3T4Z0D/Nicontrol/releases/tag/v0.3.0"}"#;
        assert_eq!(
            extract_json_string(sample, "tag_name").as_deref(),
            Some("v0.3.0")
        );
        assert_eq!(
            extract_json_string(sample, "html_url").as_deref(),
            Some("https://github.com/M3T4Z0D/Nicontrol/releases/tag/v0.3.0")
        );
    }
}
