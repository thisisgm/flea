// The backend command geometry and the live card use one composition contract.
use super::output;
use std::path::{Path, PathBuf};

pub const MAX_DIMENSION: u32 = 50_000;
pub const MIN_GRID_COLUMNS: usize = 1;
pub const MAX_GRID_COLUMNS: usize = output::MAX_ITEMS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
    Grid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    None,
    Small,
    Large,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Transparent,
    White,
    Black,
}

impl Spacing {
    pub fn pixels(self, edge: u32) -> u32 {
        let percent = match self {
            Self::None => 0,
            Self::Small => 3,
            Self::Large => 8,
        };
        let scaled = u64::from(edge) * percent;
        let whole = scaled / 100;
        let round_up = scaled % 100 > 50 || (scaled % 100 == 50 && whole % 2 == 1);
        (whole + u64::from(round_up)).min(u32::MAX as u64) as u32
    }
}

impl Background {
    pub fn ink(self) -> &'static str {
        match self {
            Self::Transparent => "none",
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

pub fn background_enabled(layout: Layout, spacing: Spacing, count: usize, columns: usize) -> bool {
    spacing != Spacing::None || (layout == Layout::Grid && (columns == 0 || count % columns != 0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub layout: Layout,
    pub spacing: Spacing,
    pub background: Background,
    pub columns: usize,
}

pub fn shared_size(layout: Layout, sizes: &[(u32, u32)]) -> Result<(u32, u32), String> {
    let first = *sizes
        .first()
        .ok_or("The image dimensions could not be used.")?;
    if sizes
        .iter()
        .any(|(w, h)| *w == 0 || *h == 0 || *w > MAX_DIMENSION || *h > MAX_DIMENSION)
    {
        return Err("The image dimensions could not be used.".into());
    }
    Ok(match layout {
        Layout::Row => (first.0, sizes.iter().map(|(_, h)| *h).min().unwrap()),
        Layout::Column => (sizes.iter().map(|(w, _)| *w).min().unwrap(), first.1),
        Layout::Grid => (
            sizes.iter().map(|(w, _)| *w).max().unwrap(),
            sizes.iter().map(|(_, h)| *h).max().unwrap(),
        ),
    })
}

// ImageMagick rounds positive resize dimensions to the nearest pixel and keeps at least one pixel.
pub fn resized_size(layout: Layout, source: (u32, u32), shared: (u32, u32)) -> (u32, u32) {
    let (w, h) = (f64::from(source.0), f64::from(source.1));
    let (cw, ch) = (f64::from(shared.0), f64::from(shared.1));
    let scale = match layout {
        Layout::Row => ch / h,
        Layout::Column => cw / w,
        Layout::Grid => (cw / w).min(ch / h),
    };
    (
        (w * scale).round().max(1.) as u32,
        (h * scale).round().max(1.) as u32,
    )
}

impl Settings {
    pub fn parse(
        layout: &str,
        spacing: &str,
        background: &str,
        format: &str,
        columns: usize,
    ) -> Result<Self, String> {
        let layout = match layout {
            "Row" => Layout::Row,
            "Column" => Layout::Column,
            "Grid" => Layout::Grid,
            _ => return Err("Choose Row, Column or Grid.".into()),
        };
        let spacing = match spacing {
            "None" => Spacing::None,
            "Small" => Spacing::Small,
            "Large" => Spacing::Large,
            _ => return Err("Choose None, Small or Large spacing.".into()),
        };
        let background = match background {
            "Transparent" => Background::Transparent,
            "White" => Background::White,
            "Black" => Background::Black,
            _ => return Err("Choose Transparent, White or Black.".into()),
        };
        if !output::COMBINE_FORMATS.contains(&format) {
            return Err("Choose PNG, JPEG, WebP or PDF.".into());
        }
        if background == Background::Transparent && format == "jpg" {
            return Err("JPEG does not support a transparent background.".into());
        }
        if layout == Layout::Grid && !(MIN_GRID_COLUMNS..=MAX_GRID_COLUMNS).contains(&columns) {
            return Err(format!(
                "Columns must be between {MIN_GRID_COLUMNS} and {MAX_GRID_COLUMNS}."
            ));
        }
        Ok(Self {
            layout,
            spacing,
            background,
            columns,
        })
    }

    pub fn edge(&self, size: (u32, u32)) -> u32 {
        match self.layout {
            Layout::Row => size.1,
            Layout::Column => size.0,
            Layout::Grid => size.0.min(size.1),
        }
    }
}

pub fn scale_argv(
    input: &Path,
    output: &Path,
    size: (u32, u32),
    settings: &Settings,
    first: bool,
) -> Vec<String> {
    let mut args = vec![
        "magick".into(),
        output::MAGICK_LITERAL[0].into(),
        output::MAGICK_LITERAL[1].into(),
        "-read".into(),
        output::magick_first_frame(input),
        "-auto-orient".into(),
        "-resize".into(),
    ];
    args.push(match settings.layout {
        Layout::Row => format!("x{}", size.1),
        Layout::Column => size.0.to_string(),
        Layout::Grid => format!("{}x{}", size.0, size.1),
    });
    args.extend(["-background".into(), settings.background.ink().into()]);
    let spacing = settings.spacing.pixels(settings.edge(size));
    if settings.layout == Layout::Grid {
        args.extend([
            "-gravity".into(),
            "center".into(),
            "-extent".into(),
            format!("{}x{}", size.0, size.1),
        ]);
        if spacing > 0 {
            args.extend([
                "-gravity".into(),
                "northwest".into(),
                "-extent".into(),
                format!("{}x{}", size.0 + spacing, size.1 + spacing),
            ]);
        }
    } else if !first && spacing > 0 {
        let (gravity, geometry) = if settings.layout == Layout::Row {
            ("West", format!("{spacing}x0"))
        } else {
            ("North", format!("0x{spacing}"))
        };
        args.extend([
            "-gravity".into(),
            gravity.into(),
            "-splice".into(),
            geometry,
        ]);
    }
    args.push(output::magick_output(output));
    args
}

pub fn join_argv(
    inputs: &[PathBuf],
    output: &Path,
    size: (u32, u32),
    settings: &Settings,
) -> Vec<String> {
    let mut args = vec![if settings.layout == Layout::Grid {
        "montage"
    } else {
        "magick"
    }
    .into()];
    args.extend(output::MAGICK_LITERAL.map(String::from));
    let read = if settings.layout == Layout::Grid {
        "--"
    } else {
        "-read"
    };
    for input in inputs {
        args.extend([read.into(), output::magick_input(input)]);
    }
    args.extend(["-background".into(), settings.background.ink().into()]);
    match settings.layout {
        Layout::Row => args.push("+append".into()),
        Layout::Column => args.push("-append".into()),
        Layout::Grid => {
            args.extend([
                "-tile".into(),
                format!("{}x", settings.columns),
                "-geometry".into(),
                "+0+0".into(),
            ]);
        }
    }
    let spacing = settings.spacing.pixels(settings.edge(size));
    if settings.layout != Layout::Grid && spacing > 0 {
        args.extend([
            "-bordercolor".into(),
            settings.background.ink().into(),
            "-border".into(),
            spacing.to_string(),
        ]);
    }
    finish_format(&mut args, output);
    args.push(output::magick_output(output));
    args
}

// JPEG has no alpha. A PDF page takes the virtual canvas, which +append leaves at the first image's size,
// and a gravity still set places the image on a Letter page, so both are reset.
fn finish_format(args: &mut Vec<String>, output: &Path) {
    match output::extension(output).as_str() {
        "jpg" => args.extend([
            "-alpha".into(),
            "remove".into(),
            "-alpha".into(),
            "off".into(),
        ]),
        "pdf" => args.extend(["+repage".into(), "+gravity".into()]),
        _ => {}
    }
}

pub fn join_commands(
    inputs: &[PathBuf],
    output: &Path,
    size: (u32, u32),
    settings: &Settings,
) -> Vec<Vec<String>> {
    let spacing = settings.spacing.pixels(settings.edge(size));
    if settings.layout != Layout::Grid || spacing == 0 {
        return vec![join_argv(inputs, output, size, settings)];
    }
    let intermediate = output.with_file_name(format!(
        "{}.grid.png",
        output.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut finish = vec![
        "magick".into(),
        output::MAGICK_LITERAL[0].into(),
        output::MAGICK_LITERAL[1].into(),
        "-read".into(),
        output::magick_input(&intermediate),
        "-background".into(),
        settings.background.ink().into(),
        "-gravity".into(),
        "northwest".into(),
        "-splice".into(),
        format!("{spacing}x{spacing}"),
    ];
    finish_format(&mut finish, output);
    finish.push(output::magick_output(output));
    vec![join_argv(inputs, &intermediate, size, settings), finish]
}
