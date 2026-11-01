//! A bare server's share rows carry only names; activating one uses the same share opener.
#[derive(Debug, Default)]
pub struct Browser {
    pub open: bool,
    pub uri: String,
    pub label: String,
    pub names: Vec<String>,
    pub cursor: usize,
    pub start: usize,
    pub origin: u64,
    pub generation: u64,
}
impl Browser {
    pub fn show(&mut self, uri: String, label: String, names: Vec<String>, origin: u64) {
        self.open = true;
        self.uri = uri;
        self.label = label;
        self.names = names;
        self.cursor = 0;
        self.start = 0;
        self.origin = origin;
        self.generation += 1;
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    /// The IPC cursor: -1 until the first show (v0.3.12's unloaded ShareBrowser); a closed browser keeps its row.
    pub fn cursor_ipc(&self) -> i64 {
        if self.generation == 0 {
            -1
        } else {
            self.cursor as i64
        }
    }
    pub fn step(&mut self, delta: isize, visible: usize) {
        self.cursor = self
            .cursor
            .saturating_add_signed(delta)
            .min(self.names.len().saturating_sub(1));
        if self.cursor < self.start {
            self.start = self.cursor;
        }
        if self.cursor >= self.start + visible.max(1) {
            self.start = self.cursor + 1 - visible.max(1);
        }
    }
    pub fn activate(&self) -> Option<(String, String, u64)> {
        if !self.open {
            return None;
        }
        let name = self.names.get(self.cursor)?;
        if !valid_share(name) {
            return None;
        }
        Some((
            format!(
                "{}/{}/",
                self.uri.trim_end_matches('/'),
                super::networkaddress::encode(name, false)
            ),
            name.clone(),
            self.origin,
        ))
    }
}

pub fn valid_share(name: &str) -> bool {
    !name.is_empty()
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

pub fn same_server(left: &str, right: &str) -> bool {
    let (Ok(a), Ok(b)) = (
        super::form::Form::saved_address(left),
        super::form::Form::saved_address(right),
    ) else {
        return false;
    };
    a.scheme == b.scheme
        && a.host == b.host
        && a.port == b.port
        && a.user == b.user
        && a.domain == b.domain
}

pub fn child_name(server: &str, child: &str) -> Option<String> {
    let root = super::form::Form::saved_address(server).ok()?;
    let address = super::form::Form::saved_address(child).ok()?;
    if root.path != "/" || !same_server(server, child) {
        return None;
    }
    let name = address.path.strip_prefix('/')?;
    valid_share(name).then(|| name.to_owned())
}
