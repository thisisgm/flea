//! The existing tag transaction uses held menu objects or ordinary listing paths.
use super::*;
use crate::backend::{menu_actions::Selected, menutags_io::Pinned, tags};
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;

pub(super) struct Target {
    pub(super) path: PathBuf,
    pin: Option<Pinned>,
}

pub(super) fn capture(
    paths: Vec<PathBuf>,
    selected: Option<Vec<Selected>>,
) -> Result<Vec<Target>, FleaError> {
    let Some(selected) = selected else {
        return Ok(paths
            .into_iter()
            .map(|path| Target { path, pin: None })
            .collect());
    };
    if paths.len() != selected.len() {
        return Err(FleaError {
            where_: "settags".into(),
            path: String::new(),
            msg: super::super::menu_actions::SELECTION_CHANGED.into(),
        });
    }
    paths
        .into_iter()
        .zip(selected)
        .map(|(path, item)| {
            let failure = |msg| FleaError {
                where_: "settags".into(),
                path: path.to_string_lossy().into_owned(),
                msg,
            };
            if path != item.path {
                return Err(failure(
                    super::super::menu_actions::SELECTION_CHANGED.into(),
                ));
            }
            let pin = Pinned::open(&item).map_err(failure)?;
            Ok(Target {
                path,
                pin: Some(pin),
            })
        })
        .collect()
}

impl Target {
    fn metadata(&self) -> std::io::Result<Metadata> {
        match &self.pin {
            Some(pin) => pin.metadata(),
            None => self.path.symlink_metadata(),
        }
    }

    fn read(&self) -> Result<Option<Vec<u8>>, FleaError> {
        match &self.pin {
            Some(pin) => pin.read_raw(),
            None => tags::read_raw(&self.path),
        }
    }

    fn write(&self, names: &[String]) -> Result<(), FleaError> {
        match &self.pin {
            Some(pin) => pin.write_names(names),
            None => tags::write_names(&self.path, names),
        }
    }

    fn restore(&self, raw: &Option<Vec<u8>>) -> Result<(), FleaError> {
        match &self.pin {
            Some(pin) => pin.restore(raw),
            None => tags::restore(&self.path, raw),
        }
    }

    fn generation(&self) -> Option<u64> {
        match &self.pin {
            Some(pin) => pin.generation(),
            None => super::super::filegen::of_path(&self.path),
        }
    }
}

// Every file or none: probe and read every target before writing, then roll back through the same held objects.
pub(super) fn write(base: &Path, targets: Vec<Target>, op: &str, tag: &str) -> Written {
    let mut skipped = Vec::new();
    let targets: Vec<_> = targets
        .into_iter()
        .filter(|target| {
            let taggable = target
                .metadata()
                .map_or(true, |meta| meta.is_file() || meta.is_dir());
            if !taggable {
                skipped.push(row_name(base, &target.path));
            }
            taggable
        })
        .collect();
    let mut devs = HashSet::new();
    let mut befores = Vec::with_capacity(targets.len());
    for target in &targets {
        let missing = |error: FleaError| FleaError {
            where_: "settags".into(),
            path: target.path.to_string_lossy().into_owned(),
            msg: error.msg,
        };
        let meta = target.metadata().map_err(|error| {
            crate::error::from_io("settags", &target.path.to_string_lossy(), &error)
        })?;
        if devs.insert(meta.dev()) {
            if let Some(refused) = tags::support_error(&target.path) {
                return Err(refused);
            }
        }
        let born = super::super::permissions::born_of(&meta);
        befores.push((
            target.read().map_err(missing)?,
            meta.dev(),
            meta.ino(),
            (born, target.generation()),
        ));
    }
    let carried: Vec<Vec<String>> = befores
        .iter()
        .map(|(raw, ..)| raw.as_ref().map(|raw| tags::parse(raw)).unwrap_or_default())
        .collect();
    let op = resolved(op, tag, &carried);
    let mut steps = Vec::new();
    for ((target, (before, dev, ino, (born, generation))), names) in
        targets.iter().zip(befores).zip(carried)
    {
        let after = match tags::apply(&names, op, tag) {
            Ok(after) => after,
            Err(error) => {
                rollback(&targets, &steps);
                return Err(error);
            }
        };
        if names == after {
            continue;
        }
        if let Err(error) = target.write(&after) {
            rollback(&targets, &steps);
            return Err(FleaError {
                where_: "settags".into(),
                path: target.path.to_string_lossy().into_owned(),
                msg: error.msg,
            });
        }
        let after = (!after.is_empty()).then(|| tags::join(&after));
        steps.push(Step::Tags {
            path: target.path.clone(),
            before,
            after,
            dev,
            ino,
            born,
            generation,
        });
    }
    Ok((steps.len(), steps, op, skipped))
}

fn rollback(targets: &[Target], steps: &[Step]) {
    for step in steps.iter().rev() {
        if let Step::Tags { path, before, .. } = step {
            if let Some(target) = targets.iter().find(|target| target.path == *path) {
                let _ = target.restore(before);
            }
        }
    }
}
