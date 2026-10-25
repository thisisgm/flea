//! One LocalSend send's phase, so a late, foreign or backend-death outcome lands exactly once.
use super::wire::Event;

// Where one send stands for its front end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    // Asked for, no terminal verdict yet.
    Asked,
    // The transfer finished; the verdict was said once.
    Done,
    // The send ended without a receipt; the backend's sentence is the diagnostic.
    Failed(String),
    // The backend ended while the send ran; its request id means nothing to the next backend.
    Lost,
}

// One send as a front end holds it: its own request, the peer and paths it named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Send {
    pub request: u64,
    pub peer: String,
    pub paths: Vec<String>,
    pub phase: Phase,
}

impl Send {
    pub fn new(request: u64, peer: &str, paths: &[String]) -> Self {
        Self { request, peer: peer.into(), paths: paths.to_vec(), phase: Phase::Asked }
    }

    pub fn running(&self) -> bool {
        self.phase == Phase::Asked
    }

    // Applies one parsed event; anything for another request or after the end changes nothing.
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::Sent { id, ok, reason } = event else { return false };
        if *id != self.request || self.phase != Phase::Asked {
            return false;
        }
        self.phase = if *ok { Phase::Done } else { Phase::Failed(reason.clone()) };
        true
    }

    pub fn backend_dead(&mut self) -> bool {
        if self.phase == Phase::Asked {
            self.phase = Phase::Lost;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(id: u64, ok: bool, reason: &str) -> Event {
        Event::Sent { id, ok, reason: reason.into() }
    }

    #[test]
    fn a_send_ends_once_on_its_own_verdict_and_ignores_foreign_lines() {
        let mut send = Send::new(3, "Clean Lemon", &["/home/gm/a.txt".to_string()]);
        assert!(send.running());
        assert!(!send.apply(&sent(4, true, "")));
        assert!(send.running());
        assert!(send.apply(&sent(3, true, "")));
        assert_eq!(send.phase, Phase::Done);
        assert!(!send.running());
        assert!(!send.apply(&sent(3, false, "late")));
    }

    #[test]
    fn a_refused_send_keeps_the_backend_s_sentence_and_a_dead_backend_loses_it() {
        let mut send = Send::new(3, "Clean Lemon", &["/home/gm/a.txt".to_string()]);
        assert!(send.apply(&sent(3, false, "Send cancelled.")));
        assert_eq!(send.phase, Phase::Failed("Send cancelled.".into()));
        assert!(!send.backend_dead());
        let mut live = Send::new(5, "Clean Lemon", &["/home/gm/a.txt".to_string()]);
        assert!(live.backend_dead());
        assert_eq!(live.phase, Phase::Lost);
    }
}
