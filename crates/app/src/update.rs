//! Self-update, wired to this repository's GitHub releases.
//!
//! guise owns the whole feature — the release feed, the in-place install, the
//! prompt window. This file only says which repository to watch and what a
//! genuine Tables build is signed with.
//!
//! The codesign requirement is what makes an unattended install safe: without
//! one, guise refuses to execute a downloaded bundle and opens the release page
//! instead. It pins the Developer ID team the release workflow notarizes under,
//! so a DMG served from anywhere else fails the check rather than running.

use gpui::App;
use guise::update::{self, Updater};

/// Where releases come from. `owner/repo`, matching `release.yml`.
const REPO: &str = "wess/tables";

/// The Developer ID team the release workflow signs and notarizes under.
/// A bundle that does not satisfy this is never executed.
const TEAM_ID: &str = "XJDC46F35X";

fn updater() -> Updater {
    Updater::github("Tables", env!("CARGO_PKG_VERSION"), REPO).codesign_requirement(format!(
        "anchor apple generic and certificate leaf[subject.OU] = {TEAM_ID}"
    ))
}

/// Start the launch-and-hourly check. Call once, behind the user's preference —
/// guise itself guards against being started twice.
pub fn start(cx: &mut App) {
    update::start(updater(), cx);
}

/// Help → Check for Updates…. Always answers: the prompt when there is
/// something to install, a short notice saying why not when there isn't.
pub fn check_now(cx: &mut App) {
    update::check_now(updater(), cx);
}
