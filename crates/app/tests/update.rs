#[path = "../src/update/check.rs"]
#[allow(dead_code)]
mod check;

use guise::update::{InstallKind, UpdateCheck};
use serde_json::json;

#[test]
fn release_waits_for_platform_asset_and_rejects_unsafe_feed() {
  let mut feed = json!({"tag_name":"v2.0.0", "html_url":"https://github.com/wess/tables/releases/tag/v2.0.0", "assets":[]});
  let kind = InstallKind::MacApp("/Applications/Tables.app".into());
  assert_eq!(
    check::classify(&serde_json::to_vec(&feed).unwrap(), "1.0.0", &kind).unwrap(),
    UpdateCheck::Pending("2.0.0".into())
  );
  feed["assets"] = json!([{"name":"Tables.dmg", "browser_download_url":"https://github.com/wess/tables/releases/download/v2.0.0/Tables.dmg", "size":100}]);
  assert!(matches!(
    check::classify(&serde_json::to_vec(&feed).unwrap(), "1.0.0", &kind).unwrap(),
    UpdateCheck::Ready(_)
  ));
  assert_eq!(
    check::classify(&serde_json::to_vec(&feed).unwrap(), "2.0.0", &kind).unwrap(),
    UpdateCheck::UpToDate
  );
  feed["tag_name"] = json!("v2.0.0/../../escape");
  assert!(check::classify(&serde_json::to_vec(&feed).unwrap(), "1.0.0", &kind).is_err());
  feed["tag_name"] = json!("v2.0.0");
  feed["html_url"] = json!("file:///tmp/installer");
  assert!(check::classify(&serde_json::to_vec(&feed).unwrap(), "1.0.0", &kind).is_err());
}
