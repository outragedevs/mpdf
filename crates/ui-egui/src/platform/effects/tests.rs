use super::*;

fn effects(mode: ReducedEffects, adapter: Option<bool>) -> Effects {
    let mut e = Effects { mode, ..Effects::default() };
    if let Some(cpu) = adapter {
        e.set_software_renderer(cpu);
    }
    e
}

#[test]
fn reduced_effects_follow_the_override_then_the_adapter() {
    for adapter in [Some(true), Some(false), None] {
        assert!(effects(ReducedEffects::On, adapter).reduced(), "on is reduced on {adapter:?}");
        assert!(!effects(ReducedEffects::Off, adapter).reduced(), "off is never reduced, {adapter:?}");
    }
    assert!(effects(ReducedEffects::Auto, Some(true)).reduced(), "a software rasterizer reduces them");
    assert!(!effects(ReducedEffects::Auto, Some(false)).reduced(), "a GPU doesn't");
    assert!(!effects(ReducedEffects::Auto, None).reduced(), "nor does a start that reported no adapter");

    assert_eq!(ReducedEffects::from_env(Some("1")), ReducedEffects::On);
    assert_eq!(ReducedEffects::from_env(Some(" 1 ")), ReducedEffects::On);
    assert_eq!(ReducedEffects::from_env(Some("0")), ReducedEffects::Off);
    for other in [Some(""), Some("yes"), Some("on"), None] {
        assert_eq!(ReducedEffects::from_env(other), ReducedEffects::Auto, "{other:?}");
    }
    for mode in ReducedEffects::ALL {
        assert_eq!(ReducedEffects::parse(mode.name()), Some(mode));
        assert_eq!(ReducedEffects::parse(&mode.name().to_uppercase()), Some(mode));
    }
    assert_eq!(ReducedEffects::parse("maybe"), None);
}

#[test]
fn dithering_is_off_only_after_a_software_start_or_when_forced() {
    for last in [true, false] {
        assert!(!ReducedEffects::On.dithering_at_start(last), "forced on, after a software start: {last}");
        assert!(ReducedEffects::Off.dithering_at_start(last), "forced off, after a software start: {last}");
    }
    assert!(!ReducedEffects::Auto.dithering_at_start(true));
    assert!(ReducedEffects::Auto.dithering_at_start(false));
}

#[test]
fn the_software_flag_survives_a_save_and_a_start_without_an_adapter() {
    let mut first = effects(ReducedEffects::On, Some(true));
    let saved_json = serde_json::json!({ KEY: first.saved_value() });
    assert!(saved(&saved_json));

    // An OpenGL start reports no adapter and keeps what the last start said.
    let mut next = Effects::default();
    next.restore(&saved_json);
    assert!(next.saved_value());
    assert!(!next.reduced(), "the saved flag alone doesn't reduce effects");
    next.set_software_renderer(false);
    assert!(!next.saved_value(), "a GPU start replaces it");

    for malformed in [serde_json::json!({}), serde_json::json!({ KEY: 1 }), serde_json::json!("["), serde_json::Value::Null] {
        assert!(!saved(&malformed), "{malformed}");
    }
    first.mode = ReducedEffects::Off;
    assert!(first.saved_value(), "the session override is not what gets saved");
}

#[test]
fn the_state_is_handed_to_egui_once_per_change() {
    let ctx = egui::Context::default();
    assert!(!reduced(&ctx));
    let mut e = effects(ReducedEffects::Auto, Some(true));
    assert!(e.publish(&ctx));
    assert!(reduced(&ctx));
    assert!(!e.publish(&ctx), "unchanged");
    e.mode = ReducedEffects::Off;
    assert!(e.publish(&ctx));
    assert!(!reduced(&ctx));
}

#[test]
fn the_frame_cap_waits_out_the_rest_of_the_interval() {
    let size = egui::vec2(1440.0, 920.0);
    let mut cap = FrameCap::per_second(30).unwrap();
    assert_eq!(cap.fps(), 30);
    let t0 = Instant::now();
    assert_eq!(cap.wait(t0, size, false), None, "the first frame never waits");
    cap.frame_started(t0, size);
    let wait = cap.wait(t0 + Duration::from_millis(10), size, false).unwrap();
    assert!((wait.as_secs_f64() - (1.0 / 30.0 - 0.010)).abs() < 0.001, "{wait:?}");
    assert_eq!(cap.wait(t0 + Duration::from_millis(34), size, false), None, "the interval has passed");
    assert_eq!(cap.wait(t0 + Duration::from_millis(10), egui::vec2(1000.0, 920.0), false), None, "a resize is drawn at once");
    assert_eq!(cap.wait(t0, size, false), Some(Duration::from_secs(1) / 30), "right after a frame: the whole interval");

    assert!(FrameCap::per_second(0).is_none(), "0 is no cap");
    assert_eq!(FrameCap::per_second(100_000).unwrap().fps(), MAX_FPS_LIMIT);
}

#[test]
fn only_reduced_effects_wait_for_the_cap() {
    let size = egui::vec2(800.0, 600.0);
    let t0 = Instant::now();
    let soon = t0 + Duration::from_millis(5);
    let mut e = effects(ReducedEffects::Auto, Some(true));
    assert_eq!(e.frame_wait(t0, size, false), None, "no cap installed: tests and the web never wait");
    e.frame_cap = FrameCap::per_second(DEFAULT_MAX_FPS);
    assert_eq!(e.max_fps(), Some(30));
    assert_eq!(e.frame_wait(t0, size, false), None);
    let wait = e.frame_wait(soon, size, false).unwrap();
    assert!(wait > Duration::from_millis(25) && wait < Duration::from_millis(30), "{wait:?}");
    // The next frame counts from the end of that wait.
    let after = soon + wait + Duration::from_millis(1);
    assert!(e.frame_wait(after, size, false).is_some());

    // The same rhythm with reduced effects off: a frame 5 ms after the last is drawn at once.
    let mut off = effects(ReducedEffects::Off, Some(true));
    off.frame_cap = FrameCap::per_second(DEFAULT_MAX_FPS);
    assert_eq!(off.max_fps(), None);
    assert_eq!(off.frame_wait(t0, size, false), None);
    assert_eq!(off.frame_wait(soon, size, false), None, "a GPU, or reduced effects off, never waits");
}

#[test]
fn max_fps_comes_from_the_environment() {
    assert_eq!(max_fps_from_env(Some("0")), Some(0));
    assert_eq!(max_fps_from_env(Some("24")), Some(24));
    assert_eq!(max_fps_from_env(Some(" 60 ")), Some(60));
    for other in [Some("abc"), Some("-5"), Some("100000"), Some(""), None] {
        assert_eq!(max_fps_from_env(other), None, "{other:?}");
    }
}

#[test]
fn the_window_background_is_opaque_so_partial_repaint_can_keep_pixels() {
    use eframe::App as _;
    let app = crate::PdfCraftApp::new();
    for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
        let [.., alpha] = app.clear_color(&visuals);
        assert_eq!(alpha, 1.0, "egui-wgpu repaints everything under a translucent clear colour");
    }
}

#[test]
fn scrolling_frames_come_less_often() {
    let size = egui::vec2(1440.0, 920.0);
    let t0 = Instant::now();
    let mut cap = FrameCap::per_second(30).unwrap().with_scroll_fps(15);
    assert_eq!(cap.scroll_fps(), 15);
    cap.frame_started(t0, size);
    let later = t0 + Duration::from_millis(40);
    assert_eq!(cap.wait(later, size, false), None, "other frames keep the cap's own rate");
    let wait = cap.wait(later, size, true).unwrap();
    assert!(wait > Duration::from_millis(26) && wait < Duration::from_millis(27), "{wait:?}");
    assert_eq!(cap.wait(t0 + Duration::from_millis(67), size, true), None);
    assert_eq!(cap.wait(later, egui::vec2(1000.0, 920.0), true), None, "a resize is drawn at once");

    // No separate rate, or one no lower than the cap's: scrolling keeps the cap's rate.
    for fps in [0, 30, 60] {
        let mut same = FrameCap::per_second(30).unwrap().with_scroll_fps(fps);
        assert_eq!(same.scroll_fps(), 30, "{fps}");
        same.frame_started(t0, size);
        assert_eq!(same.wait(later, size, true), None, "{fps}");
    }

    assert_eq!(scroll_fps_from_env(Some("12")), Some(12));
    assert_eq!(scroll_fps_from_env(Some("0")), Some(0));
    assert_eq!(scroll_fps_from_env(Some("fast")), None);
    assert_eq!(scroll_fps_from_env(None), None);
}

#[test]
fn the_page_view_reports_a_scroll_once() {
    let ctx = egui::Context::default();
    assert!(!take_scrolled(&ctx));
    note_scroll(&ctx);
    assert!(take_scrolled(&ctx));
    assert!(!take_scrolled(&ctx), "asked once a frame");
}

#[test]
fn a_software_rasterizer_leaves_processors_to_drawing_the_window() {
    for (cores, threads) in [(1, 1), (2, 1), (4, 2), (6, 3), (8, 3), (64, 3)] {
        assert_eq!(software_render_threads(cores), threads, "{cores} processors");
    }
    let reduced = effects(ReducedEffects::Auto, Some(true));
    assert_eq!(reduced.render_thread_limit(8), Some(3));
    assert_eq!(effects(ReducedEffects::Auto, Some(false)).render_thread_limit(8), None, "a GPU keeps the engine's choice");
    let asked = Effects { render_threads: Some(5), ..effects(ReducedEffects::Auto, Some(true)) };
    assert_eq!(asked.render_thread_limit(8), Some(5), "the environment decides alone");

    assert_eq!(render_threads_from_env(Some(" 4 ")), Some(4));
    for other in [Some("0"), Some("17"), Some("many"), Some(""), None] {
        assert_eq!(render_threads_from_env(other), None, "{other:?}");
    }
}
