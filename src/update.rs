//! Newer releases on GitHub (the same check as Agent Signals): the release in GitHub's answer when
//! it is newer than this build, with the download for this build.

/// A release newer than this build: its version and its APK (else its release page).
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// The release in GitHub's `releases/latest` answer when it is newer than `current`, with the
/// download whose name ends in `asset`, else its release page. Anything else (an error page, the
/// rate limit, no release yet): `None`.
pub fn newer(json: &str, current: &str, asset: &str) -> Option<Update> {
    let release: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = release.get("tag_name")?.as_str()?;
    if version(tag)? <= version(current)? {
        return None;
    }
    let download = release.get("assets").and_then(|a| a.as_array()).and_then(|assets| {
        assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.ends_with(asset)))
            .and_then(|a| a.get("browser_download_url")?.as_str())
    });
    let url = download.or_else(|| release.get("html_url")?.as_str())?;
    Some(Update { version: tag.trim_start_matches('v').to_owned(), url: url.to_owned() })
}

/// "v0.6.5" or "0.6.5" to (0, 6, 5).
fn version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split(['.', '-']).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_newer_release_and_its_apk() {
        let json = r#"{"tag_name":"v0.2.0","html_url":"https://github.com/x/releases/tag/v0.2.0","assets":[
            {"name":"netconnect-020-arm64.apk","browser_download_url":"https://dl/app.apk"}]}"#;
        assert_eq!(newer(json, "0.1.0", "-arm64.apk"), Some(Update { version: "0.2.0".into(), url: "https://dl/app.apk".into() }));
        assert_eq!(newer(json, "0.1.0", "-other.apk").unwrap().url, "https://github.com/x/releases/tag/v0.2.0");
        assert_eq!(newer(json, "0.2.0", "-arm64.apk"), None);
        assert_eq!(newer(json, "0.10.0", "-arm64.apk"), None);
        assert_eq!(newer(r#"{"message":"Not Found"}"#, "0.1.0", "x"), None);
        assert_eq!(newer("", "0.1.0", "x"), None);
    }
}
