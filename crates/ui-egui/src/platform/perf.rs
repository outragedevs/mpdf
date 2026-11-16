//! Performance diagnostics: what a run cost, written to the log for a report.
//!
//! Every few seconds in which the window ran passes, one record (target [`TARGET`], debug
//! level) says how many passes ran and how long they took, how long the frame cap made them
//! wait, what input arrived, and how many page pictures came back from the render threads and
//! how long those took. A pass that took long gets a record of its own. The painter adds what
//! drawing the frames cost (`egui_wgpu::partial_repaint`).
//!
//! Nothing here names a document, a file, a folder or any text of a page: the records are
//! counts, sizes and times only, so a log can leave the machine it was written on. Keep it so.
//!
//! Off unless the desktop app turns it on ([`Perf::enable`]): tests, screenshots and the web
//! build never record.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use web_time::{Duration, Instant};

/// The log target of every record written here.
pub const TARGET: &str = "diagnostics";

/// A record is written once passes have run for this long.
const EVERY: Duration = Duration::from_secs(5);

/// A pass at least this long counts as slow.
const SLOW: Duration = Duration::from_millis(50);

/// A pass at least this long gets a record of its own.
const VERY_SLOW: Duration = Duration::from_millis(150);

/// Whether anything records. The page cache reports pictures from where it has no app.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The page pictures received since the last record.
static RASTERS: Mutex<Rasters> = Mutex::new(Rasters::new());

/// What kind of picture a render thread returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Raster {
    Page,
    Tile,
    Thumbnail,
    Text,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RasterKind {
    count: u32,
    millis: u64,
    longest: u32,
    pixels: u64,
}

impl RasterKind {
    const fn new() -> Self {
        Self { count: 0, millis: 0, longest: 0, pixels: 0 }
    }

    fn add(&mut self, millis: u32, pixels: u64) {
        self.count = self.count.saturating_add(1);
        self.millis = self.millis.saturating_add(u64::from(millis));
        self.longest = self.longest.max(millis);
        self.pixels = self.pixels.saturating_add(pixels);
    }

    /// `3 pages (210 ms each, 400 ms at most, 6.2 MP)`, or nothing for none.
    fn describe(&self, name: &str) -> Option<String> {
        (self.count > 0).then(|| {
            format!(
                "{} {name} ({} ms each, {} ms at most, {:.1} MP)",
                self.count,
                self.millis / u64::from(self.count),
                self.longest,
                self.pixels as f64 / 1.0e6
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rasters {
    pages: RasterKind,
    tiles: RasterKind,
    thumbnails: RasterKind,
    texts: RasterKind,
    failed: u32,
}

impl Rasters {
    const fn new() -> Self {
        Self { pages: RasterKind::new(), tiles: RasterKind::new(), thumbnails: RasterKind::new(), texts: RasterKind::new(), failed: 0 }
    }

    fn describe(&self) -> String {
        let mut parts: Vec<String> =
            [self.pages.describe("pages"), self.tiles.describe("tiles"), self.thumbnails.describe("thumbnails"), self.texts.describe("text layers")]
                .into_iter()
                .flatten()
                .collect();
        if self.failed > 0 {
            parts.push(format!("{} failed", self.failed));
        }
        if parts.is_empty() { "none".to_owned() } else { parts.join(", ") }
    }
}

/// A render thread returned a picture of `kind`, `width` by `height` pixels, after `millis`.
pub(crate) fn raster_received(kind: Raster, millis: u32, width: u32, height: u32, failed: bool) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let mut rasters = RASTERS.lock().unwrap_or_else(PoisonError::into_inner);
    if failed {
        rasters.failed = rasters.failed.saturating_add(1);
        return;
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    match kind {
        Raster::Page => rasters.pages.add(millis, pixels),
        Raster::Tile => rasters.tiles.add(millis, pixels),
        Raster::Thumbnail => rasters.thumbnails.add(millis, pixels),
        Raster::Text => rasters.texts.add(millis, pixels),
    }
}

/// The input events of one pass, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Input {
    moves: u32,
    wheels: u32,
    keys: u32,
    presses: u32,
    texts: u32,
}

impl Input {
    fn of(events: &[egui::Event]) -> Self {
        let mut input = Self::default();
        for event in events {
            match event {
                egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_) => input.moves += 1,
                egui::Event::MouseWheel { .. } => input.wheels += 1,
                egui::Event::Key { pressed: true, .. } => input.keys += 1,
                egui::Event::PointerButton { pressed: true, .. } => input.presses += 1,
                egui::Event::Text(_) | egui::Event::Paste(_) => input.texts += 1,
                _ => {}
            }
        }
        input
    }

    fn add(&mut self, other: Self) {
        self.moves = self.moves.saturating_add(other.moves);
        self.wheels = self.wheels.saturating_add(other.wheels);
        self.keys = self.keys.saturating_add(other.keys);
        self.presses = self.presses.saturating_add(other.presses);
        self.texts = self.texts.saturating_add(other.texts);
    }

    fn describe(&self) -> String {
        format!(
            "{} pointer moves, {} wheel steps, {} keys, {} presses, {} text entries",
            self.moves, self.wheels, self.keys, self.presses, self.texts
        )
    }
}

/// The pass that is running.
#[derive(Clone, Copy, Debug)]
struct Pass {
    started: Instant,
    scrolled: bool,
    input: Input,
}

/// The app's performance record; see the module.
#[derive(Debug, Default)]
pub struct Perf {
    enabled: bool,
    /// When the process started, for the time to the first pass.
    process_started: Option<Instant>,
    first_pass_reported: bool,
    since: Option<Instant>,
    pass: Option<Pass>,
    passes: u32,
    scroll_passes: u32,
    slow_passes: u32,
    seconds: f64,
    longest: f64,
    waited: f64,
    input: Input,
}

impl Perf {
    /// Start recording. `process_started`: when the process began, as the app measured it.
    pub fn enable(&mut self, process_started: Option<Instant>) {
        self.enabled = true;
        self.process_started = process_started;
        ENABLED.store(true, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// A pass starts at `now`, after waiting `waited` for the frame cap. `scrolled`: the pass
    /// before moved the page view.
    pub(crate) fn begin_pass(&mut self, now: Instant, waited: Option<Duration>, scrolled: bool, events: &[egui::Event]) {
        if !self.enabled {
            return;
        }
        self.waited += waited.unwrap_or_default().as_secs_f64();
        self.pass = Some(Pass { started: now, scrolled, input: Input::of(events) });
    }

    /// The pass that began last ends at `now`; `window`: the window's size in points and its
    /// pixels per point. Returns the records to write.
    pub(crate) fn end_pass(&mut self, now: Instant, window: (egui::Vec2, f32)) -> Vec<String> {
        let mut records = Vec::new();
        let Some(pass) = self.pass.take().filter(|_| self.enabled) else { return records };
        let took = now.saturating_duration_since(pass.started);
        if !self.first_pass_reported {
            self.first_pass_reported = true;
            if let Some(started) = self.process_started {
                records.push(format!("first pass ended {} ms after the start", now.saturating_duration_since(started).as_millis()));
            }
        }
        self.passes = self.passes.saturating_add(1);
        self.scroll_passes = self.scroll_passes.saturating_add(u32::from(pass.scrolled));
        self.slow_passes = self.slow_passes.saturating_add(u32::from(took >= SLOW));
        self.seconds += took.as_secs_f64();
        self.longest = self.longest.max(took.as_secs_f64());
        self.input.add(pass.input);
        if took >= VERY_SLOW {
            records.push(format!(
                "slow pass: {} ms{}; {}",
                took.as_millis(),
                if pass.scrolled { " after scrolling" } else { "" },
                pass.input.describe()
            ));
        }
        let since = *self.since.get_or_insert(pass.started);
        let span = now.saturating_duration_since(since);
        if span >= EVERY {
            let rasters = std::mem::take(&mut *RASTERS.lock().unwrap_or_else(PoisonError::into_inner));
            records.push(self.summary(span, &rasters, window));
            *self = Self { enabled: true, process_started: self.process_started, first_pass_reported: true, ..Self::default() };
        }
        records
    }

    fn summary(&self, span: Duration, rasters: &Rasters, (size, pixels_per_point): (egui::Vec2, f32)) -> String {
        let each = if self.passes > 0 { 1000.0 * self.seconds / f64::from(self.passes) } else { 0.0 };
        format!(
            "last {:.1} s: {} passes ({each:.1} ms each, {:.1} ms at most, {} of 50 ms or more), {} after scrolling; waited {:.0} ms for the frame cap; input: {}; pictures: {}; window {:.0}x{:.0} points at {pixels_per_point:.2}",
            span.as_secs_f64(),
            self.passes,
            1000.0 * self.longest,
            self.slow_passes,
            self.scroll_passes,
            1000.0 * self.waited,
            self.input.describe(),
            rasters.describe(),
            size.x,
            size.y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (egui::Vec2, f32) = (egui::vec2(1440.0, 920.0), 1.0);

    fn moves(n: usize) -> Vec<egui::Event> {
        vec![egui::Event::PointerMoved(egui::pos2(1.0, 2.0)); n]
    }

    #[test]
    fn nothing_is_recorded_until_it_is_enabled() {
        let mut perf = Perf::default();
        let t0 = Instant::now();
        perf.begin_pass(t0, None, false, &moves(3));
        assert!(perf.end_pass(t0 + Duration::from_secs(9), WINDOW).is_empty());
    }

    #[test]
    fn passes_are_summed_up_every_few_seconds() {
        let mut perf = Perf { enabled: true, ..Perf::default() };
        let t0 = Instant::now();
        // Two quick passes, the second after a scroll and a wait for the frame cap.
        perf.begin_pass(t0, None, false, &moves(2));
        assert!(perf.end_pass(t0 + Duration::from_millis(4), WINDOW).is_empty());
        perf.begin_pass(t0 + Duration::from_millis(66), Some(Duration::from_millis(30)), true, &[]);
        assert!(perf.end_pass(t0 + Duration::from_millis(72), WINDOW).is_empty());
        // A long one, past the interval: its own record, then the summary.
        let late = t0 + Duration::from_millis(5000);
        perf.begin_pass(late, None, false, &moves(1));
        let records = perf.end_pass(late + Duration::from_millis(200), WINDOW);
        assert_eq!(records.len(), 2, "{records:?}");
        assert!(records[0].starts_with("slow pass: 200 ms; 1 pointer moves"), "{records:?}");
        let summary = &records[1];
        assert!(summary.starts_with("last 5.2 s: 3 passes (70.0 ms each, 200.0 ms at most, 1 of 50 ms or more), 1 after scrolling;"), "{summary}");
        assert!(summary.contains("waited 30 ms for the frame cap"), "{summary}");
        assert!(summary.contains("input: 3 pointer moves, 0 wheel steps, 0 keys, 0 presses, 0 text entries"), "{summary}");
        assert!(summary.ends_with("window 1440x920 points at 1.00"), "{summary}");
        // The next interval starts empty.
        perf.begin_pass(late + Duration::from_secs(1), None, false, &[]);
        assert!(perf.end_pass(late + Duration::from_secs(1), WINDOW).is_empty());
        assert_eq!(perf.passes, 1);
    }

    #[test]
    fn the_first_pass_reports_how_long_the_start_took() {
        let t0 = Instant::now();
        let mut perf = Perf { enabled: true, process_started: Some(t0), ..Perf::default() };
        perf.begin_pass(t0 + Duration::from_millis(700), None, false, &[]);
        assert_eq!(perf.end_pass(t0 + Duration::from_millis(810), WINDOW), ["first pass ended 810 ms after the start"]);
        perf.begin_pass(t0 + Duration::from_millis(900), None, false, &[]);
        assert!(perf.end_pass(t0 + Duration::from_millis(905), WINDOW).is_empty(), "said once");
    }

    #[test]
    fn pictures_are_described_by_kind_without_naming_a_page() {
        let mut rasters = Rasters::default();
        assert_eq!(rasters.describe(), "none");
        rasters.pages.add(100, 2_000_000);
        rasters.pages.add(300, 2_000_000);
        rasters.tiles.add(40, 1_048_576);
        rasters.failed = 1;
        assert_eq!(rasters.describe(), "2 pages (200 ms each, 300 ms at most, 4.0 MP), 1 tiles (40 ms each, 40 ms at most, 1.0 MP), 1 failed");
    }
}
