use super::*;
use gpui::{AvailableSpace, TestAppContext};

#[gpui::test]
fn list_element_lays_out_n_fixed_rows_for_n_names(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    for count in [0, 1, 7, 20] {
        let snapshot = Rc::new(Snapshot {
            title: "files".into(),
            total: count,
            start: 0,
            listing: 1,
            names: (0..count)
                .map(|index| format!("file-{index}").into())
                .collect(),
        });
        let cache = Rc::new(RefCell::new(NameCache::default()));
        let height = px((count + 1) as f32 * ROW_HEIGHT);
        cx.draw(
            point(px(0.), px(0.)),
            size(
                AvailableSpace::Definite(px(640.)),
                AvailableSpace::Definite(height),
            ),
            |_, _| ListElement::new(Rc::clone(&snapshot), Rc::clone(&cache)),
        );
        let bounds = cache.borrow().bounds;
        assert_eq!(bounds.size.height, height);
        assert_eq!(cache.borrow().prepared.len(), count + 1);
        for index in 0..count {
            let row = row_bounds(bounds, index + 1);
            assert_eq!(row.size.height, px(31.));
            assert_eq!(row.origin.y, px((index + 1) as f32 * 31.));
        }
        let first_line = Rc::clone(&cache.borrow().prepared[0]);
        cx.draw(
            point(px(0.), px(0.)),
            size(
                AvailableSpace::Definite(px(640.)),
                AvailableSpace::Definite(height),
            ),
            |_, _| ListElement::new(snapshot, Rc::clone(&cache)),
        );
        assert_eq!(cache.borrow().entries.len(), count + 1);
        assert!(Rc::ptr_eq(&first_line, &cache.borrow().prepared[0]));
    }
}

struct RootProbe {
    snapshot: Rc<Snapshot>,
    cache: Rc<RefCell<NameCache>>,
}

impl gpui::Render for RootProbe {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        ListElement::new(Rc::clone(&self.snapshot), Rc::clone(&self.cache))
    }
}

#[gpui::test]
fn default_root_paints_a_contrasting_canvas(cx: &mut TestAppContext) {
    use gpui::AppContext;
    let cache = Rc::new(RefCell::new(NameCache::default()));
    let window = cx.open_window(size(px(320.), px(120.)), |_, _| RootProbe {
        snapshot: Rc::new(Snapshot {
            title: "files".into(),
            total: 1,
            start: 0,
            listing: 1,
            names: vec!["visible-name".into()],
        }),
        cache: Rc::clone(&cache),
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        let quads = window.painted_quads();
        println!("REVIEW root painted_quads={}", quads.len());
        assert_eq!(quads.len(), 1, "root list lost its only background quad");
        let cache = cache.borrow();
        let theme = crate::palette::Theme::default();
        assert_eq!(quads[0].background.as_solid(), Some(theme.background));
        assert_eq!(cache.background, theme.background);
        assert_eq!(cache.key.as_ref().unwrap().color, theme.foreground);
        assert_ne!(theme.background, theme.foreground);
    })
    .unwrap();
}

#[gpui::test]
fn root_uses_the_installed_theme(cx: &mut TestAppContext) {
    use gpui::AppContext;
    let original = crate::palette::Theme::default();
    let theme = crate::palette::Theme {
        background: original.foreground,
        foreground: original.background,
    };
    cx.update(|cx| cx.set_global(theme));
    let cache = Rc::new(RefCell::new(NameCache::default()));
    let window = cx.open_window(size(px(320.), px(120.)), |_, _| RootProbe {
        snapshot: Rc::new(Snapshot {
            title: "files".into(),
            total: 0,
            start: 0,
            listing: 1,
            names: vec![],
        }),
        cache: Rc::clone(&cache),
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        let quads = window.painted_quads();
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].background.as_solid(), Some(theme.background));
        let cache = cache.borrow();
        assert_eq!(cache.background, theme.background);
        assert_eq!(cache.key.as_ref().unwrap().color, theme.foreground);
    })
    .unwrap();
}

struct InheritedProbe {
    cache: Rc<RefCell<NameCache>>,
}

impl gpui::Render for InheritedProbe {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::{ParentElement, Styled};
        let theme = crate::palette::Theme::default();
        gpui::div()
            .size_full()
            .text_color(theme.background)
            .text_bg(theme.foreground)
            .child(ListElement::new(
                Rc::new(Snapshot {
                    title: "inherited".into(),
                    total: 0,
                    start: 0,
                    listing: 1,
                    names: vec![],
                }),
                Rc::clone(&self.cache),
            ))
    }
}

#[gpui::test]
fn inherited_background_keeps_its_text_colour(cx: &mut TestAppContext) {
    use gpui::AppContext;
    let theme = crate::palette::Theme::default();
    let cache = Rc::new(RefCell::new(NameCache::default()));
    let window = cx.open_window(size(px(320.), px(120.)), |_, _| InheritedProbe {
        cache: Rc::clone(&cache),
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        let cache = cache.borrow();
        assert_eq!(cache.background, theme.foreground);
        assert_eq!(cache.key.as_ref().unwrap().color, theme.background);
        let quads = window.painted_quads();
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].background.as_solid(), Some(theme.foreground));
    })
    .unwrap();
}
