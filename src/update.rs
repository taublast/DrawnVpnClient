//! Newer releases on GitHub: one repository can hold releases of several platforms, so each app
//! takes the newest release that carries its own file and ignores the others.

/// A release newer than this build: its version and its file for this build.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// From GitHub's `releases` list: the newest published release newer than `current` with a
/// download whose name ends in `asset`. Anything else (an error page, the rate limit, no such
/// release): `None`.
pub fn newer(json: &str, current: &str, asset: &str) -> Option<Update> {
    let releases: serde_json::Value = serde_json::from_str(json).ok()?;
    let current = version(current)?;
    releases
        .as_array()?
        .iter()
        .filter(|r| !r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false))
        .filter(|r| !r.get("prerelease").and_then(|d| d.as_bool()).unwrap_or(false))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            let v = version(tag)?;
            let url = r.get("assets")?.as_array()?.iter().find_map(|a| {
                let name = a.get("name")?.as_str()?;
                if name.ends_with(asset) { a.get("browser_download_url")?.as_str() } else { None }
            })?;
            (v > current).then(|| (v, Update { version: tag.trim_start_matches('v').to_owned(), url: url.to_owned() }))
        })
        .max_by_key(|(v, _)| *v)
        .map(|(_, update)| update)
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
    fn takes_the_newest_release_with_this_builds_file() {
        // Newest first, as GitHub lists them: a Windows-only release, then one with the APKs.
        let json = r#"[
            {"tag_name":"v0.3.0","assets":[{"name":"netconnect-030-win-x64.exe","browser_download_url":"https://dl/030.exe"}]},
            {"tag_name":"v0.2.0","assets":[
                {"name":"netconnect-020-arm64.apk","browser_download_url":"https://dl/020-arm64.apk"},
                {"name":"netconnect-020-armv7.apk","browser_download_url":"https://dl/020-armv7.apk"}]},
            {"tag_name":"v0.4.0","draft":true,"assets":[{"name":"netconnect-040-arm64.apk","browser_download_url":"https://dl/040.apk"}]}
        ]"#;
        assert_eq!(newer(json, "0.1.0", "-arm64.apk"), Some(Update { version: "0.2.0".into(), url: "https://dl/020-arm64.apk".into() }));
        assert_eq!(newer(json, "0.1.0", "-armv7.apk").unwrap().url, "https://dl/020-armv7.apk");
        assert_eq!(newer(json, "0.1.0", "-win-x64.exe").unwrap().version, "0.3.0");
        assert_eq!(newer(json, "0.2.0", "-arm64.apk"), None);
        assert_eq!(newer(r#"{"message":"Not Found"}"#, "0.1.0", "x"), None);
        assert_eq!(newer("", "0.1.0", "x"), None);
    }
}
