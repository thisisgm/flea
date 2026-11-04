//! Save field: name, stem, review; Enter saves, errors via keys.
use flea::i18n::I18n;
use flea::ui_logic_pk2::{self as core, SaveCheck};

// What the field draws, translated by the caller; this struct only selects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveWords {
    pub error: String,
    pub review: bool,
}

// The probe key: folder plus typed name (PickerWindow.qml saveKey).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbeKey {
    pub folder: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveField {
    // The typed name only; saveName is field text.
    name: String,
    // The request's own name: the drawn placeholder while nothing is typed.
    fallback: String,
    folder: String,
    probed: Option<Probe>,
    location_changed: bool,
    backend_gone: bool,
    save_error: String,
    review_open: bool,
    pub stem_requests: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Probe {
    key: ProbeKey,
    exists: bool,
}

impl SaveField {
    pub fn new(fallback: &str) -> Self {
        let mut field = Self {
            fallback: fallback.to_owned(),
            ..Self::default()
        };
        // The request name seeds the field when valid.
        if core::valid_name(fallback) {
            field.name = fallback.to_owned();
        }
        field
    }

    pub fn edit(&mut self, text: &str) {
        if self.name != text {
            self.name = text.to_owned();
            self.probed = None;
            self.review_open = false;
            self.save_error.clear();
        }
    }

    // The drawn text: typed name, else the request name.
    pub fn asked_name(&self) -> &str {
        if self.name.is_empty() {
            &self.fallback
        } else {
            &self.name
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_folder(&mut self, folder: &str) {
        if self.folder != folder {
            self.folder = folder.to_owned();
            self.probed = None;
        }
    }

    // The key the window probes; the answer returns via set_exists.
    pub fn probe_key(&self) -> ProbeKey {
        ProbeKey {
            folder: self.folder.clone(),
            name: self.name.clone(),
        }
    }

    // The probe answer; true when its key still matches.
    pub fn set_exists(&mut self, key: &ProbeKey, exists: bool) -> bool {
        if *key == self.probe_key() {
            self.probed = Some(Probe {
                key: key.clone(),
                exists,
            });
            return true;
        }
        false
    }

    pub fn set_location_changed(&mut self, changed: bool) {
        self.location_changed = changed;
    }

    pub fn note_backend_gone(&mut self) {
        self.backend_gone = true;
    }

    // The save reply's own refusal, shown verbatim like every backend reason.
    pub fn set_save_error(&mut self, error: &str) {
        self.save_error = error.to_owned();
    }

    pub fn refused(&self) -> bool {
        !self.name.is_empty() && !core::valid_name(&self.name)
    }

    // The check runs on the typed name through the single core check.
    pub fn check(&self) -> SaveCheck {
        let matched = match &self.probed {
            Some(probe) if probe.key.folder == self.folder && probe.key.name == self.name => {
                Some(probe.exists)
            }
            _ => None,
        };
        core::check_save_name(&self.name, matched, self.location_changed)
    }

    // Accept: empty asks for a name, collision opens review.
    pub fn accept(&mut self) -> SaveCheck {
        let check = self.check();
        if check == SaveCheck::Exists {
            self.review_open = true;
            self.stem_requests += 1;
        }
        check
    }

    pub fn stem_end(&self) -> usize {
        core::stem_end(&self.name)
    }

    // The error line under the field.
    pub fn words(&self, i18n: &I18n) -> SaveWords {
        if self.backend_gone {
            return SaveWords {
                error: core::save_stopped_field(i18n),
                review: false,
            };
        }
        if self.refused() {
            return SaveWords {
                error: core::save_refused(i18n, &self.name),
                review: false,
            };
        }
        if !self.save_error.is_empty() {
            return SaveWords {
                error: self.save_error.clone(),
                review: false,
            };
        }
        if self.review_open {
            return SaveWords {
                error: core::collision_short(i18n, &self.jointed()),
                review: true,
            };
        }
        if self.check() == SaveCheck::LocationChanged {
            return SaveWords {
                error: core::save_moved(i18n),
                review: false,
            };
        }
        SaveWords::default()
    }

    // The already-exists line with wrap joints.
    pub fn jointed(&self) -> String {
        core::joint_name(&self.name)
    }
}
