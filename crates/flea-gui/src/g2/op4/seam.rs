//! The joins the window supplies: transport, status, output checks, document facts and held thumbnails.
use gpui::RenderImage;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub generation: u64,
    pub request: u64,
}

#[derive(Clone)]
pub struct Thumbnail {
    pub path: String,
    pub size: (u32, u32),
    pub image: Arc<RenderImage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputError {
    Exists(String),
    Access {
        key: Option<&'static str>,
        diagnostic: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Send(String),
    CheckOutputs {
        ticket: Ticket,
        paths: Vec<String>,
    },
    DocumentPages {
        ticket: Ticket,
        path: String,
    },
    Refresh {
        paths: Vec<String>,
        select: bool,
    },
    Status {
        error: bool,
        text: String,
    },
    Progress {
        done: usize,
        total: usize,
        name: String,
    },
    ClearProgress,
    Launch(Vec<String>),
    RestoreFocus,
}

/// The pv1 service returns already-held images, in the supplied order, without codec work in the card.
pub trait Thumbnails {
    fn held(&self, generation: u64, paths: &[String]) -> Vec<Thumbnail>;
}

/// The pv2 service reads the cached document's page count under its source identity.
pub trait Documents {
    fn pages(&self, path: &str, ticket: Ticket);
}
