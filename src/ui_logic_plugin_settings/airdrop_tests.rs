use super::super::builtin;
use super::super::pages::{self, Row, Update};
use super::super::wire::{Plugin, Source};
use super::*;
use crate::i18n::Paths;
use crate::ui_logic_send_share::airdrop::Readiness;
use std::path::PathBuf;

fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-st2-airdrop");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

fn airdrop(enabled: bool) -> Plugin {
    let (manifest, on) = builtin::manifests()
        .into_iter()
        .find(|(m, _)| m.id == builtin::AIRDROP)
        .expect("the AirDrop built-in");
    assert!(!on, "AirDrop ships off");
    Plugin {
        manifest,
        source: Source::Builtin,
        enabled,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    }
}

fn ready() -> Page {
    Page {
        receiving: false,
        folder: "/home/u/Downloads".into(),
        shown_as: "minipc".into(),
        adapter: Adapter {
            readiness: Readiness::Ready,
            card: "Intel AX210".into(),
        },
        omdrop: false,
        contacts: false,
    }
}

// Every visible row as one line, so the board's order and strings read top to bottom.
fn lines(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Description { text } => format!("desc {text}"),
            Row::Group { label, switch, .. } => format!("group {label} {switch:?}"),
            Row::Choice {
                label,
                glyph,
                labels,
                selected,
                available,
                ..
            } => {
                format!(
                    "choice {glyph} {label} {} = {selected} {available}",
                    labels.join(" | ")
                )
            }
            Row::Action {
                label,
                glyph,
                value,
                chevron,
                ..
            } => format!("action {glyph} {label} {value} {chevron}"),
            Row::Fact { text, value } => format!("fact {text} {value}").trim_end().to_string(),
            Row::Rule => "rule".into(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn under_omdrop_the_on_choice_names_its_saved_audience_and_no_window_length() {
    let choice = |page: &Page| {
        let rows = rows(&airdrop(true), page, &english());
        lines(&rows).into_iter().find(|l| l.starts_with("choice")).unwrap()
    };
    assert!(choice(&ready()).contains("Off | Everyone for 10 Minutes"));
    let everyone = Page { omdrop: true, ..ready() };
    assert!(choice(&everyone).contains("Off | Everyone = "), "{}", choice(&everyone));
    let contacts = Page { omdrop: true, contacts: true, receiving: true, ..ready() };
    assert!(choice(&contacts).contains("Off | Contacts Only = everyone"), "{}", choice(&contacts));
}

#[test]
fn a_running_receiver_can_be_turned_off_even_when_starting_one_could_not() {
    let choice = |page: &Page| {
        let rows = rows(&airdrop(true), page, &english());
        lines(&rows).into_iter().find(|l| l.starts_with("choice")).unwrap()
    };
    let unready = Adapter { readiness: Readiness::NotReady, card: "Omdrop".into() };
    let running = Page { omdrop: true, receiving: true, adapter: unready.clone(), ..ready() };
    assert!(choice(&running).ends_with("= everyone true"), "{}", choice(&running));
    let stopped = Page { omdrop: true, receiving: false, adapter: unready, ..ready() };
    assert!(choice(&stopped).ends_with("= off false"), "{}", choice(&stopped));
}

#[test]
fn the_page_draws_the_boards_rows_in_order_with_the_credit_before_back() {
    let rows = pages::page_rows_with(&airdrop(true), &Update::Idle, Some(&ready()), &english());
    assert_eq!(
        lines(&rows),
        [
            "desc Share files with iPhones, iPads and Macs nearby.",
            "group AirDrop Some(true)",
            "choice airdrop Receiving Off | Everyone for 10 Minutes = off true",
            "action download Save received files to Downloads true",
            "group This Computer None",
            "fact Shown as minipc",
            "fact Wi-Fi adapter Intel AX210, works with AirDrop",
            "group Access None",
            "fact Reads the files you send",
            "fact Uses Wi-Fi to reach Apple devices nearby",
            "fact Saves the files you accept to the folder above",
            "group Credits None",
            "fact Built on owly by Tristan (@t4t5) MIT",
            "rule",
            "action  Back to Plugins Back true",
        ]
    );
}

#[test]
fn receiving_waits_for_the_switch_and_an_able_adapter_and_shows_the_window_in_effect() {
    let off = pages::page_rows_with(&airdrop(false), &Update::Idle, Some(&ready()), &english());
    assert!(lines(&off).contains(
        &"choice airdrop Receiving Off | Everyone for 10 Minutes = off false".to_string()
    ));
    let unable = Page {
        adapter: Adapter {
            readiness: Readiness::Broken,
            card: "Intel 8265".into(),
        },
        ..ready()
    };
    let rows = lines(&pages::page_rows_with(
        &airdrop(true),
        &Update::Idle,
        Some(&unable),
        &english(),
    ));
    assert!(rows.contains(
        &"choice airdrop Receiving Off | Everyone for 10 Minutes = off false".to_string()
    ));
    assert!(rows.contains(&"fact Wi-Fi adapter Intel 8265, cannot do AirDrop".to_string()));
    let missing = Page {
        adapter: Adapter {
            readiness: Readiness::Missing,
            card: String::new(),
        },
        ..ready()
    };
    let rows = lines(&pages::page_rows_with(
        &airdrop(true),
        &Update::Idle,
        Some(&missing),
        &english(),
    ));
    assert!(rows.contains(&"fact Wi-Fi adapter Needs the owly package".to_string()));
    let on = Page {
        receiving: true,
        ..ready()
    };
    let rows = lines(&pages::page_rows_with(
        &airdrop(true),
        &Update::Idle,
        Some(&on),
        &english(),
    ));
    assert!(rows.contains(
        &"choice airdrop Receiving Off | Everyone for 10 Minutes = everyone true".to_string()
    ));
}

#[test]
fn without_the_windows_facts_or_on_another_plugin_the_generic_page_stands() {
    let plain = lines(&pages::page_rows(&airdrop(true), &Update::Idle, &english()));
    assert!(!plain
        .iter()
        .any(|l| l.contains("Receiving") || l.contains("Credits")));
    let (shelf, _) = builtin::manifests()
        .into_iter()
        .find(|(m, _)| m.id == "flea.localsend")
        .unwrap();
    let other = Plugin {
        manifest: shelf,
        ..airdrop(true)
    };
    let rows = lines(&pages::page_rows_with(
        &other,
        &Update::Idle,
        Some(&ready()),
        &english(),
    ));
    assert!(!rows
        .iter()
        .any(|l| l.contains("Receiving") || l.contains("Credits")));
}
