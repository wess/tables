use std::process::Command;

use guise::update::{is_newer, InstallKind, Release, ReleaseAsset, UpdateCheck, UpdateConfig};
use serde_json::Value;

pub fn check(config: &UpdateConfig) -> Result<UpdateCheck, String> {
  let output = Command::new("curl")
    .args([
      "-fsSL",
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--connect-timeout",
      "10",
      "--max-time",
      "30",
      "--max-filesize",
      "1048576",
      "-H",
      "Accept: application/vnd.github+json",
      "-H",
      "User-Agent: tables-updater",
      "--",
    ])
    .arg(config.source().endpoint())
    .output()
    .map_err(|e| format!("Update check failed: {e}"))?;
  if !output.status.success() {
    return Err("Could not reach the release feed. Check your connection and try again.".into());
  }
  classify(&output.stdout, config.version(), &config.install_kind())
}

pub fn classify(body: &[u8], current: &str, kind: &InstallKind) -> Result<UpdateCheck, String> {
  if body.len() > 1024 * 1024 {
    return Err("Release feed exceeded 1 MiB".into());
  }
  let feed: Value =
    serde_json::from_slice(body).map_err(|e| format!("Invalid release feed: {e}"))?;
  let tag = feed["tag_name"].as_str().ok_or("Release has no version")?;
  let version = tag.trim_start_matches('v');
  let parts: Vec<_> = version.split('.').collect();
  if parts.len() != 3
    || parts
      .iter()
      .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
  {
    return Err("Release has an invalid version".into());
  }
  if !is_newer(version, current) {
    return Ok(UpdateCheck::UpToDate);
  }
  let url = feed["html_url"]
    .as_str()
    .filter(|u| u.starts_with("https://github.com/wess/tables/releases/"))
    .ok_or("Release has an invalid download page")?;
  let assets = feed["assets"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|asset| {
      let url = asset["browser_download_url"].as_str()?;
      if !url.starts_with("https://github.com/wess/tables/releases/download/") {
        return None;
      }
      Some(ReleaseAsset {
        name: asset["name"].as_str()?.into(),
        url: url.into(),
        size: asset["size"].as_u64().unwrap_or(0),
      })
    })
    .collect();
  let release = Release {
    version: version.into(),
    url: url.into(),
    assets,
  };
  if release.ready_for(kind) {
    Ok(UpdateCheck::Ready(release))
  } else {
    Ok(UpdateCheck::Pending(release.version))
  }
}
