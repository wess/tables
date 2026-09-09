//! Guise installs and prompts; Tables schedules bounded release checks and applies preferences.

mod check;

use gpui::App;
use guise::update::{self, Updater};

/// Where releases come from. `owner/repo`, matching `release.yml`.
const REPO: &str = "wess/tables";

/// The Developer ID team the release workflow signs and notarizes under.
/// A bundle that does not satisfy this is never executed.
const TEAM_ID: &str = "XJDC46F35X";

fn updater() -> Updater {
    Updater::github("Tables", env!("CARGO_PKG_VERSION"), REPO)
        .codesign_requirement(format!(
            "anchor apple generic and certificate leaf[subject.OU] = {TEAM_ID}"
        ))
        .require_checksum(true)
}

#[derive(Default)]
struct Checks {
    automatic: Option<gpui::Task<()>>,
    manual: Option<gpui::Task<()>>,
    checking: bool,
    notified: String,
}
impl gpui::Global for Checks {}

pub fn configure(enabled: bool, cx: &mut App) {
    if cx.try_global::<Checks>().is_none() {
        cx.set_global(Checks::default());
    }
    if !enabled {
        cx.global_mut::<Checks>().automatic = None;
        return;
    }
    if cx.global::<Checks>().automatic.is_some() {
        return;
    }
    let executor = cx.background_executor().clone();
    let task = cx.spawn(async move |cx| loop {
        let config = updater().config().clone();
        let result = executor.spawn(async move { check::check(&config) }).await;
        let _ = cx.update(|cx| {
            if let Ok(update::UpdateCheck::Ready(release)) = result {
                if !update::is_installing(cx) && cx.global::<Checks>().notified != release.version {
                    cx.global_mut::<Checks>().notified = release.version.clone();
                    update::open(updater(), release, cx);
                }
            }
        });
        executor.timer(update::POLL).await;
    });
    cx.global_mut::<Checks>().automatic = Some(task);
}

pub fn checking(cx: &App) -> bool {
    cx.try_global::<Checks>()
        .is_some_and(|checks| checks.checking)
}

pub fn check_now(cx: &mut App) {
    if cx.try_global::<Checks>().is_none() {
        cx.set_global(Checks::default());
    }
    if cx.global::<Checks>().checking || update::is_installing(cx) {
        return;
    }
    cx.global_mut::<Checks>().checking = true;
    cx.refresh_windows();
    let executor = cx.background_executor().clone();
    let task = cx.spawn(async move |cx| {
        let config = updater().config().clone();
        let result = executor.spawn(async move { check::check(&config) }).await;
        let _ = cx.update(|cx| {
            cx.global_mut::<Checks>().checking = false;
            cx.refresh_windows();
            match result {
                Ok(update::UpdateCheck::Ready(release)) => {
                    if !update::is_installing(cx) {
                        cx.global_mut::<Checks>().notified = release.version.clone();
                        update::open(updater(), release, cx);
                    }
                }
                other => {
                    let outcome = match other {
                        Ok(update::UpdateCheck::Pending(version)) => {
                            update::UpdateOutcome::Pending(version)
                        }
                        Ok(_) => update::UpdateOutcome::UpToDate,
                        Err(error) => update::UpdateOutcome::Failed(error),
                    };
                    update::open_notice(updater(), outcome, cx);
                }
            }
        });
    });
    cx.global_mut::<Checks>().manual = Some(task);
}
