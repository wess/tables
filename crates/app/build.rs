//! Build metadata for the About card.
//!
//! Two facts the binary cannot work out at runtime: the day it was built, and
//! whether it is *the* build of its version or just some checkout carrying that
//! number. `guise::BuildKind` exists for the second one, and printing "Released
//! 2026-08-21" on a developer's local build is the small lie it prevents.
//!
//! The release workflow sets `TABLES_RELEASE=1`. Nothing else does, so every
//! other build — CI, a local `cargo run`, a distro rebuild — says what it is.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=TABLES_RELEASE");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    println!("cargo:rustc-env=BUILD_DATE={}", build_date());

    let released = std::env::var("TABLES_RELEASE").is_ok_and(|v| v == "1");
    println!("cargo:rustc-env=BUILD_KIND={}", if released { "released" } else { "development" });
}

/// The build date as `YYYY-MM-DD`, or `unknown` when there is nothing to ask.
///
/// `SOURCE_DATE_EPOCH` first, so a reproducible build stamps the source date
/// rather than the day the rebuild happened. `date` is the fallback; if that is
/// missing too the About card reads "Development build" with no date, which is
/// the honest answer rather than a wrong one.
fn build_date() -> String {
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = epoch.parse::<i64>() {
            if let Some(date) = utc_date(secs) {
                return date;
            }
        }
    }

    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Civil date from a Unix timestamp, by Howard Hinnant's `civil_from_days`.
/// Only whole days matter here, so this needs no timezone database.
fn utc_date(secs: i64) -> Option<String> {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}
