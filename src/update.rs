// flea --update: ask the source that installs this Flea for a newer one, and hand installing to Omarchy.
use crate::terminal;
use std::cmp::Ordering;
use std::process::{Command, Stdio};

// The exit statuses of `flea --update check`; the printed line carries the detail.
pub const AVAILABLE: i32 = 0;
pub const FAILED: i32 = 2;
pub const NOTHING_TO_INSTALL: i32 = 3;

// Omarchy's own menu command for its updater, handed over as a fixed argv and never through a shell.
const UPDATER: &str = "omarchy-update";

// The signed package Omarchy installs from its repository.
const OPR_PACKAGE: &str = "flea";

// The prebuilt release binary's package, which `omarchy update` upgrades or moves to OPR's `flea`.
const RELEASE_PACKAGE: &str = "flea-bin";

// Where the check found this Flea, which decides what may be asked and what `omarchy update` does to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    // OPR's signed `flea`, which `omarchy update` upgrades through pacman.
    Opr,
    // `flea-bin`, a release build no source here describes, which `omarchy update` keeps current.
    Release,
    // Any other package, which is a local build.
    Local,
    // A binary no package owns, which is a cargo build.
    Unowned,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Opr => "opr",
            Kind::Release => "release",
            Kind::Local => "local",
            Kind::Unowned => "unowned",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Available,
    Current,
    // The sentence stderr carries, while stdout still prints the machine line.
    Failed(&'static str),
    // A release or local build, which no source here is asked about.
    Unchecked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub state: State,
    pub kind: Kind,
    pub installed: Option<String>,
    pub latest: Option<String>,
}

impl Report {
    // Sample output: "available opr 0.3.2-1 0.3.3-1", "current opr 0.3.3-1 0.3.3-1", "unchecked local - -".
    pub fn line(&self) -> String {
        let state = match self.state {
            State::Available => "available",
            State::Current => "current",
            State::Failed(_) => "failed",
            State::Unchecked => "unchecked",
        };
        format!("{} {} {} {}", state, self.kind.word(), shown(&self.installed), shown(&self.latest))
    }

    pub fn status(&self) -> i32 {
        match self.state {
            State::Available => AVAILABLE,
            State::Failed(_) => FAILED,
            State::Current | State::Unchecked => NOTHING_TO_INSTALL,
        }
    }
}

// A field the check could not fill prints as one dash, so the line always has four words.
fn shown(version: &Option<String>) -> &str {
    version.as_deref().unwrap_or("-")
}

// flea --update check: print the one line, say a failure on stderr, and exit with its status.
pub fn check() -> i32 {
    let report = investigate();
    println!("{}", report.line());
    if let State::Failed(why) = report.state {
        eprintln!("flea: {}", why);
    }
    report.status()
}

// flea --update: Omarchy's updater in its own floating terminal, the command its menu and its bar run.
pub fn launch() -> i32 {
    let mut presenter = Command::new("omarchy-launch-floating-terminal-with-presentation");
    terminal::detach(&mut presenter);
    match presenter.arg(UPDATER).spawn() {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: Omarchy's updater could not be started, so nothing was updated");
            FAILED
        }
    }
}

fn investigate() -> Report {
    let Some(owner) = owner() else {
        return unchecked(Kind::Unowned, None);
    };
    let Some(described) = run("pacman", &["-Qi", owner.as_str()]).filter(|ran| ran.code == 0) else {
        return failed(kind_for(&owner, false), None, "pacman could not find the package that installed Flea");
    };
    let (installed, signed) = package_info(&described.stdout);
    let installed = installed.filter(|v| valid_version(v));
    match kind_for(&owner, signed) {
        Kind::Opr => from_source(Kind::Opr, installed, repo_latest(&owner)),
        other => unchecked(other, installed),
    }
}

// The package that owns the running binary, which is the one an update would replace.
fn owner() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let ran = run("pacman", &["-Qqo", exe.to_str()?])?;
    if ran.code != 0 {
        return None;
    }
    owner_from(&ran.stdout)
}

// Ok(None) is a source that answered with nothing newer; Err is a source that could not be asked.
fn from_source(kind: Kind, installed: Option<String>, latest: Result<Option<String>, &'static str>) -> Report {
    let latest = match latest {
        Ok(latest) => latest,
        Err(why) => return failed(kind, installed, why),
    };
    let Some(have) = installed.clone() else {
        return failed(kind, None, "the installed Flea reports a version this check cannot compare");
    };
    let Some(offer) = latest else {
        return Report { state: State::Current, kind, installed, latest: None };
    };
    if !valid_version(&offer) {
        return failed(kind, installed, "the package source answered with a version this check cannot compare");
    }
    let order = run("vercmp", &[offer.as_str(), have.as_str()]).filter(|ran| ran.code == 0).and_then(|ran| vercmp_order(&ran.stdout));
    decide(kind, installed, offer, order)
}

// The one decision: a version the owner's source can install, set against the installed one by vercmp.
fn decide(kind: Kind, installed: Option<String>, offer: String, order: Option<Ordering>) -> Report {
    let state = match order {
        Some(Ordering::Greater) => State::Available,
        Some(_) => State::Current,
        None => State::Failed("the two versions could not be compared"),
    };
    Report { state, kind, installed, latest: Some(offer) }
}

fn unchecked(kind: Kind, installed: Option<String>) -> Report {
    Report { state: State::Unchecked, kind, installed, latest: None }
}

fn failed(kind: Kind, installed: Option<String>, why: &'static str) -> Report {
    Report { state: State::Failed(why), kind, installed, latest: None }
}

// checkupdates exits 0 with updates, 2 with none and 1 on an error, and syncs its own database copy.
fn repo_latest(package: &str) -> Result<Option<String>, &'static str> {
    const UPDATES: i32 = 0;
    const NO_UPDATES: i32 = 2;
    let why = "the package mirrors could not be asked for a newer Flea";
    let ran = run("checkupdates", &[]).ok_or(why)?;
    match ran.code {
        UPDATES => Ok(checkupdates_version(&ran.stdout, package)),
        NO_UPDATES => Ok(None),
        _ => Err(why),
    }
}

struct Ran {
    code: i32,
    stdout: String,
}

// A read-only query under the C locale, because pacman translates the field names -Qi prints.
fn run(program: &str, args: &[&str]) -> Option<Ran> {
    let output = crate::backend::locale::command(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    // A child killed by a signal has no exit code, which reads as a failure like any other.
    let code = output.status.code().unwrap_or(-1);
    Some(Ran { code, stdout: String::from_utf8_lossy(&output.stdout).into_owned() })
}

// Sample input: "flea\n"
fn owner_from(text: &str) -> Option<String> {
    let name = text.trim();
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "@._+-".contains(c);
    if name.is_empty() || !name.chars().all(allowed) {
        return None;
    }
    Some(name.to_string())
}

// Sample input: "Name            : flea\nVersion         : 0.3.2-1\nValidated By    : Signature\n"
fn package_info(text: &str) -> (Option<String>, bool) {
    let mut version = None;
    let mut signed = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "Version" => version = Some(value.trim().to_string()),
            // One or more methods separated by two spaces, such as "SHA-256 Sum  Signature".
            "Validated By" => signed = value.split_whitespace().any(|method| method == "Signature"),
            _ => {}
        }
    }
    (version, signed)
}

// Only a signed flea is checked; flea-bin is a prebuilt release, and anything else is somebody's own build.
fn kind_for(owner: &str, signed: bool) -> Kind {
    match owner {
        OPR_PACKAGE if signed => Kind::Opr,
        RELEASE_PACKAGE => Kind::Release,
        _ => Kind::Local,
    }
}

// Sample input: "linux 6.16.8-1 -> 6.16.9-1\nflea 0.3.2-1 -> 0.3.3-1\n"
fn checkupdates_version(text: &str, package: &str) -> Option<String> {
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let [name, _, "->", newer] = words.as_slice() {
            if *name == package {
                return Some(newer.to_string());
            }
        }
    }
    None
}

// pkgver-pkgrel as ^[0-9]+(\.[0-9]+){2}-[0-9]+$, checked before a version reaches vercmp or a screen.
fn valid_version(version: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let Some((pkgver, pkgrel)) = version.split_once('-') else {
        return false;
    };
    let parts: Vec<&str> = pkgver.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| digits(p)) && digits(pkgrel)
}

// Sample input: "1\n", which vercmp prints when its first argument is the newer one.
fn vercmp_order(text: &str) -> Option<Ordering> {
    let value: i64 = text.trim().parse().ok()?;
    Some(value.cmp(&0))
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
