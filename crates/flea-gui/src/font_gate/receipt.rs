//! A test-only decorator records the real shaper's inputs and resolved face IDs.
use gpui::*;
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
pub(super) struct Run {
    pub text: String,
    pub requested: Vec<String>,
    pub resolved: Vec<(FontId, String)>,
}

#[derive(Default)]
struct State {
    enabled: bool,
    faces: HashMap<FontId, String>,
    requested: HashMap<FontId, String>,
    runs: Vec<Run>,
}

pub(super) struct Recorder {
    real: Arc<dyn PlatformTextSystem>,
    state: Mutex<State>,
}

impl Recorder {
    pub fn new() -> Arc<Self> {
        let real = gpui_platform::current_platform(true).text_system();
        let mut faces = HashMap::new();
        for family in [
            super::FAMILY,
            super::SECOND,
            super::FALLBACK,
            "IBM Plex Sans",
        ] {
            real.add_fonts(vec![
                Cow::Owned(super::test_font::named(
                    include_bytes!("../../tests/fonts/JetBrainsMono-Regular.ttf"),
                    family,
                )),
                Cow::Owned(super::test_font::named(
                    include_bytes!("../../tests/fonts/JetBrainsMono-Bold.ttf"),
                    family,
                )),
            ])
            .unwrap();
            assert!(
                real.all_font_names().contains(&family.to_owned()),
                "renamed family {family:?} absent: {:?}",
                real.all_font_names()
            );
            for descriptor in [font(family), font(family).bold()] {
                faces.insert(real.font_id(&descriptor).unwrap(), family.to_owned());
            }
        }
        assert!(real.all_font_names().contains(&super::FAMILY.to_owned()));
        assert_ne!(super::FAMILY, Font::default().family.as_ref());
        assert_ne!(
            real.font_id(&font(super::FAMILY)).unwrap(),
            TextSystem::new(real.clone()).resolve_font(&Font::default())
        );
        Arc::new(Self {
            real,
            state: Mutex::new(State {
                faces,
                ..State::default()
            }),
        })
    }

    pub fn begin(&self) {
        let mut state = self.state.lock().unwrap();
        state.runs.clear();
        state.enabled = true;
    }

    pub fn finish(&self) -> Vec<Run> {
        let mut state = self.state.lock().unwrap();
        state.enabled = false;
        std::mem::take(&mut state.runs)
    }

    pub fn contains(&self, text: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .runs
            .iter()
            .any(|run| run.text.contains(text))
    }
}

impl PlatformTextSystem for Recorder {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.real.add_fonts(fonts)
    }
    fn set_missing_glyph_sink(&self, sink: Option<Arc<dyn MissingGlyphSink>>) {
        self.real.set_missing_glyph_sink(sink);
    }
    fn all_font_names(&self) -> Vec<String> {
        self.real.all_font_names()
    }
    fn font_id(&self, descriptor: &Font) -> Result<FontId> {
        let id = self.real.font_id(descriptor)?;
        let mut state = self.state.lock().unwrap();
        state.requested.insert(id, descriptor.family.to_string());
        state
            .faces
            .entry(id)
            .or_insert_with(|| descriptor.family.to_string());
        Ok(id)
    }
    fn prewarm_fonts(&self, ids: &[FontId]) {
        self.real.prewarm_fonts(ids);
    }
    fn font_metrics(&self, id: FontId) -> FontMetrics {
        self.real.font_metrics(id)
    }
    fn typographic_bounds(&self, id: FontId, glyph: GlyphId) -> Result<Bounds<f32>> {
        self.real.typographic_bounds(id, glyph)
    }
    fn advance(&self, id: FontId, glyph: GlyphId) -> Result<Size<f32>> {
        self.real.advance(id, glyph)
    }
    fn glyph_for_char(&self, id: FontId, ch: char) -> Option<GlyphId> {
        self.real.glyph_for_char(id, ch)
    }
    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.real.glyph_raster_bounds(params)
    }
    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.real.rasterize_glyph(params, bounds)
    }
    fn recommended_rendering_mode(&self, id: FontId, size: Pixels) -> TextRenderingMode {
        self.real.recommended_rendering_mode(id, size)
    }
    fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        self.real.glyph_dilation_for_color(color)
    }
    fn layout_line(&self, text: &str, size: Pixels, requested: &[FontRun]) -> LineLayout {
        let line = self.real.layout_line(text, size, requested);
        let mut state = self.state.lock().unwrap();
        if state.enabled && !text.is_empty() {
            let family = |id: FontId| {
                state
                    .faces
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| format!("unresolved:{id:?}"))
            };
            let run = Run {
                text: text.into(),
                requested: requested
                    .iter()
                    .map(|run| {
                        state
                            .requested
                            .get(&run.font_id)
                            .cloned()
                            .unwrap_or_else(|| family(run.font_id))
                    })
                    .collect(),
                resolved: line
                    .runs
                    .iter()
                    .map(|run| (run.font_id, family(run.font_id)))
                    .collect(),
            };
            state.runs.push(run);
        }
        line
    }
}
