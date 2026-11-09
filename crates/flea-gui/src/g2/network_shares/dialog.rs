//! A draft is saved only after its own endpoint mounted; the writer acknowledges the commit once.
use crate::g2::device_rail::secret::Secret;
use flea::ui_logic_network_shares::form::{Form, Protocol, PROTOCOLS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    Protocol(usize),
    Label,
    #[default]
    Host,
    Port,
    Path,
    Domain,
    User,
    Password,
    Eye,
    Tls,
    Cancel,
    Connect,
    Dropbox,
}

#[derive(Debug)]
pub enum Effect {
    Mount {
        id: u64,
        uri: String,
        label: String,
        origin: u64,
        password: Secret,
    },
    Cancel(u64),
    Save {
        id: u64,
        uri: String,
        label: String,
        replace: Option<String>,
    },
    Closed,
    CheckDropbox(u64),
    CancelDropbox(u64),
    InstallDropbox,
}

#[derive(Default)]
pub struct Dialog {
    pub origin: u64,
    pub opened: bool,
    pub form: Form,
    pub password: Secret,
    pub reveal: bool,
    pub focus: Focus,
    pub connecting: bool,
    pub saving: bool,
    pub committed: bool,
    pub retry: bool,
    pub failed_connect: bool,
    pub status: Option<&'static str>,
    pub detail: String,
    pub request: u64,
    serial: u64,
    pub pending_uri: String,
    pub mounted_uri: String,
    pub pending_label: String,
    pub replace: Option<String>,
    pub save_new: bool,
    pub dropbox_installed: bool,
    pub provider_check: Option<u64>,
    unacknowledged: Option<(String, &'static str, String)>,
}

impl Dialog {
    pub fn open(&mut self) -> Vec<Effect> {
        if self.opened || self.busy() {
            return Vec::new();
        }
        self.form = Form::default();
        self.password.clear();
        self.reveal = false;
        self.focus = Focus::Host;
        self.status = None;
        self.detail.clear();
        self.retry = false;
        self.committed = false;
        self.failed_connect = false;
        self.unacknowledged = None;
        self.save_new = true;
        self.replace = None;
        self.mounted_uri.clear();
        self.opened = true;
        self.serial += 1;
        let id = self.serial;
        self.provider_check = Some(id);
        vec![Effect::CheckDropbox(id)]
    }
    pub fn open_location(&mut self, uri: &str, label: &str, replace: bool) -> Vec<Effect> {
        if self.opened || self.busy() {
            return Vec::new();
        }
        let Some(address) = Form::saved_address(uri).ok().or_else(|| {
            (!replace)
                .then(|| super::routing::incomplete(uri))
                .flatten()
        }) else {
            return Vec::new();
        };
        let unacknowledged = self.unacknowledged.take();
        let effects = self.open();
        self.unacknowledged = unacknowledged;
        self.form = Form::load(&address, label);
        if address.host.is_empty() {
            self.form.original = None;
        }
        self.save_new = false;
        self.retry = true;
        self.replace = replace.then_some(address.uri);
        if self.unacknowledged.as_ref().is_some_and(|(old, _, _)| {
            flea::ui_logic_network_shares::mounts::normalize(old)
                == flea::ui_logic_network_shares::mounts::normalize(uri)
        }) {
            let (_, key, detail) = self.unacknowledged.take().unwrap();
            self.status = Some(key);
            self.detail = detail;
            self.failed_connect = true;
        }
        effects
    }
    /// A nearby host fills the protocol, Host, Port and any path it names, and the caret waits on Connect; nothing connects until then.
    pub fn suggest(&mut self, address: &flea::ui_logic_network_shares::networkaddress::Address) {
        if !self.opened || self.busy() {
            return;
        }
        self.form.suggest(address);
        self.focus = Focus::Connect;
    }
    pub fn busy(&self) -> bool {
        self.connecting || self.saving
    }
    pub fn close(&mut self) -> Vec<Effect> {
        if !self.opened || self.saving {
            return Vec::new();
        }
        let mut effects = Vec::new();
        if let Some(id) = self.provider_check.take() {
            effects.push(Effect::CancelDropbox(id));
        }
        if self.connecting {
            effects.push(Effect::Cancel(self.request));
        }
        self.connecting = false;
        self.opened = false;
        self.request = 0;
        self.password.clear();
        self.reveal = false;
        self.mounted_uri.clear();
        effects.push(Effect::Closed);
        effects
    }
    pub fn submit(&mut self) -> Vec<Effect> {
        if !self.opened || self.busy() {
            return Vec::new();
        }
        if self.committed {
            return self.close();
        }
        let Some(uri) = self.form.uri() else {
            self.password.clear();
            self.reveal = false;
            self.status = Some("network.invalid_form");
            return Vec::new();
        };
        if self.form.protocol.credentials()
            && self.mounted_uri != uri
            && !self.form.user.trim().is_empty()
            && self.password.is_empty()
        {
            self.status = Some("network.password_needed");
            self.retry = true;
            self.failed_connect = false;
            return Vec::new();
        }
        self.unacknowledged = None;
        self.serial += 1;
        self.request = self.serial;
        self.pending_uri = uri.clone();
        self.pending_label = self.form.name();
        self.status = None;
        self.detail.clear();
        self.reveal = false;
        self.focus = Focus::Connect;
        if self.mounted_uri == uri {
            self.password.clear();
            return self.save();
        }
        self.connecting = true;
        vec![Effect::Mount {
            id: self.request,
            uri,
            label: self.pending_label.clone(),
            origin: self.origin,
            password: if self.form.protocol.credentials() {
                Secret::from_bytes(self.password.expose())
            } else {
                self.password.clear();
                Secret::new()
            },
        }]
    }
    pub fn mount_finished(
        &mut self,
        id: u64,
        uri: &str,
        result: Result<(), &'static str>,
        connect_failed: bool,
    ) -> Vec<Effect> {
        if !self.opened || !self.connecting || self.request != id || self.pending_uri != uri {
            return Vec::new();
        }
        self.connecting = false;
        if let Err(key) = result {
            self.failed(key, connect_failed);
            return Vec::new();
        }
        self.unacknowledged = None;
        self.password.clear();
        self.mounted_uri = uri.into();
        self.save()
    }
    fn save(&mut self) -> Vec<Effect> {
        if !self.save_new
            && self.replace.as_ref().is_none_or(|old| {
                flea::ui_logic_network_shares::mounts::normalize(old)
                    == flea::ui_logic_network_shares::mounts::normalize(&self.pending_uri)
            })
        {
            return self.close();
        }
        self.saving = true;
        vec![Effect::Save {
            id: self.request,
            uri: self.pending_uri.clone(),
            label: self.pending_label.clone(),
            replace: self.replace.clone(),
        }]
    }
    pub fn saved(&mut self, id: u64, success: bool, detail: &str) -> Vec<Effect> {
        if !self.opened || !self.saving || id != self.request {
            return Vec::new();
        }
        self.saving = false;
        if !success {
            self.failed("network.save_failed", false);
            self.detail = detail.into();
            return Vec::new();
        }
        if !detail.is_empty() {
            self.committed = true;
            self.status = Some("network.save_committed");
            self.detail = detail.into();
            return Vec::new();
        }
        self.close()
    }
    pub fn failed(&mut self, key: &'static str, connect: bool) {
        self.reveal = false;
        self.status = Some(key);
        self.failed_connect = connect;
        self.retry = true;
        // The sign-in invitation puts the caret where the missing name goes.
        self.focus = if key == super::mount::GUEST_EMPTY && self.form.protocol.credentials() {
            Focus::User
        } else {
            Focus::Host
        };
        if connect {
            self.unacknowledged = Some((self.pending_uri.clone(), key, self.detail.clone()));
        }
    }
    pub fn set_detail(&mut self, detail: String) {
        if let Some((_, key, held)) = &mut self.unacknowledged {
            if self.status == Some(*key) {
                held.clone_from(&detail);
            }
        }
        self.detail = detail;
    }
    pub fn ring(&self) -> Vec<Focus> {
        if self.committed {
            let mut ring = vec![Focus::Cancel, Focus::Connect];
            if !self.dropbox_installed {
                ring.push(Focus::Dropbox);
            }
            return ring;
        }
        let mut ring: Vec<_> = (0..5).map(Focus::Protocol).collect();
        ring.extend([Focus::Label, Focus::Host, Focus::Port, Focus::Path]);
        if self.form.protocol == Protocol::Smb {
            ring.push(Focus::Domain);
        }
        if self.form.protocol.credentials() {
            ring.extend([Focus::User, Focus::Password]);
        }
        if self.form.protocol.tls_control() {
            ring.push(Focus::Tls);
        }
        ring.extend([Focus::Cancel, Focus::Connect]);
        if !self.dropbox_installed {
            ring.push(Focus::Dropbox);
        }
        ring
    }
    pub fn step(&mut self, back: bool) {
        if self.busy() {
            return;
        }
        let ring = self.ring();
        let at = ring.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = ring[(at + if back { ring.len() - 1 } else { 1 }) % ring.len()];
    }
    pub fn pick(&mut self, index: usize) {
        if let Some(protocol) = PROTOCOLS.get(index) {
            let previous = self.ring();
            let at = previous.iter().position(|f| *f == self.focus).unwrap_or(0);
            self.form.pick(*protocol);
            let visible = self.ring();
            if !visible.contains(&self.focus) {
                self.focus = previous
                    .iter()
                    .cycle()
                    .skip(at + 1)
                    .take(previous.len())
                    .find(|f| visible.contains(f))
                    .copied()
                    .unwrap_or(Focus::Host);
            }
        }
    }
    pub fn backend_died(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(id) = self.provider_check.take() {
            effects.push(Effect::CancelDropbox(id));
        }
        if self.connecting {
            effects.push(Effect::Cancel(self.request));
        }
        self.connecting = false;
        self.password.clear();
        self.reveal = false;
        if self.saving {
            return effects;
        }
        self.request = 0;
        self.failed("network.backend_died", true);
        effects
    }
}
