// The Settings About pane's rows: what is installed, the one updater and where help lives, from v0.3.12's ui/js/SettingsAbout.js.
use super::rows::{Kind, Row};
use super::update::{self, Status};
use super::Text;

pub const ISSUES_URL: &str = "https://github.com/thisisgm/flea/issues";
pub const SPONSORS_URL: &str = "https://github.com/sponsors/thisisgm";

// The signed repository package, src/update.rs's OPR_PACKAGE.
pub const OPR_PACKAGE: &str = "flea";
// The prebuilt release package, src/update.rs's RELEASE_PACKAGE.
pub const RELEASE_PACKAGE: &str = "flea-bin";

// Each kind word src/update.rs prints, with the key of the sentence About shows for it.
pub const SOURCES: [(&str, &str); 4] = [
    ("opr", "settings.about.source_opr"),
    ("local", "settings.about.source_local"),
    // flea-bin, a prebuilt release a package helper installed, so it too is a local package.
    ("release", "settings.about.source_local"),
    ("unowned", "settings.about.source_unowned"),
];

// The installed build as About reads it. Empty text is not reported. Package None is a query that did not run, Some("") no owner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub version: String,
    pub built: String,
    pub source: Text,
    pub package: Option<String>,
    pub handler: String,
    pub update: Status,
}

// The fields of pacman -Qi that About reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageFacts {
    pub package: String,
    pub built: String,
    pub signed: bool,
}

// The pane's rows in order. The caller supplies the make-default rows, which another unit owns.
pub fn rows(facts: &Facts, data_auto_check: bool, language_name: &str, make_default: Vec<Row>) -> Vec<Row> {
    let mut out = vec![
        Row { value: Text::Key("settings.about.tagline"), ..Row::new(Kind::Hero, Text::data("Flea")) },
        Row::fact("settings.about.version", or_key(&facts.version, "settings.about.not_reported")),
        Row::fact("settings.about.built", or_key(&facts.built, "settings.about.not_recorded")),
        Row::fact("settings.about.installed_from", text_or(&facts.source, "settings.about.not_reported")),
        Row::fact("settings.about.package", package_text(facts.package.as_deref())),
        Row::fact("settings.about.license", Text::Key("settings.about.license_text")),
        Row::group("settings.about.group_language"),
        Row::action("language", "settings.about.language", "globe", Text::data(language_name)),
        Row::group("settings.about.group_updates"),
    ];
    out.extend(update_rows(&facts.update, data_auto_check));
    out.push(Row::group("settings.about.group_this_computer"));
    out.push(Row {
        glyph: "folder",
        elide_head: true,
        ..Row::fact("settings.about.file_manager", or_key(&facts.handler, "settings.about.not_reported"))
    });
    out.extend(make_default);
    out.push(Row::action("keyboardSheet", "settings.about.keyboard_sheet", "keyboard", Text::data("?")));
    out.push(Row {
        url: ISSUES_URL,
        ..Row::action("reportIssue", "settings.about.report_issue", "network", Text::Key("settings.about.open"))
    });
    out.push(Row {
        url: SPONSORS_URL,
        ..Row::action("support", "settings.about.support", "", Text::data("GitHub Sponsors"))
    });
    out
}

// Update Flea and the switch that governs automatic checks, then the note when the state needs one.
fn update_rows(update: &Status, data_auto_check: bool) -> Vec<Row> {
    let kind = if update::is_fact(update) { Kind::Fact } else { Kind::Action };
    let mut out = vec![
        Row {
            id: "updateFlea".to_owned(),
            glyph: "download",
            value: update::value(update),
            role: update::role(update),
            inert: !update::acts(update),
            ..Row::new(kind, Text::Key("settings.about.update_flea"))
        },
        Row {
            caption: Text::Key("settings.about.check_automatically_caption"),
            ..Row::check("updates.autoCheck", "settings.about.check_automatically", "", data_auto_check)
        },
    ];
    if let Some((text, role)) = update::note(update) {
        out.push(Row { role, ..Row::hint(text) });
    }
    out
}

// On until switched off, the default src/uischema.rs ships for updates.autoCheck.
pub fn auto_check(stored: Option<bool>) -> bool {
    stored.unwrap_or(true)
}

fn or_key(value: &str, key: &'static str) -> Text {
    if value.is_empty() { Text::Key(key) } else { Text::data(value) }
}

fn text_or(value: &Text, key: &'static str) -> Text {
    if value.is_empty() { Text::Key(key) } else { value.clone() }
}

fn package_text(package: Option<&str>) -> Text {
    match package {
        None => Text::Key("settings.about.not_reported"),
        Some("") => Text::Key("settings.about.not_owned"),
        Some(name) => Text::data(name),
    }
}

// src/update.rs kind_for over pacman -Qqo's owner, "" when no package owns Flea.
pub fn install_kind(owner: &str, signed: bool) -> &'static str {
    if owner.is_empty() {
        "unowned"
    } else if owner == OPR_PACKAGE && signed {
        "opr"
    } else if owner == RELEASE_PACKAGE {
        "release"
    } else {
        "local"
    }
}

// The sentence for the kind install_kind derives, so this row and the Update Flea row name one source.
pub fn installed_from(owner: &str, signed: bool) -> Text {
    let kind = install_kind(owner, signed);
    SOURCES
        .iter()
        .find(|&&(word, _)| word == kind)
        .map_or(Text::Empty, |&(_, key)| Text::Key(key))
}

// pacman -Qi under LC_ALL=C, read the way src/update.rs package_info reads it.
pub fn package_facts(text: &str) -> PackageFacts {
    let mut facts = PackageFacts::default();
    for line in text.split('\n') {
        let Some(cut) = line.find(':') else { continue };
        let key = line[..cut].trim();
        let value = line[cut + 1..].trim();
        match key {
            "Name" => facts.package = value.to_owned(),
            "Version" if facts.package.is_empty() => facts.package = value.to_owned(),
            "Version" => facts.package = format!("{} {}", facts.package, value),
            "Build Date" => facts.built = value.to_owned(),
            // One or more methods separated by two spaces, such as "SHA-256 Sum  Signature".
            "Validated By" => facts.signed = value.split_whitespace().any(|method| method == "Signature"),
            _ => {}
        }
    }
    facts
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
