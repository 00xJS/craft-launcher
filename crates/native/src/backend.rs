//! [`NativeBackend`]: the desktop implementation of the UI's [`Backend`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use artcraft_launcher_core::catalog::{LAUNCHER, LAUNCHER_ID};
use artcraft_launcher_core::feed::{Feed, FeedAnswer};
use artcraft_launcher_core::platform::{Arch, Os, Platform, pick_asset};
use artcraft_launcher_core::protocol::{
    Backend, BackendOptions, Capabilities, Command, Event, JobPhase, LatestRelease, Origin, PAUSED, PausedDownload, Source,
};
use artcraft_launcher_core::release::{Asset, Release};
use artcraft_launcher_core::{CraftApp, Installed, PackageKind, SUITE, version};

use crate::http::{self, Http, Stop};
use crate::install::{self, Job};
use crate::manifest::Manifest;
use crate::paths::Paths;
use crate::source::{GitHubSource, ReleaseSource};

/// Automatic checks closer together than this reuse the last answers. The UI asks every five
/// minutes; this stops a burst of requests (refresh, focus, timers) from repeating work.
const FRESH_FOR: i64 = 4 * 60;
/// Once every app has loaded, GitHub is asked about an app the feed can't vouch for at most this
/// often (pressing Refresh asks right away).
const GITHUB_FALLBACK_EVERY: i64 = 60 * 60;
/// Until every app has loaded once, GitHub may be asked a little more eagerly.
const EARLY_GITHUB_EVERY: i64 = 5 * 60;
/// Tries at the feed per check (after the very first poll), and the waits between them.
const FEED_ATTEMPTS: usize = 3;
const FEED_RETRY_WAITS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];
/// After everything has loaded, the feed counts as seriously failed (and GitHub takes over)
/// only after this many checks in a row without an answer…
const SERIOUS_FEED_FAILURES: u32 = 3;
/// …and this long without a good answer.
const SERIOUS_FEED_OUTAGE: i64 = 60 * 60;
/// Requests in flight at once during a check.
const WORKERS: usize = 4;

type Waker = Arc<dyn Fn() + Send + Sync>;

struct Shared {
    events: Mutex<Vec<Event>>,
    waker: Waker,
    latest: Mutex<HashMap<String, Release>>,
    installed: Mutex<HashMap<String, Installed>>,
    jobs: Mutex<HashMap<String, Arc<Stop>>>,
    manifest: Mutex<Manifest>,
    options: Mutex<BackendOptions>,
    checking: AtomicBool,
    /// A detection pass is running; another request waits for the next one.
    detecting: AtomicBool,
    last_check: Mutex<Option<i64>>,
    /// The newer ArtCraft Launcher release, once a check has found one.
    launcher_release: Mutex<Option<Release>>,
    /// When GitHub was last asked about each app (Unix seconds), to keep the fallback rare.
    github_asked: Mutex<HashMap<String, i64>>,
    /// Apps with an answer (a release or "none yet") from either service.
    answered: Mutex<std::collections::HashSet<String>>,
    /// Every app has had an answer at least once: from then on GitHub is asked sparingly.
    loaded: AtomicBool,
    /// Checks in a row in which the feed didn't answer.
    feed_failures: Mutex<u32>,
    /// When the feed last answered (Unix seconds).
    feed_ok_at: Mutex<Option<i64>>,
}

impl Shared {
    fn emit(&self, event: Event) {
        self.events.lock().unwrap_or_else(PoisonError::into_inner).push(event);
        (self.waker)();
    }
}

pub struct NativeBackend {
    shared: Arc<Shared>,
    paths: Paths,
    http: Option<Http>,
    source: Option<Arc<dyn ReleaseSource>>,
    platform: Option<Platform>,
    /// How this launcher is installed, when it can replace itself.
    self_install: Option<crate::self_update::Install>,
}

impl NativeBackend {
    /// `waker` is called (from any thread) whenever events are waiting; it should repaint the UI.
    pub fn new(waker: impl Fn() + Send + Sync + 'static) -> Self {
        let waker: Waker = Arc::new(waker);
        let paths = Paths::new().unwrap_or_else(|e| {
            log::error!("{e}; keeping files in the temporary folder");
            Paths::under(&std::env::temp_dir().join("artcraft-launcher"))
        });
        let http = match Http::new() {
            Ok(h) => Some(h),
            Err(e) => {
                log::error!("HTTPS unavailable: {e}");
                None
            }
        };
        let source: Option<Arc<dyn ReleaseSource>> =
            http.clone().map(|h| Arc::new(GitHubSource::new(h, paths.release_cache())) as Arc<dyn ReleaseSource>);
        let shared = Arc::new(Shared {
            events: Mutex::new(Vec::new()),
            waker,
            latest: Mutex::new(HashMap::new()),
            installed: Mutex::new(HashMap::new()),
            jobs: Mutex::new(HashMap::new()),
            manifest: Mutex::new(Manifest::load(&paths.manifest())),
            options: Mutex::new(BackendOptions::default()),
            checking: AtomicBool::new(false),
            detecting: AtomicBool::new(false),
            launcher_release: Mutex::new(None),
            github_asked: Mutex::new(HashMap::new()),
            answered: Mutex::new(std::collections::HashSet::new()),
            loaded: AtomicBool::new(false),
            feed_failures: Mutex::new(0),
            feed_ok_at: Mutex::new(None),
            last_check: Mutex::new(None),
        });
        let backend = Self { shared, paths, http, source, platform: native_platform(), self_install: crate::self_update::current() };
        backend.replay_cache();
        backend.detect_all();
        backend
    }

    /// Shows the saved answers from the last run straight away, before any network request: the
    /// release feed's where they're still fresh, GitHub's otherwise.
    fn replay_cache(&self) {
        let pre = self.options().prereleases;
        let feed = if pre { None } else { load_feed(&self.paths) };
        let now = crate::now();
        for app in &SUITE {
            let answer = match feed.as_ref().map(|f| f.answer(app, now)) {
                Some(FeedAnswer::Fresh(Some(r))) => Some((LatestRelease::Found(r), Origin::ArtCraftApi)),
                Some(FeedAnswer::Fresh(None)) => Some((LatestRelease::None, Origin::ArtCraftApi)),
                _ => self.source.as_ref().and_then(|s| s.cached(&feed_repo(feed.as_ref(), app), pre)).map(|l| (l, Origin::GitHub)),
            };
            if let Some((latest, from)) = answer {
                if let LatestRelease::Found(r) = &latest {
                    self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner).insert(app.id.to_string(), r.clone());
                }
                self.shared.emit(Event::Latest { app: app.id.to_string(), latest, from });
            }
        }
        if let Some(f) = feed {
            self.shared.emit(Event::Notices { announcements: f.announcements, alerts: f.alerts });
        }
    }

    fn options(&self) -> BackendOptions {
        self.shared.options.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn detect_all(&self) {
        if self.shared.detecting.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let paths = self.paths.clone();
        let platform = self.platform;
        let options = self.options();
        std::thread::spawn(move || {
            // Leftovers from interrupted installs or older versions of the launcher.
            http::clean_downloads(&paths.downloads(), Duration::from_secs(30 * 86_400));
            let snapshot = crate::detect::Snapshot::take();
            let manifest = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner).clone();
            for app in &SUITE {
                let found = crate::detect::detect(app, &manifest, &snapshot);
                {
                    let mut map = shared.installed.lock().unwrap_or_else(PoisonError::into_inner);
                    match &found {
                        Some(i) => map.insert(app.id.to_string(), i.clone()),
                        None => map.remove(app.id),
                    };
                }
                shared.emit(Event::Detected { app: app.id.to_string(), installed: found });
            }
            // Downloads paused (or cut off by quitting) in an earlier run.
            for app in &SUITE {
                report_partial(app, &shared, &paths, platform, &options);
            }
            shared.detecting.store(false, Ordering::SeqCst);
        });
    }

    /// Looks up the latest releases: ArtCraft's release feed first (one request for every app),
    /// GitHub for anything the feed can't vouch for (apps it doesn't list, entries more than a
    /// day old, or no answer at all), at most once an hour per app unless `force`.
    fn check(&self, force: bool) {
        let (Some(source), Some(http)) = (self.source.clone(), self.http.clone()) else {
            for app in &SUITE {
                self.shared.emit(Event::Latest {
                    app: app.id.to_string(),
                    latest: LatestRelease::Failed("secure connections aren't available".into()),
                    from: Origin::GitHub,
                });
            }
            return;
        };
        let now = crate::now();
        let recent = self.shared.last_check.lock().unwrap_or_else(PoisonError::into_inner).is_some_and(|t| now - t < FRESH_FOR);
        if (recent && !force) || self.shared.checking.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let paths = self.paths.clone();
        let pre = self.options().prereleases;
        std::thread::spawn(move || {
            shared.emit(Event::CheckStarted);
            let (feed, fallback) = if pre {
                // The feed serves stable releases only; with pre-releases on, GitHub decides.
                (None, Fallback { trust_feed: false, github_every: EARLY_GITHUB_EVERY })
            } else {
                feed_and_fallback(&http, &paths, &shared)
            };
            if let Some(f) = &feed {
                shared.emit(Event::Notices { announcements: f.announcements.clone(), alerts: f.alerts.clone() });
            }
            let now = crate::now();
            let queue = Mutex::new(SUITE.iter().chain(std::iter::once(&LAUNCHER)).collect::<Vec<&CraftApp>>());
            let sources = Mutex::new(Vec::new());
            std::thread::scope(|scope| {
                for _ in 0..WORKERS {
                    scope.spawn(|| {
                        loop {
                            let Some(app) = queue.lock().unwrap_or_else(PoisonError::into_inner).pop() else { break };
                            let (latest, from, used) = latest_for(app, feed.as_ref(), fallback, source.as_ref(), &shared, pre, force, now);
                            if let Some(used) = used {
                                sources.lock().unwrap_or_else(PoisonError::into_inner).push(used);
                            }
                            if app.id == LAUNCHER_ID {
                                // Is there a newer ArtCraft Launcher?
                                if let Some(LatestRelease::Found(r)) = latest
                                    && let (Some(newest), Some(current)) = (r.version.as_ref(), crate::self_update::current_version())
                                    && version::is_newer(newest, &current)
                                    && !crate::self_update::just_updated_to(&paths, &r)
                                {
                                    *shared.launcher_release.lock().unwrap_or_else(PoisonError::into_inner) = Some(r.clone());
                                    shared.emit(Event::LauncherUpdate(r));
                                }
                                continue;
                            }
                            let Some(latest) = latest else { continue };
                            if let LatestRelease::Found(r) = &latest {
                                shared.latest.lock().unwrap_or_else(PoisonError::into_inner).insert(app.id.to_string(), r.clone());
                            }
                            if !matches!(latest, LatestRelease::Failed(_)) {
                                shared.answered.lock().unwrap_or_else(PoisonError::into_inner).insert(app.id.to_string());
                            }
                            shared.emit(Event::Latest { app: app.id.to_string(), latest, from });
                        }
                    });
                }
            });
            source.save();
            {
                let answered = shared.answered.lock().unwrap_or_else(PoisonError::into_inner);
                if SUITE.iter().all(|a| answered.contains(a.id)) {
                    shared.loaded.store(true, Ordering::SeqCst);
                }
            }
            let sources = sources.into_inner().unwrap_or_else(PoisonError::into_inner);
            let source_used = if sources.contains(&Source::Cache) {
                Source::Cache
            } else if sources.contains(&Source::GitHubPages) {
                Source::GitHubPages
            } else if sources.contains(&Source::GitHubApi) {
                Source::GitHubApi
            } else {
                Source::Feed
            };
            let at = crate::now();
            *shared.last_check.lock().unwrap_or_else(PoisonError::into_inner) = Some(at);
            shared.checking.store(false, Ordering::SeqCst);
            shared.emit(Event::CheckFinished { at, source: source_used, rate_limited_until: source.rate_limited_until() });
        });
    }

    fn start_job(&self, app_id: &str, uninstall: bool) {
        let Some(app) = CraftApp::by_id(app_id) else { return };
        let stop = Arc::new(Stop::default());
        {
            let mut jobs = self.shared.jobs.lock().unwrap_or_else(PoisonError::into_inner);
            if jobs.contains_key(app.id) {
                return;
            }
            jobs.insert(app.id.to_string(), stop.clone());
        }
        let shared = self.shared.clone();
        let paths = self.paths.clone();
        let http = self.http.clone();
        let platform = self.platform;
        let options = self.options();
        std::thread::spawn(move || {
            let existing = shared.installed.lock().unwrap_or_else(PoisonError::into_inner).get(app.id).cloned();
            let record = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner).apps.get(app.id).cloned();
            let result = if uninstall {
                shared.emit(Event::Progress { app: app.id.to_string(), phase: JobPhase::Removing });
                match &existing {
                    Some(inst) => install::uninstall(app, inst, record.as_ref()).map(|()| {
                        let mut m = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner);
                        if m.apps.remove(app.id).is_some() {
                            let _ = m.save(&paths.manifest());
                        }
                        format!("{} was removed", app.name)
                    }),
                    None => Err(format!("{} isn't installed", app.name)),
                }
            } else {
                let r = run_install(app, &shared, &paths, http.as_ref(), platform, &options, existing.as_ref(), record.as_ref(), &stop);
                // Stopped by the connection rather than by the user: say so, and how to go on.
                match r {
                    Err(e) if e == PAUSED && stop.check().is_ok() => {
                        Err("The download paused because the connection keeps dropping. Press Resume to continue where it stopped.".into())
                    }
                    other => other,
                }
            };
            report_partial(app, &shared, &paths, platform, &options);
            // Look again: the result is whatever is on disk now.
            let manifest = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let found = crate::detect::detect(app, &manifest, &crate::detect::Snapshot::take());
            {
                let mut map = shared.installed.lock().unwrap_or_else(PoisonError::into_inner);
                match &found {
                    Some(i) => map.insert(app.id.to_string(), i.clone()),
                    None => map.remove(app.id),
                };
            }
            shared.emit(Event::Detected { app: app.id.to_string(), installed: found });
            shared.jobs.lock().unwrap_or_else(PoisonError::into_inner).remove(app.id);
            shared.emit(Event::JobFinished { app: app.id.to_string(), result });
        });
    }

    /// Downloads the newer launcher, puts it in place and asks the UI to quit so it can start.
    fn update_launcher(&self) {
        let id = LAUNCHER_ID.to_string();
        let (Some(install), Some(platform)) = (self.self_install.clone(), self.platform) else {
            self.shared.emit(Event::JobFinished {
                app: id,
                result: Err("this copy of ArtCraft Launcher can't update itself; download the new version instead".into()),
            });
            return;
        };
        let Some(release) = self.shared.launcher_release.lock().unwrap_or_else(PoisonError::into_inner).clone() else { return };
        let stop = Arc::new(Stop::default());
        {
            let mut jobs = self.shared.jobs.lock().unwrap_or_else(PoisonError::into_inner);
            if jobs.contains_key(&id) {
                return;
            }
            jobs.insert(id.clone(), stop.clone());
        }
        let (shared, paths, http) = (self.shared.clone(), self.paths.clone(), self.http.clone());
        std::thread::spawn(move || {
            let mut last_emit = Instant::now() - Duration::from_secs(1);
            let mut phase = |p: JobPhase| {
                if matches!(p, JobPhase::Downloading { done, total } if done != total && done != 0) && last_emit.elapsed() < Duration::from_millis(50)
                {
                    return;
                }
                last_emit = Instant::now();
                shared.emit(Event::Progress { app: LAUNCHER_ID.to_string(), phase: p });
            };
            let result = match http.as_ref() {
                Some(http) => crate::self_update::update(&install, &release, platform, http, &paths, &stop, &mut phase),
                None => Err("secure connections aren't available on this system".into()),
            };
            shared.jobs.lock().unwrap_or_else(PoisonError::into_inner).remove(LAUNCHER_ID);
            match result {
                Ok(()) => shared.emit(Event::LauncherRestarting),
                Err(e) => shared.emit(Event::JobFinished { app: LAUNCHER_ID.to_string(), result: Err(e) }),
            }
        });
    }

    fn installed(&self, app: &str) -> Option<Installed> {
        self.shared.installed.lock().unwrap_or_else(PoisonError::into_inner).get(app).cloned()
    }
}

#[allow(clippy::too_many_arguments)]
fn run_install(
    app: &CraftApp,
    shared: &Shared,
    paths: &Paths,
    http: Option<&Http>,
    platform: Option<Platform>,
    options: &BackendOptions,
    existing: Option<&Installed>,
    record: Option<&crate::manifest::Record>,
    stop: &Stop,
) -> Result<String, String> {
    if let Some(msg) = existing.and_then(|e| install::managed_elsewhere(app, e, false)) {
        return Err(msg);
    }
    let http = http.ok_or("secure connections aren't available on this system")?;
    let platform = platform.ok_or("this system isn't supported")?;
    let release = shared
        .latest
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(app.id)
        .cloned()
        .ok_or("the latest release hasn't been found yet. Check for updates first.")?;
    let (asset, kind) = planned_asset(app, &release, platform, options, existing, record)
        .ok_or_else(|| format!("{} {} has no download for {}", app.name, release.version_label(), platform.label()))?;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut phase = |p: JobPhase| {
        // Downloads report thousands of times; the UI needs ~20 a second.
        if matches!(p, JobPhase::Downloading { done, total } if done != total && done != 0) && last_emit.elapsed() < Duration::from_millis(50) {
            return;
        }
        last_emit = Instant::now();
        shared.emit(Event::Progress { app: app.id.to_string(), phase: p });
    };
    let mut job = Job { http, paths, stop, phase: &mut phase };
    let (new_record, verified) = install::install(app, &release, &asset, kind, existing, &mut job)?;
    {
        let mut m = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = match new_record {
            Some(r) => {
                m.apps.insert(app.id.to_string(), r);
                true
            }
            // Installed the platform's own way: any old launcher record is stale.
            None => m.apps.remove(app.id).is_some(),
        };
        if changed && let Err(e) = m.save(&paths.manifest()) {
            log::warn!("couldn't save the install record: {e}");
        }
    }
    let verb = if existing.is_some() { "updated to" } else { "installed:" };
    let note = match verified {
        http::Verified::Checksum => "",
        http::Verified::SizeOnly => " (this release publishes no checksum, so only its size was checked)",
    };
    Ok(format!("{} {verb} {}{note}", app.name, release.version_label()))
}

/// The file an install of `app` would download now: same format as the installed copy, else
/// the user's preference, else the best this system supports.
fn planned_asset(
    app: &CraftApp,
    release: &Release,
    platform: Platform,
    options: &BackendOptions,
    existing: Option<&Installed>,
    record: Option<&crate::manifest::Record>,
) -> Option<(Asset, PackageKind)> {
    let stems: Vec<&str> = app.stems().collect();
    let kinds = install::choose_kinds(existing, record, &options.kinds);
    pick_asset(release, &stems, platform, &kinds).map(|(a, k)| (a.clone(), k))
}

/// Tells the UI whether `app` has a paused download waiting.
fn report_partial(app: &CraftApp, shared: &Shared, paths: &Paths, platform: Option<Platform>, options: &BackendOptions) {
    let download = partial_for(app, shared, paths, platform, options).map(|(asset, done)| PausedDownload { done, total: asset.size });
    shared.emit(Event::Paused { app: app.id.to_string(), download });
}

/// The planned download of `app` and how many bytes of it are already on disk.
fn partial_for(app: &CraftApp, shared: &Shared, paths: &Paths, platform: Option<Platform>, options: &BackendOptions) -> Option<(Asset, u64)> {
    let release = shared.latest.lock().unwrap_or_else(PoisonError::into_inner).get(app.id).cloned()?;
    let existing = shared.installed.lock().unwrap_or_else(PoisonError::into_inner).get(app.id).cloned();
    let record = shared.manifest.lock().unwrap_or_else(PoisonError::into_inner).apps.get(app.id).cloned();
    let (asset, _) = planned_asset(app, &release, platform?, options, existing.as_ref(), record.as_ref())?;
    let done = http::partial_bytes(&install::download_path(paths, &asset), asset.size)?;
    Some((asset, done))
}

/// How much the feed is trusted in one check, and how freely GitHub may be asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fallback {
    /// Use the feed's answers (fresh entries only). Off when the feed has seriously failed.
    trust_feed: bool,
    /// Ask GitHub about an app at most this often (seconds), unless forced.
    github_every: i64,
}

/// Asks the feed (retrying, except on the very first poll) and decides how much to lean on
/// GitHub this time:
///
/// - The very first poll (nothing has ever loaded, nothing saved): one try, then GitHub at once,
///   so the window fills.
/// - Until every app has loaded once: if the feed doesn't answer after its retries, GitHub is
///   asked fairly freely (every 5 minutes per app).
/// - After that: the feed's last answer stands in while it fails, and GitHub is asked only for
///   apps the feed doesn't list or hasn't refreshed in a day, or once the feed has seriously
///   failed (several checks in a row and an hour without an answer); then hourly per app.
fn feed_and_fallback(http: &Http, paths: &Paths, shared: &Shared) -> (Option<Feed>, Fallback) {
    let saved = load_feed(paths);
    let loaded = shared.loaded.load(Ordering::SeqCst);
    let first_time = !loaded && saved.is_none();
    let attempts = if first_time { 1 } else { FEED_ATTEMPTS };
    let fresh = fetch_feed(http, paths, attempts);
    let now = crate::now();
    let failures = {
        let mut failures = shared.feed_failures.lock().unwrap_or_else(PoisonError::into_inner);
        if fresh.is_some() {
            *failures = 0;
            *shared.feed_ok_at.lock().unwrap_or_else(PoisonError::into_inner) = Some(now);
        } else {
            *failures = failures.saturating_add(1);
        }
        *failures
    };
    let ok_at = *shared.feed_ok_at.lock().unwrap_or_else(PoisonError::into_inner);
    let answered = fresh.is_some();
    let fallback = decide_fallback(answered, loaded, first_time, failures, ok_at, now);
    (fresh.or(saved), fallback)
}

/// The policy in [`feed_and_fallback`], on its own so it can be tested.
fn decide_fallback(answered: bool, loaded: bool, first_time: bool, failures: u32, ok_at: Option<i64>, now: i64) -> Fallback {
    let github_every = if loaded { GITHUB_FALLBACK_EVERY } else { EARLY_GITHUB_EVERY };
    let seriously_failed =
        !answered && (first_time || !loaded || (failures >= SERIOUS_FEED_FAILURES && ok_at.is_none_or(|t| now - t >= SERIOUS_FEED_OUTAGE)));
    Fallback { trust_feed: !seriously_failed, github_every }
}

/// The latest release of `app`, which service it came from, and the GitHub route used (for the
/// rate-limit notice): the feed when it's trusted and vouches for the app, else GitHub if it's
/// due, else GitHub's last answer. No release when there's nothing new to say.
#[allow(clippy::too_many_arguments)]
fn latest_for(
    app: &CraftApp,
    feed: Option<&Feed>,
    fallback: Fallback,
    source: &dyn ReleaseSource,
    shared: &Shared,
    pre: bool,
    force: bool,
    now: i64,
) -> (Option<LatestRelease>, Origin, Option<Source>) {
    if fallback.trust_feed {
        match feed.map(|f| f.answer(app, now)) {
            Some(FeedAnswer::Fresh(Some(r))) => return (Some(LatestRelease::Found(r)), Origin::ArtCraftApi, Some(Source::Feed)),
            Some(FeedAnswer::Fresh(None)) => return (Some(LatestRelease::None), Origin::ArtCraftApi, Some(Source::Feed)),
            Some(FeedAnswer::AskGitHub) | None => {}
        }
    }
    let repo = feed_repo(feed, app);
    let due = {
        let mut asked = shared.github_asked.lock().unwrap_or_else(PoisonError::into_inner);
        let due = force || asked.get(app.id).is_none_or(|t| now - t >= fallback.github_every);
        if due {
            asked.insert(app.id.to_string(), now);
        }
        due
    };
    if due {
        let answer = source.latest(&repo, pre);
        (Some(answer.latest), Origin::GitHub, Some(answer.source))
    } else {
        (source.cached(&repo, pre), Origin::GitHub, None)
    }
}

/// The repo to ask GitHub about, following the feed's repo renames.
fn feed_repo(feed: Option<&Feed>, app: &CraftApp) -> String {
    feed.map_or_else(|| app.repo.to_string(), |f| f.repo_for(app))
}

/// Asks ArtCraft's release feed up to `attempts` times (waiting 1 s, then 3 s, between tries),
/// saving a good answer. `None` when it never answered with something readable.
fn fetch_feed(http: &Http, paths: &Paths, attempts: usize) -> Option<Feed> {
    let url = feed_url();
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            std::thread::sleep(FEED_RETRY_WAITS.get(attempt - 1).copied().unwrap_or(Duration::from_secs(3)));
        }
        match http.get_feed(&url) {
            Ok(body) => match Feed::parse(&body) {
                Some(feed) => {
                    if let Err(e) = crate::paths::write_atomic(&paths.feed_cache(), body.as_bytes()) {
                        log::warn!("couldn't save the release feed: {e}");
                    }
                    return Some(feed);
                }
                None => log::warn!("the release feed answered with something unreadable (try {})", attempt + 1),
            },
            Err(e) => log::info!("release feed unavailable (try {}): {e}", attempt + 1),
        }
    }
    None
}

/// The feed's address. `ARTCRAFT_LAUNCHER_FEED_URL` points at a staging server or a local copy
/// (only `https://`, or plain `http://` on localhost).
fn feed_url() -> String {
    std::env::var("ARTCRAFT_LAUNCHER_FEED_URL")
        .ok()
        .filter(|u| u.starts_with("https://") || u.starts_with("http://127.0.0.1:") || u.starts_with("http://localhost:"))
        .unwrap_or_else(|| artcraft_launcher_core::feed::FEED_URL.to_string())
}

/// The last saved feed answer, if any.
fn load_feed(paths: &Paths) -> Option<Feed> {
    std::fs::read_to_string(paths.feed_cache()).ok().and_then(|body| Feed::parse(&body))
}

/// The machine's real architecture: an x64 launcher on Windows on ARM should still pick ARM64
/// builds.
fn native_platform() -> Option<Platform> {
    let mut p = Platform::current()?;
    if p.os == Os::Windows {
        let arch = std::env::var("PROCESSOR_ARCHITEW6432").or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE")).unwrap_or_default();
        if arch.eq_ignore_ascii_case("ARM64") {
            p.arch = Arch::Arm64;
        } else if arch.eq_ignore_ascii_case("AMD64") {
            p.arch = Arch::X64;
        }
    }
    Some(p)
}

impl Backend for NativeBackend {
    fn send(&mut self, command: Command) {
        match command {
            Command::CheckForUpdates { force } => self.check(force),
            Command::Detect => self.detect_all(),
            Command::Install { app } => self.start_job(&app, false),
            Command::Uninstall { app } => self.start_job(&app, true),
            Command::Pause { app } => {
                if let Some(stop) = self.shared.jobs.lock().unwrap_or_else(PoisonError::into_inner).get(&app) {
                    stop.pause();
                }
            }
            Command::Cancel { app } => {
                let running = self.shared.jobs.lock().unwrap_or_else(PoisonError::into_inner).get(&app).cloned();
                match running {
                    Some(stop) => stop.cancel(),
                    // A paused download: throw its partial file away.
                    None => {
                        if let Some(def) = CraftApp::by_id(&app)
                            && let Some((asset, _)) = partial_for(def, &self.shared, &self.paths, self.platform, &self.options())
                        {
                            http::discard_partial(&install::download_path(&self.paths, &asset), asset.size);
                        }
                        self.shared.emit(Event::Paused { app, download: None });
                    }
                }
            }
            Command::Launch { app } => {
                let Some(def) = CraftApp::by_id(&app) else { return };
                let result = match self.installed(&app) {
                    Some(inst) => install::launch(def, &inst),
                    None => Err(format!("{} isn't installed", def.name)),
                };
                if let Err(e) = result {
                    self.shared.emit(Event::JobFinished { app, result: Err(e) });
                }
            }
            Command::Reveal { app } => {
                if let Some(inst) = self.installed(&app) {
                    let path = std::path::PathBuf::from(&inst.location);
                    let result = if path.exists() { crate::process::reveal(&path) } else { Err("it isn't a file on this computer".into()) };
                    if let Err(e) = result {
                        self.shared.emit(Event::JobFinished { app, result: Err(format!("Couldn't show it: {e}")) });
                    }
                }
            }
            Command::UpdateLauncher => self.update_launcher(),
            Command::Configure(options) => {
                let changed_pre = {
                    let mut o = self.shared.options.lock().unwrap_or_else(PoisonError::into_inner);
                    let changed = o.prereleases != options.prereleases;
                    *o = options;
                    changed
                };
                if changed_pre {
                    self.check(true);
                }
            }
        }
    }

    fn poll(&mut self) -> Vec<Event> {
        std::mem::take(&mut *self.shared.events.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn capabilities(&self) -> Capabilities {
        let reveal_label = match self.platform.map(|p| p.os) {
            Some(Os::MacOs) => "Show in Finder",
            Some(Os::Windows) => "Show in Explorer",
            _ => "Show in Files",
        };
        Capabilities { manage_apps: true, reveal_label, self_update: self.self_install.is_some() }
    }

    fn platform(&self) -> Option<Platform> {
        self.platform
    }

    fn now(&self) -> i64 {
        crate::now()
    }
}

/// The launcher's own download for this machine, for "a new launcher is available".
pub fn launcher_download(release: &Release, platform: Platform) -> Option<String> {
    let kinds: &[artcraft_launcher_core::PackageKind] = match platform.os {
        Os::Linux => &[artcraft_launcher_core::PackageKind::AppImage, artcraft_launcher_core::PackageKind::Deb],
        _ => platform.default_kinds(),
    };
    pick_asset(release, &[LAUNCHER_ID], platform, kinds).map(|(a, _)| a.url.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use artcraft_launcher_core::protocol::LatestRelease;

    use super::*;
    use crate::source::Answer;

    /// GitHub, counting how often it's asked.
    #[derive(Default)]
    struct CountingGitHub(AtomicUsize);

    impl ReleaseSource for CountingGitHub {
        fn latest(&self, _repo: &str, _pre: bool) -> Answer {
            self.0.fetch_add(1, Ordering::SeqCst);
            Answer { latest: LatestRelease::None, source: Source::GitHubApi }
        }
        fn cached(&self, _repo: &str, _pre: bool) -> Option<LatestRelease> {
            None
        }
        fn rate_limited_until(&self) -> Option<i64> {
            None
        }
        fn save(&self) {}
    }

    fn shared() -> Shared {
        Shared {
            events: Mutex::new(Vec::new()),
            waker: Arc::new(|| {}),
            latest: Mutex::new(HashMap::new()),
            installed: Mutex::new(HashMap::new()),
            jobs: Mutex::new(HashMap::new()),
            manifest: Mutex::new(Manifest::default()),
            options: Mutex::new(BackendOptions::default()),
            checking: AtomicBool::new(false),
            detecting: AtomicBool::new(false),
            last_check: Mutex::new(None),
            launcher_release: Mutex::new(None),
            github_asked: Mutex::new(HashMap::new()),
            answered: Mutex::new(std::collections::HashSet::new()),
            loaded: AtomicBool::new(false),
            feed_failures: Mutex::new(0),
            feed_ok_at: Mutex::new(None),
        }
    }

    const NOW: i64 = 1_791_500_000;
    /// The steady state: everything has loaded and the feed is answering.
    const LOADED: Fallback = Fallback { trust_feed: true, github_every: GITHUB_FALLBACK_EVERY };

    fn feed() -> Feed {
        Feed::parse(
            r#"{"release_info": {"apps": {
            "photocraft": {"repo": "storytold/photocraft", "latest_release": {"tag": "v0.6.0"}, "app_last_fetched": "2026-10-09T13:00:00Z"}
        }}}"#,
        )
        .unwrap()
    }

    #[test]
    fn the_first_poll_tries_once_and_falls_back_at_once() {
        // Nothing has ever loaded and the feed didn't answer: GitHub, freely.
        let f = decide_fallback(false, false, true, 1, None, NOW);
        assert_eq!(f, Fallback { trust_feed: false, github_every: EARLY_GITHUB_EVERY });
        // The feed answered: trust it.
        assert!(decide_fallback(true, false, true, 0, Some(NOW), NOW).trust_feed);
    }

    #[test]
    fn before_everything_loads_github_is_asked_more_eagerly() {
        // A saved feed but not everything loaded yet, and the feed isn't answering.
        let f = decide_fallback(false, false, false, 1, None, NOW);
        assert_eq!(f, Fallback { trust_feed: false, github_every: EARLY_GITHUB_EVERY });
    }

    #[test]
    fn after_loading_github_waits_for_a_serious_feed_failure() {
        // A blip or two: keep showing the feed's last answer.
        for failures in 1..SERIOUS_FEED_FAILURES {
            let f = decide_fallback(false, true, false, failures, Some(NOW - 10 * 60), NOW);
            assert_eq!(f, Fallback { trust_feed: true, github_every: GITHUB_FALLBACK_EVERY }, "{failures}");
        }
        // Several checks in a row, but it answered recently: still trusted.
        assert!(decide_fallback(false, true, false, SERIOUS_FEED_FAILURES, Some(NOW - 20 * 60), NOW).trust_feed);
        // Several checks in a row and an hour without an answer: GitHub takes over, hourly.
        let f = decide_fallback(false, true, false, SERIOUS_FEED_FAILURES, Some(NOW - SERIOUS_FEED_OUTAGE), NOW);
        assert_eq!(f, Fallback { trust_feed: false, github_every: GITHUB_FALLBACK_EVERY });
    }

    #[test]
    fn github_is_only_the_fallback_and_is_asked_rarely() {
        let github = CountingGitHub::default();
        let shared = shared();
        let photo = CraftApp::by_id("photocraft").unwrap();
        let word = CraftApp::by_id("wordcraft").unwrap();
        let feed = feed();

        // The feed vouches for PhotoCraft: GitHub isn't asked.
        let (latest, _, used) = latest_for(photo, Some(&feed), LOADED, &github, &shared, false, false, NOW);
        assert!(matches!(latest, Some(LatestRelease::Found(ref r)) if r.tag == "v0.6.0"));
        assert_eq!(used, Some(Source::Feed));
        assert_eq!(github.0.load(Ordering::SeqCst), 0);

        // The feed doesn't list WordCraft: GitHub is asked once…
        let (_, _, used) = latest_for(word, Some(&feed), LOADED, &github, &shared, false, false, NOW);
        assert_eq!(used, Some(Source::GitHubApi));
        assert_eq!(github.0.load(Ordering::SeqCst), 1);
        // …not again within the hour…
        let (_, _, used) = latest_for(word, Some(&feed), LOADED, &github, &shared, false, false, NOW + 30 * 60);
        assert_eq!(used, None);
        assert_eq!(github.0.load(Ordering::SeqCst), 1);
        // …but Refresh asks right away, and an hour later it's due again.
        latest_for(word, Some(&feed), LOADED, &github, &shared, false, true, NOW + 31 * 60);
        assert_eq!(github.0.load(Ordering::SeqCst), 2);
        latest_for(word, Some(&feed), LOADED, &github, &shared, false, false, NOW + 31 * 60 + GITHUB_FALLBACK_EVERY);
        assert_eq!(github.0.load(Ordering::SeqCst), 3);

        // No feed answer at all (server down): GitHub, still rate-limited per app.
        let (_, _, used) = latest_for(photo, None, LOADED, &github, &shared, false, false, NOW);
        assert_eq!(used, Some(Source::GitHubApi));
        assert_eq!(github.0.load(Ordering::SeqCst), 4);

        // A feed entry more than a day old doesn't count.
        let fresh_shared = super::tests::shared();
        let (_, _, used) = latest_for(photo, Some(&feed), LOADED, &github, &fresh_shared, false, false, NOW + 2 * 86_400);
        assert_eq!(used, Some(Source::GitHubApi));
    }
}
