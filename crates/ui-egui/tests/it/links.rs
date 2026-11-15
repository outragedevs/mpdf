//! The About dialog, and the absence of the original publisher's links and marks.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

/// This fork ships no links to the original publisher: not in the top bar, not on the home screen,
/// and not as Help commands.
#[test]
fn the_interface_has_no_publisher_links() {
    let mut h = harness(|_| {});
    for label in ["Discord", "Join our Discord", "Join the ArtCraft community", "PdfCraft web page", "PdfCraft on GitHub", "ArtCraft website"] {
        assert!(h.query_by_label(label).is_none(), "{label} is still shown");
    }
    for command in ["help.discord", "help.app_page", "help.github", "help.website", "help.check_updates"] {
        assert!(pdfcraft_engine::commands::command(command).is_none(), "{command} is still a command");
        assert!(!h.state_mut().execute(command), "{command} still runs");
    }
    assert_eq!(h.state().last_opened_url, None, "nothing was opened");
}

/// About names where the app comes from in plain text, and shows none of the original's marks.
#[test]
fn about_dialog_says_what_it_is_based_on_without_the_marks() {
    let h = harness(|app| app.dialog = Some(Dialog::About));
    h.get_by_label("Based on PdfCraft by the ArtCraft team.");
    assert_eq!(h.query_all_by_label("ArtCraft").count(), 0, "a mark or wordmark (alt text) is still shown");
    assert!(h.query_by_label("Join our Discord").is_none());
}

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, PdfCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_by_label(label).map(|n| n.rect());
        if now.is_some() && now == last {
            return h.get_by_label(label);
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

#[test]
fn about_dialog_has_contributors_and_models_tabs() {
    let mut h = harness(|app| app.dialog = Some(Dialog::About));
    settled(&mut h, "Contributors").click();
    // The owner is always in the compiled-in credits (contributors/contributors.json), shown by username.
    settled(&mut h, "@echelon");
    settled(&mut h, "Table").click();
    settled(&mut h, "PRs");
    settled(&mut h, "Display name").click();
    settled(&mut h, "Brandon Thomas");
    settled(&mut h, "Models").click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Anthropic").count() >= 1);
}
