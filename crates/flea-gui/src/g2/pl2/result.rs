//! The last connection result persists after its single-flight request has finished.
use super::{
    mount::{Effect, Phase},
    service::Service,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultState {
    #[default]
    Idle,
    Resolving,
    Mounting,
    Mounted,
    MissingCredential,
    Failed,
    Cancelled,
    Unmounted,
}

impl ResultState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Resolving => "resolving",
            Self::Mounting => "mounting",
            Self::Mounted => "mounted",
            Self::MissingCredential => "missing-credential",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unmounted => "unmounted",
        }
    }

    pub(super) fn effect(effect: &Effect) -> Option<Self> {
        match effect {
            Effect::Run {
                phase: Phase::Mount | Phase::Auth,
                ..
            } => Some(Self::Mounting),
            Effect::Run { .. } | Effect::Peek { .. } | Effect::EnsureBridge { .. } => {
                Some(Self::Resolving)
            }
            Effect::Open { .. } | Effect::OpenFile { .. } => Some(Self::Mounted),
            Effect::Completed {
                error: Some(key), ..
            }
            | Effect::Retry { error: key, .. } => Self::error(key),
            Effect::Say { note, .. } => Self::error(note.key),
            _ => None,
        }
    }

    fn error(key: &str) -> Option<Self> {
        match key {
            "network.another_opening" => None,
            "network.password_needed" => Some(Self::MissingCredential),
            "network.unmounted" => Some(Self::Unmounted),
            _ => Some(Self::Failed),
        }
    }
}
