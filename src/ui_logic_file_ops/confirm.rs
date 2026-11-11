// The permanent-delete card's wire: the session it answers to, the identity check, refusals, and a fresh review when stale.
use super::collide::{text, Request};
use super::lines;
use super::trash::{self, Review, Scope};
use super::Reply;
use crate::i18n::I18n;

// An open confirmation re-checks its reviewed identities this often while nothing is out (ui/TrashView.qml:468).
pub const CHECK_MS: u64 = 2500;

// The Trash worker's answer to a request a later one replaced (src/backend/trashmanifest.rs:32); the menu's carries "cancelled":true.
pub const TRASH_CANCELLED: &str = "Trash request cancelled.";

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    // Delete permanently from the menu on a writable place without Trash; the id is the menu snapshot's.
    Menu(u64),
    // The Trash view's delete or Empty Trash; the prepare fields as the view sent them, resent with refreshToken when stale.
    Trash(Request),
}

// What a reply did to the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    // Not this card's reply.
    Ignored,
    // Taken, nothing drawn changes.
    Kept,
    // A fresh review landed: the card is drawn again from it.
    Shown,
    // The review went stale: the card hides and this line asks for a fresh one.
    Refresh(String),
    // A later request replaced this one: nothing is said; send the line if any and drop the card.
    Dropped(Option<String>),
    // Refused: say the error, send the close line if any, and drop the card.
    Refused {
        error: String,
        close: Option<String>,
    },
    // Deleted: send the close line if any, say the line if any, and drop the card.
    Deleted {
        close: Option<String>,
        said: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Confirm {
    pub source: Source,
    pub scope: Scope,
    pub review: Review,
    // The card is drawn; false while a delete or a fresh review is out.
    pub shown: bool,
    pending: Option<(u64, &'static str)>,
    check_at: u64,
}

impl Confirm {
    pub fn new(source: Source, scope: Scope, review: Review, now: u64) -> Self {
        Self {
            source,
            scope,
            review,
            shown: true,
            pending: None,
            check_at: now + CHECK_MS,
        }
    }

    // The request id a delete went out under, for the Trash's trashdeletestarted.requestId.
    pub fn deleting(&self) -> Option<u64> {
        self.pending
            .filter(|(_, op)| *op == "delete")
            .map(|(id, _)| id)
    }

    fn trash_line(next: &mut u64, op: &str, rest: &str) -> (u64, String) {
        *next += 1;
        (
            *next,
            format!(
                "{{\"c\":\"trashbrowse\",\"op\":\"{op}\",\"id\":{}{rest}}}",
                *next
            ),
        )
    }

    fn menu_line(id: u64, op: &str, rest: &str) -> String {
        format!("{{\"c\":\"menuaction\",\"op\":\"{op}\",\"id\":{id}{rest}}}")
    }

    // Cancel, Escape or a press outside: the Trash session forgets its confirmation, the menu snapshot closes.
    pub fn cancel(&self, next: &mut u64) -> String {
        match &self.source {
            Source::Trash(_) => Self::trash_line(next, "cancel", "").1,
            Source::Menu(id) => Self::menu_line(*id, "close", ""),
        }
    }

    // Delete hides the card at once, so the reviewed token goes out once and is never sent again.
    pub fn delete(&mut self, next: &mut u64, running: bool) -> Option<String> {
        if !self.shown || !trash::delete_enabled(&self.review, running) {
            return None;
        }
        self.shown = false;
        let rest = format!(",\"token\":{}", self.review.token);
        Some(match &self.source {
            Source::Trash(_) => {
                let (id, line) = Self::trash_line(next, "delete", &rest);
                self.pending = Some((id, "delete"));
                line
            }
            Source::Menu(id) => {
                self.pending = Some((*id, "delete"));
                Self::menu_line(*id, "delete", &rest)
            }
        })
    }

    pub fn is_trash(&self) -> bool {
        matches!(self.source, Source::Trash(_))
    }

    // Seam: a Trash request of this card is out, so the Trash view's own requests wait as v0.3.12's one busy does.
    pub fn trash_out(&self) -> bool {
        self.is_trash() && self.pending.is_some()
    }

    // The Trash view's busy cleared: a full interval runs again before the next check, as v0.3.12's Timer restarts.
    pub fn restart(&mut self, now: u64) {
        self.check_at = now + CHECK_MS;
    }

    // When the next identity check is due, if one can go out.
    pub fn due(&self) -> Option<u64> {
        (self.shown && self.pending.is_none()).then_some(self.check_at)
    }

    pub fn tick(&mut self, now: u64, next: &mut u64) -> Option<String> {
        if !self.due().is_some_and(|at| now >= at) {
            return None;
        }
        let rest = format!(",\"token\":{}", self.review.token);
        Some(match &self.source {
            Source::Trash(_) => {
                let (id, line) = Self::trash_line(next, "check", &rest);
                self.pending = Some((id, "check"));
                line
            }
            Source::Menu(id) => {
                self.pending = Some((*id, "checkDelete"));
                Self::menu_line(*id, "checkDelete", &rest)
            }
        })
    }

    fn refresh(&mut self, next: &mut u64) -> String {
        self.shown = false;
        match &self.source {
            Source::Trash(fields) => {
                *next += 1;
                let line = fields
                    .clone()
                    .with("c", &text("trashbrowse"))
                    .with("op", &text("prepare"))
                    .with("id", &next.to_string())
                    .with(
                        "refreshToken",
                        if self.review.token.is_empty() {
                            "0"
                        } else {
                            &self.review.token
                        },
                    )
                    .json();
                self.pending = Some((*next, "prepare"));
                line
            }
            Source::Menu(id) => {
                self.pending = Some((*id, "refreshDelete"));
                Self::menu_line(*id, "refreshDelete", "")
            }
        }
    }

    // Sample input: {"t":"trashbrowse","id":2,"op":"delete","ok":false,"stale":true,"error":"Trash changed; confirm again."}
    pub fn reply(
        &mut self,
        catalog: &I18n,
        trash_reply: bool,
        r: &Reply,
        now: u64,
        next: &mut u64,
    ) -> Answer {
        let menu = match self.source {
            Source::Menu(id) => Some(id),
            Source::Trash(_) => None,
        };
        if trash_reply != menu.is_none() || self.pending != Some((r.id, op_name(&r.op))) {
            return Answer::Ignored;
        }
        self.pending = None;
        self.check_at = now + CHECK_MS;
        let close = menu.map(|id| Self::menu_line(id, "close", ""));
        // On the shared counter a later id means the Trash view's own request replaced this one.
        let replaced = menu.is_none() && *next > r.id;
        if !r.ok && (r.cancelled || r.error == TRASH_CANCELLED || replaced) {
            // v0.3.12 drops a superseded reply and sends nothing; the menu's close is a no-op for a superseded snapshot.
            return match r.op.as_str() {
                "check" | "checkDelete" => Answer::Kept,
                _ => Answer::Dropped(close),
            };
        }
        if !r.ok && !(r.stale && menu.is_none()) {
            let fallback = if menu.is_some() {
                "jobs.delete.failed"
            } else {
                "jobs.delete.failed_trash"
            };
            let error = if r.error.is_empty() {
                catalog.tr(fallback)
            } else {
                r.error.clone()
            };
            return Answer::Refused { error, close };
        }
        if r.stale || (r.op == "check" || r.op == "checkDelete") && !r.valid {
            return Answer::Refresh(self.refresh(next));
        }
        match r.op.as_str() {
            "prepare" | "refreshDelete" => {
                self.review = Review {
                    count: Some(r.count),
                    bytes: r.bytes,
                    all: if trash_reply { r.all } else { self.review.all },
                    token: r.token.clone(),
                    stale: false,
                };
                self.shown = true;
                Answer::Shown
            }
            "delete" => {
                let said = menu.map(|_| {
                    lines::deleted_line(
                        catalog,
                        r.deleted,
                        self.review.count.unwrap_or(0),
                        r.failed,
                        r.cancelled,
                    )
                });
                Answer::Deleted { close, said }
            }
            _ => Answer::Kept,
        }
    }
}

// The ops this card sends, so a reply's op is matched against what went out and nothing else.
fn op_name(op: &str) -> &'static str {
    match op {
        "delete" => "delete",
        "check" => "check",
        "checkDelete" => "checkDelete",
        "prepare" => "prepare",
        "refreshDelete" => "refreshDelete",
        _ => "",
    }
}
