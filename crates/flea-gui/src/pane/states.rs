//! State facts for the theme-owned hero producer, translated before any paint or cache lookup.
use flea::i18n::I18n;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Loading,
    Ready,
    Empty,
    Locked,
    Error,
    Waiting,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Locked => "locked",
            Self::Error => "error",
            Self::Waiting => "waiting",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub kind: Kind,
    pub target: String,
    pub mode: u32,
    pub beat: usize,
    pub query: String,
    pub search: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub message: String,
    pub detail: String,
    pub status: String,
}

impl State {
    pub fn text(&self, catalog: &I18n, total: usize) -> Text {
        let key = match self.kind {
            Kind::Loading => "pane.reading",
            Kind::Locked => "pane.locked",
            Kind::Error => "pane.unread",
            Kind::Waiting => "window.waiting",
            Kind::Ready => "",
            Kind::Empty => match self.beat % 4 {
                0 => "pane.empty.initial",
                1 => "pane.empty.beat_one",
                2 => "pane.empty.beat_two",
                _ => "pane.empty.rest",
            },
        };
        let message = if key.is_empty() {
            String::new()
        } else {
            catalog.tr(key)
        };
        let detail = if self.kind == Kind::Locked && self.mode > 0 {
            let mut permissions = String::with_capacity(9);
            for (bit, letter) in [
                (0o400, 'r'),
                (0o200, 'w'),
                (0o100, 'x'),
                (0o040, 'r'),
                (0o020, 'w'),
                (0o010, 'x'),
                (0o004, 'r'),
                (0o002, 'w'),
                (0o001, 'x'),
            ] {
                permissions.push(if self.mode & bit != 0 { letter } else { '-' });
            }
            for (bit, execute, at, set, unset) in [
                (0o4000, 0o100, 2, "s", "S"),
                (0o2000, 0o010, 5, "s", "S"),
                (0o1000, 0o001, 8, "t", "T"),
            ] {
                if self.mode & bit != 0 {
                    permissions.replace_range(
                        at..at + 1,
                        if self.mode & execute != 0 { set } else { unset },
                    );
                }
            }
            if self.mode & 0o500 == 0o500 {
                catalog.tr_args("pane.permissions_other_owner", &[&permissions])
            } else {
                permissions
            }
        } else {
            String::new()
        };
        let status = if matches!(self.kind, Kind::Ready | Kind::Empty) {
            catalog.trn("items.count", total as i64)
        } else if matches!(self.kind, Kind::Locked | Kind::Error) {
            catalog.tr("pane.unavailable")
        } else {
            message.clone()
        };
        Text {
            message,
            detail,
            status,
        }
    }

    pub fn locked_target(&self) -> Option<&str> {
        (self.kind == Kind::Locked).then_some(self.target.as_str())
    }

    /// One caption beat; the words loop while the pane is Empty and drawn, and nothing advances otherwise.
    pub fn empty_beat(&mut self, reduced_motion: bool, shown: bool) -> bool {
        if self.kind != Kind::Empty || reduced_motion || !shown {
            return false;
        }
        self.beat = (self.beat + 1) % 4;
        true
    }
}

pub fn hero_span(slot_width: f32, active_x: f32, natural_width: f32) -> (f32, f32) {
    let start = if slot_width / 2. - natural_width / 2. < active_x {
        active_x
    } else {
        0.
    };
    (start, (slot_width - start).max(0.))
}
