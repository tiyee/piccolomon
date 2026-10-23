//! Explicit, bounded release checks; never downloads or installs an application.
use ryne_extensions::{UpdateChecker, UpdateStatus};
use std::{io::Read, time::Duration};

pub struct GithubUpdates;
const REPOSITORY: &str = "https://github.com/tiyee/ryne";
impl UpdateChecker for GithubUpdates {
    fn check(&self, current: &str) -> Result<UpdateStatus, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        let mut response = match agent
            .get("https://api.github.com/repos/tiyee/ryne/releases/latest")
            .header("User-Agent", "Ryne")
            .header("Accept", "application/vnd.github+json")
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(404)) => return Ok(UpdateStatus::NoRelease),
            Err(error) => return Err(error.to_string()),
        };
        let mut body = String::new();
        response
            .body_mut()
            .as_reader()
            .take(256 * 1024 + 1)
            .read_to_string(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() > 256 * 1024 {
            return Err("Release response exceeds 256 KiB".into());
        }
        parse_release(&body, current)
    }
}
fn parse_release(body: &str, current: &str) -> Result<UpdateStatus, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if value["draft"].as_bool() != Some(false) || value["prerelease"].as_bool() != Some(false) {
        return Err("Expected a published stable release".into());
    }
    let tag = value["tag_name"].as_str().ok_or("Missing release tag")?;
    let version = semver::Version::parse(tag.trim_start_matches('v')).map_err(|e| e.to_string())?;
    let installed = semver::Version::parse(current).map_err(|e| e.to_string())?;
    if version <= installed {
        return Ok(UpdateStatus::Current);
    }
    // Build a trusted repository URL instead of opening arbitrary response data.
    Ok(UpdateStatus::Available {
        version: version.to_string(),
        url: format!("{REPOSITORY}/releases/latest"),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_versions_and_rejects_invalid_release_data() {
        let release = |tag| format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":false}}"#);
        assert_eq!(
            parse_release(&release("v0.1.0"), "0.1.0").unwrap(),
            UpdateStatus::Current
        );
        assert_eq!(
            parse_release(&release("v0.0.9"), "0.1.0").unwrap(),
            UpdateStatus::Current
        );
        assert!(matches!(
            parse_release(&release("v0.2.0"), "0.1.0").unwrap(),
            UpdateStatus::Available { .. }
        ));
        assert!(parse_release(&release("unknown"), "0.1.0").is_err());
        assert!(parse_release(
            r#"{"tag_name":"v9.0.0","draft":true,"prerelease":false}"#,
            "0.1.0"
        )
        .is_err());
        assert!(parse_release("{}", "0.1.0").is_err());
    }
}
