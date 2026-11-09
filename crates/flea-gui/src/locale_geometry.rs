//! Logical-leading geometry for the source contract in i18n/rtl-policy.json.
use crate::leaf::{Leaf, Painted};
use flea::i18n::Locale;
use gpui::{AnyElement, App, Bounds, IntoElement, LayoutId, Pixels, Point, Window};

/// Direction is captured with the locale epoch, never inferred from a file's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Direction(bool);

/// Only navigation glyphs have a reading direction; artwork and media glyphs keep theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconRole {
    Navigation,
    NonDirectional,
    Media,
}

impl Direction {
    pub fn from_locale(locale: &Locale) -> Self {
        Self(locale.right_to_left)
    }

    pub fn is_rtl(self) -> bool {
        self.0
    }

    /// Reflect a logical LTR rectangle in the same coordinate space, preserving size and overflow.
    pub fn rect(self, container: Bounds<Pixels>, mut bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        if self.0 {
            bounds.origin.x = container.origin.x + (container.right() - bounds.right());
        }
        bounds
    }

    /// Reflect a continuous point, including slider endpoints; the same operation is its inverse.
    pub fn point(self, container: Bounds<Pixels>, mut point: Point<Pixels>) -> Point<Pixels> {
        if self.0 {
            point.x = container.origin.x + (container.right() - point.x);
        }
        point
    }

    /// Hit painted bounds with physical left/top inclusive and right/bottom exclusive edges.
    pub fn hit_test(
        self,
        container: Bounds<Pixels>,
        logical: Bounds<Pixels>,
        physical: Point<Pixels>,
    ) -> bool {
        let painted = self.rect(container, logical);
        physical.x >= painted.origin.x
            && physical.x < painted.right()
            && physical.y >= painted.origin.y
            && physical.y < painted.bottom()
    }

    /// Map a column or cell slot without changing its model identity; also the inverse map.
    pub fn visual_index(self, logical: usize, count: usize) -> Option<usize> {
        (logical < count).then(|| if self.0 { count - 1 - logical } else { logical })
    }

    /// Physical Right (true) or Left (false) to a logical step, after resolving h/l bindings.
    pub fn reading_step(self, physical_right: bool) -> i8 {
        if physical_right != self.0 {
            1
        } else {
            -1
        }
    }

    pub fn mirrors_icon(self, role: IconRole) -> bool {
        self.0 && role == IconRole::Navigation
    }
}

/// Back points to logical leading, so a mirrored window draws the toolbar's back arrow pointing right.
pub const ARROW_RIGHT: (&str, &[u8]) = (
    "flea-chrome:arrow-right",
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M5 12h14 M12 5l7 7-7 7"/></svg>"#,
);

/// A disclosure chevron points to logical trailing, so a mirrored window draws it pointing left.
pub fn trailing_chevron(mirrored: bool) -> &'static str {
    if mirrored {
        "chevron-left"
    } else {
        "chevron-right"
    }
}

/// Keeps an editable line, file content or media transport left to right, moved whole to its mirrored place.
pub struct Unmirrored(AnyElement);

pub fn unmirrored(child: impl IntoElement) -> Painted<Unmirrored> {
    Painted(Unmirrored(child.into_any_element()))
}

impl Leaf for Unmirrored {
    type Prepaint = ();

    fn layout(&mut self, window: &mut Window, cx: &mut App) -> LayoutId {
        self.0.request_layout(window, cx)
    }

    fn prepaint(&mut self, _: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        self.0.prepaint(window, cx);
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        window.with_unmirrored(bounds, |window| self.0.paint(window, cx));
    }
}

/// Isolate sanitized name/path display text before shaping, never backend path bytes or paint.
pub fn isolate_auto(text: &str) -> String {
    isolate_display(text, '\u{2068}')
}

/// Keep a formatted number, date or resolved key label in LTR order, outside paint.
pub fn isolate_ltr(text: &str) -> String {
    isolate_display(text, '\u{2066}')
}

fn isolate_display(text: &str, opening: char) -> String {
    let closing = '\u{2069}';
    let mut output = String::with_capacity(text.len() + opening.len_utf8() + closing.len_utf8());
    output.push(opening);
    // Raw bidi controls and paragraph boundaries can escape the enclosing display isolate.
    output.extend(text.chars().filter(|c| {
        !c.is_control()
            && !matches!(*c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }));
    output.push(closing);
    output
}

#[cfg(test)]
#[path = "locale_geometry_mirror_tests.rs"]
mod mirror_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn rtl() -> Direction {
        Direction(true)
    }

    #[test]
    fn i50_rtl_rect_involution() {
        let container = rect(50., 20., 300., 100.);
        let logical = rect(65., 25., 40., 31.);
        assert_eq!(rtl().rect(container, logical), rect(295., 25., 40., 31.));
        for direction in [Direction::default(), rtl()] {
            for item in [
                logical,
                container,
                rect(40., 5., 0., 0.),
                rect(0., 0., 500., 200.),
            ] {
                assert_eq!(
                    direction.rect(container, direction.rect(container, item)),
                    item
                );
            }
        }
    }

    #[test]
    fn i50_rtl_hit_test_inverse() {
        let container = rect(50., 20., 300., 100.);
        let logical = rect(65., 25., 40., 31.);
        for direction in [Direction::default(), rtl()] {
            let painted = direction.rect(container, logical);
            for x in [
                49., 65., 66., 104., 105., 294., 295., 296., 334., 335., 350.,
            ] {
                for y in [24., 25., 26., 55., 56.] {
                    let p = point(px(x), px(y));
                    assert_eq!(direction.point(container, direction.point(container, p)), p);
                    let inside = p.x >= painted.origin.x
                        && p.x < painted.right()
                        && p.y >= painted.origin.y
                        && p.y < painted.bottom();
                    assert_eq!(direction.hit_test(container, logical, p), inside);
                }
            }
            assert!(!direction.hit_test(container, rect(65., 25., 0., 31.), painted.origin));
        }
        assert_eq!(rtl().visual_index(0, 4), Some(3));
        assert_eq!(rtl().visual_index(3, 4), Some(0));
        assert_eq!(rtl().visual_index(0, 0), None);
        assert_eq!(rtl().visual_index(usize::MAX, 4), None);
        for count in 1..8 {
            for index in 0..count {
                assert_eq!(
                    rtl().visual_index(rtl().visual_index(index, count).unwrap(), count),
                    Some(index)
                );
            }
        }
        assert_eq!(rtl().reading_step(true), -1);
        assert_eq!(rtl().reading_step(false), 1);
        assert_eq!(Direction::default().reading_step(true), 1);
        assert_eq!(Direction::default().reading_step(false), -1);
        for fraction in [0., 0.25, 0.5, 0.75, 1.] {
            let p = point(px(50. + 300. * fraction), px(25.));
            assert_eq!(rtl().point(container, p).x, px(350. - 300. * fraction));
        }
    }

    #[test]
    fn i50_rtl_numbers_isolated() {
        for value in [
            "22.1 kB",
            "۲۲٫۱",
            "2025-06-15 10:00",
            "ctrl+shift+1",
            "13 °C",
        ] {
            assert_eq!(isolate_ltr(value), format!("\u{2066}{value}\u{2069}"));
        }
        let name = "تقرير 2025.pdf";
        assert_eq!(isolate_auto(name), format!("\u{2068}{name}\u{2069}"));
        assert_eq!(isolate_auto("/בית/2025"), "\u{2068}/בית/2025\u{2069}");
    }

    // The shaper takes a line's direction from its first strong letter, so a hint opening on a keycap must still read right to left.
    #[test]
    fn i50_keycap_keeps_the_rtl_paragraph() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|dir| dir.join("src/keys/fixtures/keycaps"))
            .find(|dir| dir.is_dir())
            .expect("the repo's src/keys/fixtures/keycaps");
        let fa = flea::i18n::I18n::load(flea::i18n::Paths {
            omarchy: root.clone(),
            language_file: root.join("language-fa"),
            catalogs: vec![root.join("catalogs")],
        });
        let actions = ["textSizeUp", "textSizeDown", "textSizeReset"];
        let rtl = |text: &str| {
            unicode_bidi::BidiInfo::new(text, None).paragraphs[0]
                .level
                .is_rtl()
        };
        let hint = flea::keys::presentation::hinted_all(
            &fa,
            "settings.hint.text_keys",
            "default",
            &actions,
        );
        assert!(rtl(&hint), "{hint:?}");
        let raw = actions.map(|action| flea::keys::presentation::key_for("default", action));
        let raw = fa.tr_args(
            "settings.hint.text_keys",
            &raw.each_ref().map(String::as_str),
        );
        assert!(!rtl(&raw), "{raw:?}");
    }

    #[test]
    fn i50_rtl_nondirectional_icon_kept() {
        assert!(!rtl().mirrors_icon(IconRole::NonDirectional));
        assert!(!rtl().mirrors_icon(IconRole::Media));
        assert!(rtl().mirrors_icon(IconRole::Navigation));
        assert!(!Direction::default().mirrors_icon(IconRole::Navigation));
    }

    #[test]
    fn i50_rtl_four_codes() {
        let roster =
            flea::jsondoc::parse(include_str!("../../../src/i18n/fixtures/locales.json")).unwrap();
        let mut locale = flea::i18n::Locale::default();
        let mut codes = Vec::new();
        for (code, entry) in roster.as_object().unwrap() {
            locale.right_to_left =
                entry.get("direction").and_then(flea::jsondoc::Json::as_str) == Some("rtl");
            assert_eq!(
                Direction::from_locale(&locale).is_rtl(),
                locale.right_to_left
            );
            if Direction::from_locale(&locale).is_rtl() {
                codes.push(code.as_str());
            }
        }
        codes.sort_unstable();
        assert_eq!(codes, ["ar", "fa", "he", "ur"]);
    }

    #[test]
    fn i50_rtl_untrusted_isolate_controls() {
        let name = "\u{2069}אבג\u{202e}.pdf\u{2066}";
        assert_eq!(isolate_auto(name), "\u{2068}אבג.pdf\u{2069}");
        assert_eq!(isolate_ltr(name), "\u{2066}אבג.pdf\u{2069}");
        for control in [
            '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
            '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\n', '\r', '\u{0085}',
            '\u{2028}', '\u{2029}',
        ] {
            assert_eq!(isolate_auto(&format!("a{control}b")), "\u{2068}ab\u{2069}");
        }
        assert_eq!(
            isolate_auto("می\u{200c}روم"),
            "\u{2068}می\u{200c}روم\u{2069}"
        );
        assert_eq!(
            isolate_auto("ا\u{200d}بe\u{301}"),
            "\u{2068}ا\u{200d}بe\u{301}\u{2069}"
        );
    }
}
