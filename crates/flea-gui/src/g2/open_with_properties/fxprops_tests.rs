//! S2-7 and S2-2: a file's Properties names its photo and codec facts, and a read-only Permissions card names no typing error.
use super::keys::Key;
use super::permissions::Permissions;
use super::properties::Properties;
use super::shell::{Block, Card, Fact, Ink};
use super::tests::{english, notes, sent};
use flea::jsondoc::parse;

const PHOTO: &str = r#"{"make":"Canon","model":"Canon EOS R6","lens":"RF35mm F1.8 MACRO IS STM","exp":[1,250],"fnum":[28,10],"iso":400,"focal":[35,1],"taken":"2026:09:12 18:31:00","w":6000,"h":4000}"#;

fn file(photo: &str, kind: &str) -> String {
    format!(
        r#"{{"t":"menuaction","op":"properties","id":9,"ok":true,"path":"/fx/raw/L1000921.DNG","kind":"{kind}","directory":false,"symlink":false,"target":"","bytes":24117248,"modified":1758235800,"mode":"0644","owner":"gm","uid":1000,"gid":1000,"tags":[],"photo":{photo}}}"#
    )
}

fn rows(card: &Card) -> Vec<(String, String)> {
    card.body
        .iter()
        .flat_map(|block| match block {
            Block::Facts(facts) => facts.as_slice(),
            _ => &[],
        })
        .filter_map(|fact| match fact {
            Fact::Text { name, value, .. } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect()
}

fn names(card: &Card) -> String {
    rows(card)
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(",")
}

fn opened(first: &str) -> (Properties, Vec<String>) {
    let (mut card, _) = Properties::open(9, 1, &[0], 0);
    let ack = parse(r#"{"t":"menuaction","op":"snapshot","id":9,"ok":true}"#).unwrap();
    card.reply(&english(), &ack);
    let asked = sent(&card.reply(&english(), &parse(first).unwrap()));
    (card, asked)
}

#[test]
fn fxprops_s2_7_a_photos_properties_name_camera_lens_exposure_and_taken_before_modified() {
    let i18n = english();
    let (mut card, asked) = opened(&file(PHOTO, "File"));
    assert_eq!(
        asked,
        [r#"{"c":"menuaction","op":"facts","id":9}"#],
        "a file asks its codecs and deferred EXIF on the card's own id"
    );
    let expected = [
        ("Camera", "Canon EOS R6"),
        ("Lens", "RF35mm F1.8 MACRO IS STM"),
        ("Exposure", "1/250 s \u{b7} f/2.8 \u{b7} ISO 400"),
        ("Taken", "2026-09-12 18:31"),
    ];
    let first = card.card(&i18n);
    assert_eq!(
        names(&first),
        "Path,Kind,Size,Camera,Lens,Exposure,Taken,Modified,Permissions,Owner,Group",
        "the photo lines sit between Size and Modified, as v0.3.12 drew them"
    );
    for (name, value) in expected {
        assert!(
            rows(&first).contains(&(name.into(), value.into())),
            "{name} reads {value} from the properties reply's own photo"
        );
    }
    let facts = r#"{"t":"menuaction","op":"facts","id":9,"ok":true,"vcodec":"","acodec":"","bps":0,"make":"Canon","model":"Canon EOS R6","lens":"RF35mm F1.8 MACRO IS STM","shutter":[1,250],"fnumber":[28,10],"iso":400,"taken":"2026:09:12 18:31:00"}"#;
    assert!(card.reply(&i18n, &parse(facts).unwrap()).is_empty());
    assert_eq!(
        card.card(&i18n),
        first,
        "a facts reply that agrees moves nothing"
    );
}

#[test]
fn fxprops_s2_7_a_videos_facts_reply_adds_codecs_and_bit_rate() {
    let i18n = english();
    let (mut card, _) = opened(&file("null", "File"));
    assert_eq!(
        names(&card.card(&i18n)),
        "Path,Kind,Size,Modified,Permissions,Owner,Group"
    );
    let facts = r#"{"t":"menuaction","op":"facts","id":9,"ok":true,"vcodec":"h264","acodec":"aac","bps":8100000,"make":"","model":"","lens":"","shutter":[0,0],"fnumber":[0,0],"iso":0,"taken":""}"#;
    card.reply(&i18n, &parse(facts).unwrap());
    let drawn = card.card(&i18n);
    assert_eq!(
        names(&drawn),
        "Path,Kind,Size,Codecs,Bit rate,Modified,Permissions,Owner,Group"
    );
    assert!(rows(&drawn).contains(&("Codecs".into(), "H.264 \u{b7} AAC".into())));
    assert!(rows(&drawn).contains(&("Bit rate".into(), "8.1 Mb/s".into())));
}

#[test]
fn fxprops_s2_7_a_refused_or_late_facts_reply_keeps_the_photo_and_adds_nothing() {
    let i18n = english();
    let (mut card, _) = opened(&file(PHOTO, "File"));
    let before = card.card(&i18n);
    for refused in [
        r#"{"t":"menuaction","op":"facts","id":9,"ok":false,"error":"Only a file has codecs or photo facts."}"#,
        r#"{"t":"menuaction","op":"facts","id":9,"ok":true,"cancelled":true}"#,
        r#"{"t":"menuaction","op":"facts","id":10,"ok":true,"vcodec":"h264","acodec":"","bps":0,"make":"","model":"","lens":"","shutter":[0,0],"fnumber":[0,0],"iso":0,"taken":""}"#,
    ] {
        card.reply(&i18n, &parse(refused).unwrap());
        assert_eq!(card.card(&i18n), before, "{refused}");
    }
    card.key(Key::Escape);
    let late = r#"{"t":"menuaction","op":"facts","id":9,"ok":true,"vcodec":"h264","acodec":"","bps":0,"make":"","model":"","lens":"","shutter":[0,0],"fnumber":[0,0],"iso":0,"taken":""}"#;
    assert!(card.reply(&i18n, &parse(late).unwrap()).is_empty());
    assert_eq!(
        card.card(&i18n),
        before,
        "a closed card takes no Codecs row"
    );
}

#[test]
fn fxprops_s2_7_folders_links_and_special_files_never_ask_for_media_facts() {
    let i18n = english();
    for first in [
        r#"{"t":"menuaction","op":"properties","id":9,"ok":true,"path":"/fx/v/work","kind":"Directory","directory":true,"symlink":false,"bytes":4096,"photo":null}"#.to_string(),
        file("null", "Symbolic link").replace(r#""symlink":false"#, r#""symlink":true"#),
        file("null", "Special file"),
    ] {
        let (_, asked) = opened(&first);
        assert!(
            !asked.iter().any(|line| line.contains(r#""op":"facts""#)),
            "{first}"
        );
    }
    let (card, _) = opened(&file(PHOTO, "Symbolic link"));
    assert!(!names(&card.card(&i18n)).contains("Camera"));
}

const SETUID: &str = r#"{"t":"permissions","id":7,"op":"inspect","ok":true,"path":"/fx/v/work/tool","directory":false,"mode":"4644","uid":1000,"gid":1000,"owner":"gm","group":"gm","reason":"Read-only: setuid bit is present."}"#;

#[test]
fn fxprops_s2_2_a_read_only_setuid_card_names_its_reason_and_no_typing_error() {
    let i18n = english();
    let (mut card, _) = Permissions::open(7, vec!["/fx/v/work/tool".into()], "");
    card.reply(&i18n, &parse(SETUID).unwrap());
    let drawn = card.card(&i18n);
    let invalid = i18n.tr("dialogs.file.permissions.octal_invalid");
    assert!(
        !notes(&drawn).iter().any(|(text, _)| *text == invalid),
        "{:?}",
        notes(&drawn)
    );
    assert!(notes(&drawn).contains(&("Read-only: setuid bit is present.".into(), Ink::Error)));
    let octal = drawn
        .body
        .iter()
        .find_map(|block| match block {
            Block::Stack(parts) => parts.iter().find_map(|part| match part {
                Block::Field(field) => Some(field.value.clone()),
                _ => None,
            }),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        octal, "4644",
        "the four-digit mode shows as the file has it"
    );
    for typed in [Key::Tab, Key::Backspace] {
        card.key(typed);
    }
    assert_eq!(card.card(&i18n), drawn, "a read-only card takes no typing");
}

#[gpui::test]
fn fxprops_s2_3_the_launch_read_key_draws_the_defaults_line(cx: &mut gpui::TestAppContext) {
    use crate::window::settings::store::Update;
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    entity.update_in(cx, |shell, window, cx| {
        shell.receive_settings(Update::Notice("settings.error.unreadable"), window, cx);
        assert_eq!(
            shell.settings_message(cx).as_deref(),
            Some("Your saved settings could not be read, so these are the defaults.")
        );
    });
}

#[test]
fn fxprops_s2_2_an_editable_card_still_names_a_typing_error() {
    let i18n = english();
    let mut card = super::tests::permissions();
    let invalid = i18n.tr("dialogs.file.permissions.octal_invalid");
    // Apply holds focus at open; two steps back reach the octal field past Cancel.
    for key in [Key::Backtab, Key::Backtab, Key::Text("9".into())] {
        card.key(key);
    }
    assert!(notes(&card.card(&i18n)).contains(&(invalid, Ink::Error)));
}

fn summary(card: &Card) -> Vec<String> {
    card.body
        .iter()
        .find_map(|block| match block {
            Block::Stack(parts) => parts.iter().find_map(|part| match part {
                Block::Section { lines, .. } => Some(lines.clone()),
                _ => None,
            }),
            _ => None,
        })
        .unwrap()
}

// v0.3.12 PermissionsDialog.qml:602-606: a card that cannot change the mode says so and names the mode it keeps.
#[test]
fn fxpolish_a_read_only_card_requests_no_mode() {
    let i18n = english();
    let (mut card, _) = Permissions::open(7, vec!["/fx/v/work/tool".into()], "");
    card.reply(&i18n, &parse(SETUID).unwrap());
    assert_eq!(
        summary(&card.card(&i18n)),
        [
            "No changes to apply",
            "Path /fx/v/work/tool",
            "Current mode 4644",
            "Scope this item only \u{b7} ownership unchanged",
        ]
    );
    let editable = SETUID
        .replace(r#""mode":"4644""#, r#""mode":"0644""#)
        .replace("Read-only: setuid bit is present.", "");
    let (mut card, _) = Permissions::open(7, vec!["/fx/v/work/tool".into()], "");
    card.reply(&i18n, &parse(&editable).unwrap());
    assert_eq!(
        summary(&card.card(&i18n))[..2],
        ["Requested mode 0644", "Path /fx/v/work/tool"],
        "an editable card still names the mode Apply sets"
    );
}
