//! Application icon resources resolve off-thread; the painter reuses source keys between frames.
use flea::ui_logic_op3::openwith::icons::Icons;
use gpui::{App, Asset, Global, ImageAssetLoader, RenderImage, Resource, Window};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Source {
    name: Arc<str>,
    size: u32,
    scale: u32,
}
struct Resolve;
impl Asset for Resolve {
    type Source = Source;
    type Output = Option<Resource>;
    fn load(
        source: Source,
        cx: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        cx.background_executor().spawn(async move {
            Icons::session()
                .resolve(&source.name, source.size, source.scale)
                .map(|path| Resource::Path(Arc::from(path)))
        })
    }
}

#[derive(Default)]
struct Sources(HashMap<String, HashMap<(u32, u32), Source>>);
impl Global for Sources {}

pub(crate) fn image(
    name: &str,
    size: u32,
    scale: u32,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    if !cx.has_global::<Sources>() {
        cx.set_global(Sources::default());
    }
    let sources = cx.global_mut::<Sources>();
    if sources.0.len() >= 256 && !sources.0.contains_key(name) {
        sources.0.clear();
    }
    if !sources.0.contains_key(name) {
        sources.0.insert(name.into(), HashMap::new());
    }
    let variants = sources.0.get_mut(name).expect("source inserted");
    let source = variants
        .entry((size, scale))
        .or_insert_with(|| Source {
            name: name.into(),
            size,
            scale,
        })
        .clone();
    let resource = window.use_asset::<Resolve>(&source, cx)??;
    window.use_asset::<ImageAssetLoader>(&resource, cx)?.ok()
}

pub(crate) struct Icon {
    name: String,
    side: f32,
    ink: gpui::Hsla,
}
impl Icon {
    pub(crate) fn new(name: &str, side: f32, ink: gpui::Hsla) -> Self {
        Self {
            name: name.into(),
            side,
            ink,
        }
    }
}
impl gpui::IntoElement for Icon {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl gpui::Element for Icon {
    type RequestLayoutState = ();
    type PrepaintState = Option<Arc<RenderImage>>;
    fn id(&self) -> Option<gpui::ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, ()) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::px(self.side).into();
        style.size.height = gpui::px(self.side).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        image(
            &self.name,
            self.side.round().max(1.) as u32,
            window.scale_factor().ceil().max(1.) as u32,
            window,
            cx,
        )
    }
    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        image: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(image) = image {
            let fit = gpui::ObjectFit::Contain.get_bounds(bounds, image.size(0));
            let _ = window.paint_image(
                bounds,
                fit,
                gpui::Corners::default(),
                image.clone(),
                0,
                false,
            );
        } else {
            let (key, data) = crate::g2::mn1::glyphs::mark("app-window");
            let _ = window.paint_svg(
                bounds,
                key,
                Some(data),
                gpui::TransformationMatrix::unit(),
                self.ink,
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2::mn1::{
        element::MenuElement,
        lines::Env,
        menu::{Face, Menu},
    };
    use crate::list::Typeface;
    use flea::ui_logic_mn1::{build::App as Application, fit::Rect, flyouts::open_with_entries};
    use gpui::{prelude::*, px, Context, Render, TestAppContext};
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
        rc::Rc,
    };

    struct Probe(Rc<RefCell<Menu>>);
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            MenuElement::new(self.0.clone(), px(0.))
        }
    }

    fn root() -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-g2rows-03");
        std::fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(base.join("menu-icons.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(base));
        std::fs::write(root.join(".flea-test-sandbox"), "fixture\n").unwrap();
        root
    }

    #[gpui::test]
    fn m9_g2rows_03_menu_paints_each_applications_own_icon(cx: &mut TestAppContext) {
        let root = root();
        let icons: Vec<_> = ["first", "second"].iter().map(|name| {
            let path = root.join(format!("{name}.svg"));
            std::fs::write(&path, format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"><rect width="{}" height="18" fill="red"/></svg>"#, if *name == "first" { 9 } else { 18 })).unwrap();
            path.to_string_lossy().into_owned()
        }).collect();
        let apps: Vec<_> = icons
            .iter()
            .enumerate()
            .map(|(at, icon)| Application {
                id: format!("{at}.desktop"),
                label: format!("Fixture {at}"),
                icon: icon.clone(),
                default: false,
            })
            .collect();
        let menu = Rc::new(RefCell::new(Menu::default()));
        let shared = menu.clone();
        let handle = cx.add_window(move |_, _| Probe(shared));
        handle
            .update(cx, |_, window, _| {
                let font = Typeface::omarchy_default();
                let catalog = flea::i18n::I18n::default();
                let env = Env {
                    catalog: &catalog,
                    preset: "default",
                    key_hints: false,
                    typeface: &font,
                    inset: 4.,
                };
                menu.borrow_mut().open(
                    Face::File,
                    open_with_entries(&apps),
                    (20., 20.),
                    Rect {
                        x: 0.,
                        y: 0.,
                        w: 800.,
                        h: 600.,
                    },
                    "fixture",
                    &env,
                    window,
                );
            })
            .unwrap();
        let mut images = Vec::new();
        for _ in 0..12 {
            cx.run_until_parked();
            images = handle
                .update(cx, |_, window, cx| {
                    let images: Vec<_> = icons
                        .iter()
                        .filter_map(|name| {
                            let source = Source {
                                name: name.as_str().into(),
                                size: 16,
                                scale: 1,
                            };
                            let resource = cx.fetch_asset::<Resolve>(&source)??;
                            cx.fetch_asset::<ImageAssetLoader>(&resource)?.ok()
                        })
                        .collect();
                    window.refresh();
                    window.simulate_next_frame(cx);
                    images
                })
                .unwrap();
            if images.len() == 2 {
                break;
            }
        }
        assert_eq!(images.len(), 2, "both fixture icons decode");
        assert!(!Arc::ptr_eq(&images[0], &images[1]));
        handle.update(cx, |_, window, _| {
            for image in &images {
                assert!(window.has_image_atlas_entry(image), "the actual menu painter must upload each app icon instead of the generic glyph");
            }
        }).unwrap();
    }
}
