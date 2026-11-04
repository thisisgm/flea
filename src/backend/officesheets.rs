// A spreadsheet becomes one cached CSV per sheet: LibreOffice's CSV filter in the office jail, one whole-workbook run for every sheet's cells, then one run per sheet for the workbook's order.
use super::*;

// UTF-8, comma and double quote, values as Calc shows them; option 12 is the sheet, -1 for every sheet as <stem>-<sheet>.csv (LibreOffice 7.2 and later).
const CSV_FILTER: &str =
    "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false";
// A sheets entry is its own cache key, so a workbook's PDF and its sheets never answer for each other.
pub const SHEETS_SUFFIX: &str = ".sheets";
const MANIFEST_HEADER: &str = "flea-office-sheets-v1\n";
// The order runs stop here, about 1.4 s each in the jail, so a larger workbook shows its first sheets inside CONVERT_TIMEOUT.
pub(super) const MAX_SHEETS: usize = 24;
const SPREADSHEET_TYPES: [&str; 3] = [OFFICE_TYPES[2], OFFICE_TYPES[3], OFFICE_TYPES[8]];

pub fn is_sheet(name: &str, mime: &Db, aliases: &Aliases) -> bool {
    mime.lookup(name)
        .is_some_and(|m| SPREADSHEET_TYPES.contains(&aliases.canonical(m)))
}

// Sample output: soffice ... --convert-to "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,2" --outdir /out /in.xlsx
pub fn sheets_argv(program: &str, input: &Path, outdir: &Path, sheet: i32) -> Vec<String> {
    convert_argv(program, &format!("{CSV_FILTER},{sheet}"), input, outdir)
}

pub type Sheets = Vec<(String, PathBuf)>;

impl Store {
    pub(super) fn csv_path(&self, key: &str, index: usize) -> PathBuf {
        self.dir.join(format!("{key}.{index}.csv"))
    }

    pub(super) fn manifest_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.txt"))
    }

    // Sample input, <key>.txt: flea-office-sheets-v1\n{"names":["Monthly","Savings"]}; every CSV must carry the workbook's mtime.
    pub(super) fn lookup_sheets(&self, key: &str, mtime: SystemTime) -> Hit<Sheets> {
        // The sheets key has no PDF beside it, so lookup answers only its failure marker or a miss.
        if let Hit::Failed(message) = self.lookup(key, mtime) {
            return Hit::Failed(message);
        }
        let manifest = self.manifest_path(key);
        if !Store::stamped(&manifest, mtime) {
            return Hit::Miss;
        }
        let Some(names) = std::fs::read_to_string(&manifest).ok().and_then(|b| {
            b.strip_prefix(MANIFEST_HEADER)
                .map(|j| crate::json::field_str_array(j, "names"))
        }) else {
            return Hit::Miss;
        };
        let sheets: Sheets = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, self.csv_path(key, i + 1)))
            .collect();
        if sheets.is_empty() || !sheets.iter().all(|(_, csv)| Store::stamped(csv, mtime)) {
            return Hit::Miss;
        }
        Hit::Ready(sheets)
    }

    pub(super) fn find(&self, key: &str, mtime: SystemTime, sheets: bool) -> Hit<Done> {
        if sheets {
            self.lookup_sheets(key, mtime).map(Done::Sheets)
        } else {
            self.lookup(key, mtime).map(Done::Pdf)
        }
    }

    // Every CSV before the manifest, so a reader never sees a manifest whose sheets are not all in place.
    fn publish_sheets(
        &self,
        key: &str,
        mtime: SystemTime,
        sheets: &Sheets,
    ) -> std::io::Result<Sheets> {
        let mut published = Vec::with_capacity(sheets.len());
        for (i, (name, produced)) in sheets.iter().enumerate() {
            let dest = self.csv_path(key, i + 1);
            let mut body = super::super::regfile::open_regular(produced).ok_or_else(|| {
                std::io::Error::other(format!("{} is not a regular file", produced.display()))
            })?;
            Store::publish(&dest, mtime, &mut body)?;
            published.push((name.clone(), dest));
        }
        let names: Vec<String> = sheets
            .iter()
            .map(|(name, _)| format!("\"{}\"", crate::json::escape(name)))
            .collect();
        let body = format!("{MANIFEST_HEADER}{{\"names\":[{}]}}", names.join(","));
        Store::publish(&self.manifest_path(key), mtime, &mut body.as_bytes())?;
        Ok(published)
    }
}

impl<T> Hit<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Hit<U> {
        match self {
            Hit::Ready(t) => Hit::Ready(f(t)),
            Hit::Failed(message) => Hit::Failed(message),
            Hit::Miss => Hit::Miss,
        }
    }
}

// Sample input: budget-Monthly.csv for stem budget; an empty sheet writes an empty CSV, so any regular file counts.
fn produced_csvs(dir: &Path, stem: &str) -> Sheets {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "csv")
                && std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
        })
        .filter_map(|p| {
            let file = p.file_stem()?.to_string_lossy().into_owned();
            let name = file
                .strip_prefix(stem)
                .and_then(|rest| rest.strip_prefix('-'))
                .unwrap_or(&file)
                .to_string();
            Some((name, p))
        })
        .collect()
}

impl Service {
    pub(super) fn run_sheets(&self, job: &Job) -> Result<Sheets, Fail> {
        let local = |e: std::io::Error| Fail::Local(crate::error::io_message(&e));
        // Every path this job may publish is checked before the first child, as the PDF run checks its one.
        crate::cachewrite::require(&self.store.manifest_path(&job.key)).map_err(local)?;
        (1..=MAX_SHEETS)
            .try_for_each(|i| crate::cachewrite::require(&self.store.csv_path(&job.key, i)))
            .map_err(local)?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.store.dir)
            .map_err(local)?;
        let mut work = Work::new(&self.store.dir, "office").map_err(|e| Fail::Local(e.msg))?;
        let stem = job
            .input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let end = Instant::now() + self.timeout;
        let all = work.dir.join("all");
        self.jailed(
            job,
            (self.sheet_build)(&job.program, &job.input, &all, -1),
            &mut work,
            end,
        )?;
        let mut left = produced_csvs(&all, &stem);
        if left.is_empty() {
            return Err(Fail::Converter(format!(
                "{PROGRAM} wrote no sheets of {}",
                job.input.display()
            )));
        }
        let count = left.len().min(MAX_SHEETS);
        let mut ordered = Vec::with_capacity(count);
        for n in 1..=count {
            if left.len() == 1 {
                ordered.append(&mut left);
                break;
            }
            let out = work.dir.join(format!("s{n}"));
            self.jailed(
                job,
                (self.sheet_build)(&job.program, &job.input, &out, n as i32),
                &mut work,
                end,
            )?;
            let one = produced_csvs(&out, &stem);
            let at = match one.as_slice() {
                [(name, _)] => left.iter().position(|(known, _)| known == name),
                _ => None,
            };
            let at = at.ok_or_else(|| {
                Fail::Converter(format!(
                    "{PROGRAM} wrote no known sheet {n} of {}",
                    job.input.display()
                ))
            })?;
            ordered.push(left.remove(at));
        }
        let published = self
            .store
            .publish_sheets(&job.key, job.mtime, &ordered)
            .map_err(local)?;
        self.store.prune(&self.store.dir);
        Ok(published)
    }
}

pub fn sheets_line(id: usize, sheets: &Sheets) -> String {
    let quote = |s: &str| format!("\"{}\"", crate::json::escape(s));
    let names: Vec<String> = sheets.iter().map(|(name, _)| quote(name)).collect();
    let paths: Vec<String> = sheets
        .iter()
        .map(|(_, csv)| quote(&csv.to_string_lossy()))
        .collect();
    format!(
        r#"{{"t":"officesheets","id":{},"state":"ready","names":[{}],"paths":[{}]}}"#,
        id,
        names.join(","),
        paths.join(",")
    )
}
