use super::{number, object};
use crate::window::startup::View;
use flea::jsondoc::{self, Json};

const MANIFESTS: &[&str] = include!(concat!(env!("OUT_DIR"), "/visual-scenes.rs"));

pub(crate) struct Spec {
    pub name: String,
    pub path: String,
    pub view: View,
    pub width: u32,
    pub height: u32,
    pub corner: u32,
    pub cursor: usize,
    pub selection: Vec<usize>,
    pub text_size: u32,
    pub reduced_motion: bool,
    pub language: Option<Language>,
}

/// The catalog language a scene asserts the window runs, and the surface and control state it asks to see.
pub(crate) struct Language {
    pub code: String,
    pub surface: String,
    pub state: String,
}

/// What the window draws at the presented frame, read back rather than restated from the manifest.
#[derive(Default)]
pub(crate) struct Live {
    pub scale: f32,
    pub text_size: f32,
    pub row_height: f32,
    pub reduced_motion: bool,
    pub cursor: usize,
    pub selection: Vec<usize>,
    pub language: String,
    pub locale_revision: u64,
}

/// A roster code such as de, fil or zh-CN.
fn language_code(code: &str) -> bool {
    let (base, region) = code.split_once('-').unwrap_or((code, "AA"));
    (2..=3).contains(&base.len())
        && base.bytes().all(|byte| byte.is_ascii_lowercase())
        && region.len() == 2
        && region.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn index(value: &Json) -> Option<usize> {
    value
        .as_f64()
        .filter(|n| n.fract() == 0. && *n >= 0. && *n <= f64::from(u32::MAX))
        .map(|n| n as usize)
}

fn dimension(settings: &Json, key: &str, default: u32, max: u32) -> Result<u32, &'static str> {
    let n = settings
        .get(key)
        .map(|value| value.as_f64().ok_or("visual.settings"))
        .transpose()?
        .unwrap_or(f64::from(default));
    if n.fract() != 0. || !(1. ..=f64::from(max)).contains(&n) {
        return Err("visual.settings");
    }
    Ok(n as u32)
}

impl Spec {
    pub fn parse(window: &str, name: &str, settings: &str) -> Result<Self, &'static str> {
        if window != std::process::id().to_string() {
            return Err("visual.window");
        }
        let scene = MANIFESTS
            .iter()
            .find_map(|text| {
                let manifest = jsondoc::parse(text).expect("committed scene manifest");
                manifest
                    .get("scenes")
                    .and_then(Json::as_array)
                    .and_then(|scenes| {
                        scenes
                            .iter()
                            .find(|scene| scene.get("id").and_then(Json::as_str) == Some(name))
                            .cloned()
                    })
            })
            .ok_or("visual.unknown")?;
        let settings = jsondoc::parse(settings).map_err(|_| "visual.settings")?;
        Self::from_scene(name, &scene, &settings)
    }

    /// Reads one manifest scene and the caller's capture settings; every refusal is visual.settings.
    pub fn from_scene(name: &str, scene: &Json, settings: &Json) -> Result<Self, &'static str> {
        if !settings.as_object().is_some_and(|pairs| {
            pairs
                .iter()
                .all(|(key, _)| ["path", "width", "height"].contains(&key.as_str()))
        }) {
            return Err("visual.settings");
        }
        let path = settings
            .get("path")
            .and_then(Json::as_str)
            .filter(|path| path.starts_with('/') && !path.contains('\0'))
            .ok_or("visual.settings")?
            .to_owned();
        let cursor = scene
            .get("cursor")
            .and_then(index)
            .ok_or("visual.settings")?;
        let mut selection = scene
            .get("selection")
            .and_then(Json::as_array)
            .ok_or("visual.settings")?
            .iter()
            .map(|value| index(value).ok_or("visual.settings"))
            .collect::<Result<Vec<_>, _>>()?;
        selection.sort_unstable();
        selection.dedup();
        let text_size = scene
            .get("text_size")
            .and_then(index)
            .filter(|size| (8..=32).contains(size))
            .ok_or("visual.settings")? as u32;
        let reduced_motion = match scene.get("reduced_motion") {
            Some(Json::Bool(value)) => *value,
            _ => return Err("visual.settings"),
        };
        // This window draws one row density and no caret, so a scene asking for another cannot be honoured.
        if scene.get("density").and_then(Json::as_str) != Some(DENSITY)
            || scene.get("caret_blink") != Some(&Json::Bool(false))
        {
            return Err("visual.settings");
        }
        let language = match (
            scene.get("language"),
            scene.get("surface"),
            scene.get("state"),
        ) {
            (None, None, None) => None,
            (Some(Json::Str(code)), Some(Json::Str(surface)), Some(Json::Str(state))) => {
                // This window draws only the listing at rest for a language scene; other surfaces need their own state readback.
                if !language_code(code) || surface != "list" || state != "rest" {
                    return Err("visual.settings");
                }
                Some(Language {
                    code: code.clone(),
                    surface: surface.clone(),
                    state: state.clone(),
                })
            }
            _ => return Err("visual.settings"),
        };
        let view = match scene.get("view").and_then(Json::as_str) {
            Some("columns") => View::Columns,
            Some("list") => View::List,
            Some("grid") => View::Grid,
            Some("dual") => View::Dual,
            _ => return Err("visual.settings"),
        };
        Ok(Self {
            name: name.into(),
            path,
            view,
            cursor,
            selection,
            text_size,
            reduced_motion,
            language,
            width: dimension(settings, "width", 1100, 2560)?,
            height: dimension(settings, "height", 700, 1440)?,
            corner: scene
                .get("corner_radius")
                .and_then(index)
                .ok_or("visual.settings")? as u32,
        })
    }

    pub fn receipt(&self, frame: u64, live: &Live) -> Json {
        let mut pairs = vec![
            ("scene", Json::Str(self.name.clone())),
            ("window", number(std::process::id())),
            ("frame", number(frame)),
            ("view", Json::Str(super::view_name(self.view).into())),
            ("path", Json::Str(self.path.clone())),
            ("width", number(self.width)),
            ("height", number(self.height)),
            ("scale", number(live.scale)),
            ("text_size", number(live.text_size)),
            ("density", Json::Str(density(live.row_height).into())),
            ("corner_radius", number(self.corner)),
            ("cursor", number(live.cursor)),
            (
                "selection",
                Json::Arr(live.selection.iter().map(number).collect()),
            ),
            ("reduced_motion", Json::Bool(live.reduced_motion)),
            ("caret_blink", Json::Bool(false)),
        ];
        // The language is the one the drawn frame's catalog ran, never the manifest's; scenes without one keep their receipt.
        if let Some(language) = &self.language {
            pairs.extend([
                ("language", Json::Str(live.language.clone())),
                ("locale_revision", number(live.locale_revision)),
                ("surface", Json::Str(language.surface.clone())),
                ("state", Json::Str(language.state.clone())),
            ]);
        }
        object(pairs)
    }
}

const DENSITY: &str = "compact";

/// Names the density the drawn row height belongs to; board list.html's Compact row is 31 at text 14.
pub(crate) fn density(row_height: f32) -> &'static str {
    if row_height == 31. {
        DENSITY
    } else {
        "unknown"
    }
}
