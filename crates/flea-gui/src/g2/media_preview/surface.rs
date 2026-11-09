//! What a host builds from a player: the Quick Look surface, or the column's picture and framed strip.
use super::ink::{size, MediaInk};
use super::player::MediaPlayer;
use super::sound::MediaSound;
use super::strip::{StripCache, StripElement, StripInput, StripView};
use super::view::{MediaElement, Picture, ViewCache};
use crate::list::Typeface;
use flea::i18n::I18n;
use flea::ui_logic_media_preview::{
    facts, file_name,
    player::{column_duration, Media, Status},
    sample_rate_khz,
};
use gpui::{div, prelude::*, px, App, ClickEvent, Entity, Window};
use std::{cell::RefCell, rc::Rc};

/// The facts a host knows about an audio subject before the player does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The backend's kind name, "Ogg audio"; empty falls back to the suffix.
    pub kind_name: String,
    pub rate_hz: i64,
    pub size_text: String,
    pub probe_duration_ms: i64,
}

impl Facts {
    /// What the row already says before the probe answers: the backend's kind name and the size, as PreviewMedia.qml reads them.
    pub fn of_row(row: &flea::protocol::RowFacts, catalog: &I18n) -> Self {
        Self {
            kind_name: row.kind.clone(),
            size_text: if row.size > 0 {
                catalog.locale().format_size(row.size)
            } else {
                String::new()
            },
            ..Self::default()
        }
    }
}

/// Paint state a host keeps per surface.
#[derive(Clone, Default)]
pub struct Caches {
    pub view: Rc<RefCell<ViewCache>>,
    pub strip: Rc<RefCell<StripCache>>,
}

/// The facts line under an audio name, MediaPdf rule 6's order, joined by the catalog's separator.
pub fn facts_line(path: &str, known: &Facts, duration_ms: i64, catalog: &I18n) -> String {
    let rate = sample_rate_khz(known.rate_hz)
        .map_or_else(String::new, |khz| catalog.tr_args("media.rate", &[&khz]));
    let clock = if duration_ms > 0 {
        duration_ms
    } else {
        known.probe_duration_ms
    };
    facts(path, &known.kind_name, clock, &rate, &known.size_text)
        .join(&catalog.tr("media.separator"))
}

/// What the picture area shows for this player now.
pub fn picture(player: &MediaPlayer, known: &Facts, catalog: &I18n) -> Picture {
    let path = player
        .subject
        .as_ref()
        .map(|subject| subject.path.to_string_lossy().into_owned())
        .unwrap_or_default();
    if player.status() == Status::Failed {
        return Picture::Failed {
            sentence: catalog.tr("media.unplayable"),
        };
    }
    if player.state.has_video {
        return Picture::Video {
            frame: player.frame.clone(),
            poster: player.state.poster(true),
        };
    }
    Picture::Audio {
        name: file_name(&path).to_string(),
        facts: facts_line(&path, known, player.duration, catalog),
    }
}

/// The travel past which gpui starts a drag; a press that moved further was a drag, never a click.
const CLICK_SLOP: f64 = 2.;

/// Whether a click on a video's picture toggles play: one primary press released in place, never a double-click's second.
pub fn frame_toggles(event: &ClickEvent) -> bool {
    let ClickEvent::Mouse(click) = event else {
        return false;
    };
    event.standard_click()
        && event.click_count() == 1
        && (click.up.position - click.down.position).magnitude() <= CLICK_SLOP
}

/// A host's hook for pointer contact with the strip, which re-arms its hide timer.
pub type Touched = Rc<dyn Fn(&mut Window, &mut App)>;

/// Routes strip input to the player; `touched` re-arms the host's hide timer.
pub fn strip_input(
    player: &Entity<MediaPlayer>,
    touched: Option<Touched>,
) -> super::strip::OnInput {
    let player = player.downgrade();
    Rc::new(move |input, window, cx| {
        if input == StripInput::Touched {
            if let Some(touched) = touched.as_ref() {
                touched(window, cx);
            }
            return;
        }
        let _ = player.update(cx, |player, cx| player.strip_input(input, window, cx));
    })
}

/// The Quick Look card's content: the picture over the whole card and the unframed strip along its foot.
pub struct QuickLook<'a> {
    pub player: &'a Entity<MediaPlayer>,
    pub known: &'a Facts,
    pub catalog: &'a I18n,
    pub ink: MediaInk,
    pub typeface: Typeface,
    pub caches: &'a Caches,
    /// Preview.qml: a video's strip hides after its delay; audio's never does.
    pub strip_shown: bool,
    pub touched: Option<Touched>,
    /// A click on the picture toggles play; a card still revealing takes none.
    pub frame_click: bool,
}

/// Quick Look alone leaves ended clips empty at full duration, as ql-video.png and v0.3.12 draw them (EY-52).
pub fn look_view(
    player: &MediaPlayer,
    known: &Facts,
    catalog: &I18n,
    cx: &App,
) -> (Picture, StripView) {
    let mut shown = picture(player, known, catalog);
    let mut view = player.strip_view(false, cx);
    if player.state.has_video && player.state.media == Media::Ended {
        if let Picture::Video { frame, .. } = &mut shown {
            *frame = None;
        }
        if player.duration > 0 {
            view.position = player.duration;
        }
    }
    (shown, view)
}

impl QuickLook<'_> {
    pub fn build(self, cx: &mut App) -> impl IntoElement {
        let state = self.player.read(cx);
        let (picture, view) = look_view(state, self.known, self.catalog, cx);
        let audio = matches!(picture, Picture::Audio { .. });
        let shown = state.subject.is_some() && (audio || self.strip_shown);
        if !shown {
            let mut strip = self.caches.strip.borrow_mut();
            strip.play = None;
            strip.seek = None;
            strip.mute = None;
        }
        let video = matches!(picture, Picture::Video { .. });
        let on_input = strip_input(self.player, self.touched);
        let on_frame = on_input.clone();
        div()
            .relative()
            .size_full()
            .child(MediaElement {
                picture,
                reserve: size::STRIP,
                ink: self.ink,
                typeface: self.typeface.clone(),
                cache: self.caches.view.clone(),
                painted: state.painted.clone(),
            })
            // The picture area alone, so the strip's band keeps its own controls whether or not it shows.
            .when(video && self.frame_click, |card| {
                card.child(
                    div()
                        .id("media-frame")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom(px(size::STRIP))
                        .on_click(move |event, window, cx| {
                            if frame_toggles(event) {
                                on_frame(StripInput::Toggle, window, cx);
                            }
                        }),
                )
            })
            .when(shown, |card| {
                card.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(size::STRIP))
                        .child(StripElement {
                            view,
                            ink: self.ink,
                            typeface: self.typeface,
                            cache: self.caches.strip.clone(),
                            on_input,
                        }),
                )
            })
    }
}

/// The column's framed strip: before Play there is no player and the probe's duration scales it.
pub fn column_view(player: Option<&MediaPlayer>, probe_duration_ms: i64, cx: &App) -> StripView {
    let sound = MediaSound::get(cx);
    match player {
        Some(player) => StripView {
            duration: if player.state.media == Media::Ended && player.duration > 0 {
                player.duration
            } else {
                column_duration(player.position, player.duration, probe_duration_ms)
            },
            ..player.strip_view(true, cx)
        },
        None => StripView {
            playing: false,
            muted: sound.muted,
            framed: true,
            position: 0,
            duration: probe_duration_ms.max(0),
            scrub: None,
            level: sound.volume(),
        },
    }
}
