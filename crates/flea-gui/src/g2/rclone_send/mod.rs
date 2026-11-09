//! Send with rclone (#176): the menu row and its remotes, the folder card, each send's transfer-card block and its receipt.
//! The wire and every word come from core `flea::ui_logic_rclone_send`; drawing the menu, card and job blocks belongs to their hosts.
pub(crate) mod join;
pub mod seams;
pub mod sweep;

use flea::i18n::I18n;
use flea::ui_logic_rclone_send::{
    folder_check, present, remotes_line, send_line, wire, Event, Phase, Send, MENU_ID,
};
use seams::{Card, FlyoutRow, MenuRow, Wire};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use sweep::Sweep;

/// The card while it is open: the remote chosen in the flyout, the selection it captured and the field.
#[derive(Clone, Debug)]
struct Open {
    remote: String,
    paths: Vec<String>,
    folder: String,
    selected_all: bool,
    /// The request this card sent, until the backend answers it.
    asked: Option<u64>,
    /// The last request this card sent, whose refusal it shows; a refusal for any other request is dropped.
    sent: Option<u64>,
}

/// Every block's words, built once per change and per language rather than on each frame a check sweeps.
type Blocks = Rc<Vec<(u64, present::Block)>>;

/// One pane backend's rclone send state: the window keeps one per pane backend, since each numbers its jobs from 1; every reply is matched to the request that asked for it.
#[derive(Default)]
pub struct RcloneSend {
    next: u64,
    /// The remotes request the open menu is waiting on; an older answer is stale.
    asking: Option<u64>,
    remotes: Vec<String>,
    visible: bool,
    flyout_open: bool,
    /// The folder last sent to on each remote, kept until Flea closes.
    folders: HashMap<String, String>,
    card: Option<Open>,
    cancel_on_start: HashSet<u64>,
    /// Newest first, as the card stacks jobs; each is keyed by its own request id, which never repeats.
    sends: Vec<Send>,
    sweeps: HashMap<u64, Rc<Sweep>>,
    /// Bumped on every change to the sends, so the cached blocks know when to rebuild.
    generation: u64,
    cache: RefCell<Option<(u64, String, Blocks)>>,
    /// Times the blocks were built, for tests and the headless gate.
    pub builds: Cell<usize>,
}

impl RcloneSend {
    fn request_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// The file menu opened: ask for the remotes now, as Taildrop asks for its machines; nothing runs before a menu opens.
    pub fn menu_opened(&mut self, wire: &dyn Wire) {
        let id = self.request_id();
        self.asking = wire.send(remotes_line(id)).then_some(id);
        self.flyout_open = false;
    }

    pub fn menu_closed(&mut self) {
        self.asking = None;
        self.flyout_open = false;
    }

    /// The row, only while rclone answered with a remote and Settings > Menus leaves it on.
    pub fn menu_row(&self, i18n: &I18n, hidden: &[String]) -> Option<MenuRow> {
        if !self.visible || self.remotes.is_empty() || hidden.iter().any(|id| id == MENU_ID) {
            return None;
        }
        Some(MenuRow {
            id: MENU_ID,
            label: present::row_label(i18n),
            mark: "cloud-upload",
            chevron: true,
            lit: self.flyout_open,
        })
    }

    pub fn open_flyout(&mut self, open: bool) {
        self.flyout_open = open && self.visible && !self.remotes.is_empty();
    }

    pub fn flyout(&self) -> Vec<FlyoutRow> {
        self.remotes
            .iter()
            .map(|name| FlyoutRow {
                label: name.clone(),
                mark: "server",
            })
            .collect()
    }

    /// A remote chosen in the flyout opens the card over the captured selection's absolute paths.
    pub fn choose(&mut self, remote: &str, paths: Vec<String>) -> bool {
        if paths.is_empty() || !self.remotes.iter().any(|name| name == remote) {
            return false;
        }
        let folder = self.folders.get(remote).cloned().unwrap_or_default();
        self.card = Some(Open {
            remote: remote.into(),
            selected_all: !folder.is_empty(),
            folder,
            paths,
            asked: None,
            sent: None,
        });
        self.flyout_open = false;
        self.settle();
        true
    }

    /// Typing replaces the field's text and its selection.
    pub fn edit(&mut self, text: &str) {
        if let Some(open) = self.card.as_mut().filter(|open| open.asked.is_none()) {
            open.folder = text.into();
            open.selected_all = false;
            open.sent = None;
            self.settle();
        }
    }

    pub fn card(&self, i18n: &I18n) -> Option<Card> {
        let open = self.card.as_ref()?;
        let refusal = folder_check(&open.folder).err();
        let refused = self.sends.iter().find(|send| {
            matches!(send.phase, Phase::Refused(_)) && Some(send.request) == open.sent
        });
        Some(Card {
            title: present::title(i18n, open.paths.len(), &open.remote),
            field_label: present::folder_label(i18n),
            value: open.folder.clone(),
            selected_all: open.selected_all,
            error: present::card_error(i18n, refusal, refused),
            cancel: present::cancel_label(i18n),
            primary: present::send_label(i18n),
            can_submit: refusal.is_none() && open.asked.is_none(),
        })
    }

    pub fn close_card(&mut self) {
        if let Some(request) = self.card.take().and_then(|open| open.asked) {
            self.cancel_on_start.insert(request);
        }
        self.settle();
    }

    /// The card leaves without a cancel, for another card taking its place: a send it asked for runs on.
    pub(crate) fn drop_card(&mut self) {
        self.card = None;
        self.settle();
    }

    pub(crate) fn cancel_closed_card(&mut self, request: u64, wire: &dyn Wire) -> bool {
        self.cancel_on_start.remove(&request) && self.cancel(request, wire)
    }

    /// Send: one request; the card closes when the backend takes it and stays with the reason when it refuses.
    pub fn submit(&mut self, wire: &dyn Wire) -> bool {
        let Some(open) = self.card.as_ref().filter(|open| open.asked.is_none()) else {
            return false;
        };
        let Ok(folder) = folder_check(&open.folder).map(str::to_string) else {
            return false;
        };
        let (remote, paths) = (open.remote.clone(), open.paths.clone());
        let id = self.request_id();
        if !wire.send(send_line(id, &remote, &folder, &paths)) {
            return false;
        }
        self.folders.insert(remote.clone(), folder.clone());
        self.sends
            .insert(0, Send::new(id, &remote, &folder, paths.len()));
        if let Some(open) = self.card.as_mut() {
            open.asked = Some(id);
            open.sent = Some(id);
        }
        self.settle();
        true
    }

    /// One backend line; true when anything this unit draws changed.
    pub fn on_line(&mut self, line: &str) -> bool {
        match wire::parse(line) {
            Ok(Some(event)) => self.on_event(&event),
            // A malformed line is the backend's fault and changes nothing on screen.
            Ok(None) | Err(_) => false,
        }
    }

    pub fn on_event(&mut self, event: &Event) -> bool {
        if let Event::Remotes {
            id,
            visible,
            remotes,
        } = event
        {
            if self.asking != Some(*id) {
                return false;
            }
            self.asking = None;
            self.visible = *visible;
            self.remotes = if *visible {
                remotes.clone()
            } else {
                Vec::new()
            };
            return true;
        }
        let mut changed = false;
        for send in &mut self.sends {
            changed |= send.apply(event);
        }
        if let Event::Started { request, job, .. } = event {
            if self
                .card
                .as_ref()
                .is_some_and(|open| open.asked == Some(*request))
            {
                match job {
                    Some(_) => self.card = None,
                    None => self.card.iter_mut().for_each(|open| open.asked = None),
                }
            }
        }
        self.settle();
        changed
    }

    // A send with nothing left to show leaves, as does a refusal no open card asked for; a checking send keeps one sweep.
    fn settle(&mut self) {
        let shown = self.card.as_ref().and_then(|open| open.sent);
        self.sends.retain(|send| {
            send.phase != Phase::Gone
                && (!matches!(send.phase, Phase::Refused(_)) || Some(send.request) == shown)
        });
        let checking: Vec<u64> = self
            .sends
            .iter()
            .filter(|send| matches!(send.phase, Phase::Checking(_)))
            .map(|send| send.request)
            .collect();
        self.sweeps.retain(|key, _| checking.contains(key));
        for key in checking {
            self.sweeps.entry(key).or_default();
        }
        self.generation += 1;
    }

    fn by_key(&mut self, key: u64) -> Option<&mut Send> {
        self.sends.iter_mut().find(|send| send.request == key)
    }

    /// Each press takes the block's key from `blocks`, the send's own request id, which holds from Send to the end.
    pub fn cancel(&mut self, key: u64, wire: &dyn Wire) -> bool {
        let sent = self
            .by_key(key)
            .and_then(Send::cancel)
            .is_some_and(|line| wire.send(line));
        self.settle();
        sent
    }

    pub fn retry(&mut self, key: u64, wire: &dyn Wire) -> bool {
        let sent = self
            .by_key(key)
            .and_then(Send::retry)
            .is_some_and(|line| wire.send(line));
        self.settle();
        sent
    }

    /// Dismiss a failed send; a lost one leaves without asking a backend that never knew it.
    pub fn dismiss(&mut self, key: u64, wire: &dyn Wire) -> bool {
        let Some(send) = self.by_key(key) else {
            return false;
        };
        let line = send.dismiss();
        let left = send.phase == Phase::Gone;
        self.settle();
        left && line.is_none_or(|line| wire.send(line))
    }

    /// The backend ended: every send leaves its job id, unfinished ones become failed blocks with Dismiss only, and the menu forgets its remotes.
    pub fn backend_ended(&mut self) -> bool {
        let mut changed = false;
        for send in &mut self.sends {
            changed |= send.backend_ended();
        }
        self.asking = None;
        self.visible = false;
        self.cancel_on_start.clear();
        self.remotes.clear();
        if let Some(open) = self.card.as_mut() {
            open.asked = None;
        }
        self.settle();
        changed
    }

    /// Each send's block for the transfer card, newest first, keyed by its request id; rebuilt only after a change or a language switch.
    pub fn blocks(&self, i18n: &I18n) -> Blocks {
        let mut cache = self.cache.borrow_mut();
        if let Some((generation, language, blocks)) = cache.as_ref() {
            if *generation == self.generation && language == i18n.language() {
                return Rc::clone(blocks);
            }
        }
        self.builds.set(self.builds.get() + 1);
        let blocks: Blocks = Rc::new(
            self.sends
                .iter()
                .filter_map(|send| Some((send.request, present::block(i18n, send)?)))
                .collect(),
        );
        *cache = Some((self.generation, i18n.language().into(), Rc::clone(&blocks)));
        blocks
    }

    /// Drops the cached words after the catalog reloads in the same language.
    pub fn catalog_changed(&mut self) {
        self.generation += 1;
    }

    /// The check bar's sweep for a checking block, which the block draws with `sweep::SweepBar`.
    pub fn sweep(&self, key: u64) -> Option<Rc<Sweep>> {
        self.sweeps.get(&key).cloned()
    }

    /// The newest finished send's receipt for the status bar.
    pub fn receipt(&self, i18n: &I18n) -> Option<present::Receipt> {
        self.sends
            .iter()
            .find_map(|send| present::receipt(i18n, send))
    }

    /// Clears receipts once the status bar shows the next message.
    pub fn receipt_shown(&mut self) {
        self.sends
            .retain(|send| !matches!(send.phase, Phase::Receipt { .. }));
        self.settle();
    }

    /// Presets a remote's remembered folder, as a send earlier in the session would; the board scene starts from it.
    pub fn remember(&mut self, remote: &str, folder: &str) {
        self.folders.insert(remote.into(), folder.into());
    }
}
