//! A captured selection keeps its listing order and identity through every output request.
use super::{
    output::{self, Kind},
    pages::Pages,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: String,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub generation: u64,
    pub menu: u64,
    pub files: Vec<Source>,
}

impl Selection {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(output::MIN_ITEMS..=output::MAX_ITEMS).contains(&self.files.len()) {
            return Err("dialogs.output.error.item_count");
        }
        let mut seen = std::collections::HashSet::new();
        for file in &self.files {
            if !Path::new(&file.path).is_absolute() || file.path.contains('\0') {
                return Err("dialogs.output.error.absolute");
            }
            if !seen.insert(&file.path) {
                return Err("dialogs.output.error.duplicate");
            }
        }
        Ok(())
    }

    pub fn kinds(&self) -> Vec<Kind> {
        self.files.iter().map(|f| f.kind).collect()
    }

    pub fn one_folder(&self) -> bool {
        self.files.first().is_some_and(|first| {
            self.files
                .iter()
                .all(|f| Path::new(&f.path).parent() == Path::new(&first.path).parent())
        })
    }

    pub fn output_paths(
        &self,
        format: &str,
        combined: bool,
        pages: &Pages,
        count: Option<u32>,
    ) -> Result<Vec<PathBuf>, &'static str> {
        self.validate()?;
        if !output::can_convert(&self.kinds()) {
            return Err("dialogs.output.error.convert_kind");
        }
        if !self.files[0].kind.formats().contains(&format) {
            return Err("dialogs.output.error.format");
        }
        if !self.one_folder() {
            return Err("dialogs.output.error.folder");
        }
        let first = Path::new(&self.files[0].path);
        if self.files[0].kind == Kind::Pdf {
            let selected = pages
                .selected(count.ok_or("dialogs.output.error.page_count")?)
                .map_err(|_| "dialogs.output.error.pages")?;
            return Ok(selected
                .into_iter()
                .map(|n| first.with_file_name(output::page_name(first, n, format)))
                .collect());
        }
        let files = if format == "pdf" && combined {
            &self.files[..1]
        } else {
            &self.files
        };
        let mut seen = std::collections::HashSet::new();
        files
            .iter()
            .map(|f| {
                let path = Path::new(&f.path);
                let dest = path.with_file_name(output::converted_name(path, format));
                if !seen.insert(dest.clone()) {
                    return Err("dialogs.output.error.same_name");
                }
                Ok(dest)
            })
            .collect()
    }
}

pub fn default_format(source: &Source) -> &'static str {
    let current = output::extension(Path::new(&source.path));
    source
        .kind
        .formats()
        .iter()
        .copied()
        .find(|f| *f != current)
        .unwrap_or("pdf")
}
