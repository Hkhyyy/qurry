//! Confirms in small windows: the question and the warnings (⚠ Production data source) always show, the details give way;
//! y / Enter never answer a question that doesn't fit
use crate::app::Popup;
use crate::datatools::{copy_confirm_text, fake_confirm_text};
use crate::testkit::Harness;
use crate::ui::MIN_SIZE;

/// Generate data into a Production table with 20 columns (the shape datatools builds)
fn fake_fill() -> String {
    let plan: Vec<_> = (0..20).map(|i| qurry_core::fake::ColPlan { name: format!("col_{i}"), data_type: "text".into(), how: "random values".into() }).collect();
    fake_confirm_text(1000, "public.payment", &plan, &[], Some("[PROD] kr"))
}

/// Copy related rows into a Production source, not masked, with a long table list
fn subset_copy() -> String {
    let tables = (0..25)
        .map(|i| qurry_core::subset::SubsetTable { schema: "public".into(), table: format!("table_{i}"), rows: 100, masked: vec![], reason: "child of customer".into() })
        .collect();
    let plan = qurry_core::subset::SubsetPlan { tables, total: 2500, warning: None };
    copy_confirm_text(&plan, "[PROD] kr", Some("[PROD] kr"), false)
}

fn confirm(h: &mut Harness, text: String) {
    h.app.popup = Some(Popup::Confirm { text, on_yes: Box::new(|a| a.status = "ran".into()) });
}

/// The popup's border is red (the cell left of the " Confirm " title)
fn red_border(h: &mut Harness, w: u16, ht: u16) -> bool {
    let buf = h.buffer(w, ht);
    (0..ht).any(|y| {
        let row: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
        row.find(" Confirm ").is_some_and(|b| {
            let x = row[..b].chars().count() as u16;
            x > 0 && buf[(x - 1, y)].fg == crate::kit::atoms::DANGER
        })
    })
}

#[tokio::test]
async fn production_writes_show_the_question_and_the_warning_at_any_size() {
    let mut h = Harness::new().await;
    for (text, question, warnings) in [
        (fake_fill(), "Insert 1,000 generated rows into", &["⚠ Production data source \"[PROD] kr\""][..]),
        (subset_copy(), "Copy 2,500 rows into \"[PROD] kr\"?", &["⚠ Production data source \"[PROD] kr\"", "⚠ Personal data is copied as is"][..]),
    ] {
        confirm(&mut h, text);
        for (w, ht) in [(80, 24), MIN_SIZE] {
            let s = h.screen(w, ht);
            assert!(s.contains(question) && s.contains("y / Enter yes") && s.contains("lines hidden"), "{w}×{ht}\n{s}");
            // At 40 columns a warning wraps: its start and its name both show
            for warn in warnings {
                let head: String = warn.chars().take(20).collect();
                assert!(s.contains(&head), "{warn} at {w}×{ht}\n{s}");
            }
            assert!(red_border(&mut h, w, ht), "{w}×{ht}\n{s}");
        }
        h.key("y");
        assert_eq!(h.app.status, "ran");
    }
}

/// When even the question and the warnings don't fit, the keys line says how to go on and y / Enter don't answer
#[tokio::test]
async fn a_confirm_that_doesnt_fit_cant_be_answered() {
    let mut h = Harness::new().await;
    let warnings: String = (0..8).map(|i| format!("\n⚠ Column note_{i} has a check constraint the generated values may not meet")).collect();
    confirm(&mut h, format!("Insert 10 generated rows into public.notes?\n{warnings}"));
    let (w, ht) = MIN_SIZE;
    let s = h.screen(w, ht);
    assert!(s.contains("Insert 10 generated") && s.contains("Enlarge the window to answer") && !s.contains("y / Enter"), "{s}");
    h.key("y");
    h.key("Enter");
    assert!(matches!(h.app.popup, Some(Popup::Confirm { .. })) && h.app.status != "ran", "{}", h.app.status);
    assert!(h.app.status.contains("enlarge it"), "{}", h.app.status);
    // Larger: it answers
    assert!(h.screen(100, 40).contains("y / Enter yes"));
    h.key("y");
    assert_eq!(h.app.status, "ran");
    // n / Esc always cancel
    h.app.status.clear();
    confirm(&mut h, format!("Insert 10 generated rows into public.notes?\n{warnings}"));
    h.screen(w, ht);
    h.key("Esc");
    assert!(h.app.popup.is_none() && h.app.status != "ran");
}
