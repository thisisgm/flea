use crate::g2::media_preview::ink::{size as board, MediaInk};
use crate::g2::media_preview::strip::{
    metrics, part_at, Part, StripCache, StripElement, StripView,
};
use crate::g2::media_preview::view::{audio_column, elide_right, MediaElement, Picture, ViewCache};
use crate::list::bench::{allocations, bench_typeface, real_cx};
use flea::ui_logic_media_preview::strip::layout;
use gpui::{
    div, prelude::*, px, App, Bounds, Context, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Render, TestAppContext, Window,
};
use std::{cell::Cell, cell::RefCell, rc::Rc};

// Board numbers: the strip's slots, gaps and volume track on the 902 card, and the column's framed strip at 468.
#[test]
fn pv3_the_strip_lays_out_as_the_boards_draw_it() {
    let metrics = metrics(&bench_typeface());
    let card = layout(902., false, 77., metrics);
    assert_eq!(
        (card.play.x, card.play.w),
        (9., 24.),
        "ql strip padding 9, slot 24"
    );
    assert_eq!(
        card.volume.map(|track| (track.x, track.w)),
        Some((833., 60.)),
        "a 60 px track one gap from the edge"
    );
    assert_eq!(
        (card.mute.x, card.mute.right()),
        (800., 824.),
        "mute one gap left of the volume"
    );
    assert_eq!(card.clock.right(), 791.);
    assert_eq!(
        (card.seek.x, card.seek.right()),
        (42., 705.),
        "the track takes the rest, a gap each side"
    );
    let column = layout(468., true, 77., metrics);
    assert_eq!(
        column.volume, None,
        "the column strip draws no volume track"
    );
    assert_eq!(
        (column.play.x, column.mute.right()),
        (9., 459.),
        "frame 1 plus padding 8 each side"
    );
    assert_eq!(part_at(&card, 10.), Some(Part::Play));
    assert_eq!(part_at(&card, 400.), Some(Part::Seek));
    assert_eq!(part_at(&card, 810.), Some(Part::Mute));
    assert_eq!(part_at(&card, 860.), Some(Part::Volume));
    assert_eq!(part_at(&card, 795.), None, "the clock takes no press");
    assert_eq!(
        audio_column(902.),
        board::AUDIO_TEXT,
        "ql-audio's 820 px text column"
    );
    let small = crate::list::Typeface {
        body: px(12.),
        caption: px(10.),
        ..bench_typeface()
    };
    let small = layout(902., false, 77., self::metrics(&small));
    assert_eq!(
        small.volume.map(|track| track.w),
        Some(51.),
        "stepSize12: body small 11 draws a 51 px track"
    );
}

#[test]
fn pv3_a_long_audio_name_elides_at_its_column() {
    assert_eq!(elide_right("tone.ogg", 8), "tone.ogg");
    assert_eq!(elide_right("a-very-long-name.ogg", 8), "a-very-\u{2026}");
    assert_eq!(elide_right("x", 0), "\u{2026}");
}

/// Wraps an Element and counts the heap allocations its paint makes.
struct Measured<E>(E, Rc<Cell<u64>>);

impl<E: Element> IntoElement for Measured<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element> Element for Measured<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.0.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.0.prepaint(id, inspector, bounds, state, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let before = allocations();
        self.0
            .paint(id, inspector, bounds, state, prepaint, window, cx);
        self.1.set(allocations() - before);
    }
}

struct Card {
    strip: Rc<RefCell<StripCache>>,
    view: Rc<RefCell<ViewCache>>,
    spent: (Rc<Cell<u64>>, Rc<Cell<u64>>),
    position: Rc<Cell<i64>>,
}

impl Render for Card {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = MediaInk::inherited(&window.text_style(), theme);
        let typeface = bench_typeface();
        let view = StripView {
            playing: true,
            muted: false,
            framed: false,
            position: self.position.get(),
            duration: 2_000,
            scrub: None,
            level: 100,
        };
        let picture = Picture::Audio {
            name: "tone.ogg".into(),
            facts: "Ogg audio \u{b7} 0:02 \u{b7} 44.1 kHz \u{b7} 6.9 kB".into(),
        };
        div()
            .w(px(902.))
            .h(px(574.))
            .flex()
            .flex_col()
            .child(Measured(
                MediaElement {
                    picture,
                    reserve: 0.,
                    ink,
                    typeface: typeface.clone(),
                    cache: self.view.clone(),
                    painted: Rc::default(),
                },
                self.spent.0.clone(),
            ))
            .child(Measured(
                StripElement {
                    view,
                    ink,
                    typeface,
                    cache: self.strip.clone(),
                    on_input: Rc::new(|_, _, _| {}),
                },
                self.spent.1.clone(),
            ))
    }
}

// Pointer listeners are GPUI's boxed closures, four a frame; past them the strip and the pane draw from caches alone.
const LISTENERS: u64 = 4;

#[test]
fn pv3_a_playing_strip_and_its_pane_allocate_nothing_of_their_own_per_frame() {
    let mut cx: TestAppContext = real_cx();
    let card = Card {
        strip: Rc::default(),
        view: Rc::default(),
        spent: (Rc::default(), Rc::default()),
        position: Rc::new(Cell::new(0)),
    };
    let (pane, strip, position) = (
        card.spent.0.clone(),
        card.spent.1.clone(),
        card.position.clone(),
    );
    let handle = cx.add_window(|_, _| card);
    let mut worst = (0, 0);
    for frame in 0..24 {
        // Inside one second the clock text stays, so nothing is shaped again.
        position.set(frame * 20);
        cx.update_window(handle.into(), |_, window, _| window.refresh())
            .unwrap();
        cx.run_until_parked();
        if frame >= 4 {
            worst = (worst.0.max(pane.get()), worst.1.max(strip.get()));
        }
    }
    eprintln!("PV3_PAINT_ALLOCS pane={} strip={}", worst.0, worst.1);
    assert_eq!(worst.0, 0, "the audio pane");
    assert!(worst.1 <= LISTENERS, "the strip spent {}", worst.1);
}

struct Bare {
    view: StripView,
    cache: Rc<RefCell<StripCache>>,
    heard: Rc<RefCell<Vec<crate::g2::media_preview::strip::StripInput>>>,
}

impl Render for Bare {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let heard = self.heard.clone();
        let strip = StripElement {
            view: self.view,
            ink: MediaInk::inherited(
                &window.text_style(),
                cx.try_global::<crate::palette::Theme>()
                    .copied()
                    .unwrap_or_default(),
            ),
            typeface: bench_typeface(),
            cache: self.cache.clone(),
            on_input: Rc::new(move |input, _, _| heard.borrow_mut().push(input)),
        };
        div().w(px(902.)).h(px(board::STRIP)).child(strip)
    }
}

// The pointer reaches each part by the board's own layout: a tap counts where it lands, a drag seeks once.
#[test]
fn pv3_the_pointer_reaches_each_strip_part_where_the_board_draws_it() {
    use crate::g2::media_preview::strip::StripInput::{self, *};
    use gpui::{point, Modifiers, MouseButton};
    let mut cx: TestAppContext = real_cx();
    let heard: Rc<RefCell<Vec<StripInput>>> = Rc::default();
    let view = StripView {
        playing: false,
        muted: false,
        framed: false,
        position: 0,
        duration: 2_000,
        scrub: None,
        level: 100,
    };
    let bare = Bare {
        view,
        cache: Rc::default(),
        heard: heard.clone(),
    };
    let (_, cx) = cx.add_window_view(|_, _| bare);
    cx.run_until_parked();
    let at = |x: f32| point(px(x), px(13.));
    let taken = || {
        std::mem::take(&mut *heard.borrow_mut())
            .into_iter()
            .filter(|input| *input != Touched)
            .collect::<Vec<_>>()
    };
    cx.simulate_click(at(20.), Modifiers::none());
    assert_eq!(taken(), [Toggle], "play slot 9 to 33");
    cx.simulate_click(at(812.), Modifiers::none());
    assert_eq!(taken(), [Mute], "mute slot 800 to 824");
    cx.simulate_mouse_down(at(20.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(400.), MouseButton::Left, Modifiers::none());
    assert_eq!(taken(), [], "a press that leaves its slot is no tap");
    cx.simulate_mouse_down(at(42.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(705.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(705.), MouseButton::Left, Modifiers::none());
    assert_eq!(
        taken(),
        [ScrubPress(0.), ScrubMove(1.), ScrubRelease],
        "the seek track spans 42 to 705"
    );
    cx.simulate_mouse_down(at(863.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(863.), MouseButton::Left, Modifiers::none());
    assert_eq!(
        taken(),
        [LevelPress(50), LevelRelease],
        "the 60 px volume track from 833"
    );
    cx.simulate_click(at(795.), Modifiers::none());
    assert_eq!(taken(), [], "the clock takes no press");
    cx.simulate_mouse_move(at(400.), None, Modifiers::none());
    assert!(
        heard.borrow().contains(&Touched),
        "hover over the strip re-arms the host's hide timer"
    );
}

// EY-42: Quick Look's audio facts line names the kind and ends with the size, as ql-audio.png and v0.3.12 read.
#[test]
fn pv3_ey42_audio_facts_name_the_kind_and_the_size() {
    use crate::g2::media_preview::surface::{facts_line, Facts};
    let catalog = crate::test_catalogs::english();
    let row = flea::protocol::RowFacts {
        name: "tone.ogg".into(),
        size: 6_900,
        icon: "audio-x-generic".into(),
        kind: "Ogg audio".into(),
        ..Default::default()
    };
    let mut known = Facts::of_row(&row, &catalog);
    known.rate_hz = 44_100;
    assert_eq!(
        facts_line("/m/tone.ogg", &known, 2_000, &catalog),
        "Ogg audio \u{b7} 0:02 \u{b7} 44.1 kHz \u{b7} 6.9 kB"
    );
    let unnamed = Facts::of_row(
        &flea::protocol::RowFacts {
            kind: String::new(),
            size: 0,
            ..row
        },
        &catalog,
    );
    assert_eq!(
        facts_line("/m/tone.ogg", &unnamed, 2_000, &catalog),
        "ogg \u{b7} 0:02",
        "an empty kind falls back to the suffix and no size is left out"
    );
}

// EY-429, EY-436: MediaStrip fills with Theme.color.surface, the darker well, never the pane's own ground.
#[test]
fn pv3_the_strip_well_is_the_theme_surface() {
    let theme = crate::palette::Theme::from_text("background='#1c120d'\ndark_background='#150d0a'");
    let ink = MediaInk::inherited(&gpui::TextStyle::default(), theme);
    assert_eq!(ink.surface, theme.surface, "the strip well");
    assert_ne!(
        ink.surface, ink.background,
        "the well differs from the pane"
    );
}
