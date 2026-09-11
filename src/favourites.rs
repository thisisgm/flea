// Favourite edits operate on the latest state under the ui.json lock; GTK bookmarks are never opened.
use crate::jsondoc::{self, Json};
use crate::{uistate, uistore};

pub fn command(args: &[String]) -> i32 {
    if args.len() != 3 {
        eprintln!("flea: --favourites takes one JSON operation");
        return 2;
    }
    let result = jsondoc::parse(&args[2])
        .map_err(|error| format!("favorites operation is not JSON ({})", error))
        .and_then(|operation| {
            let store = uistore::Store::user()?;
            if operation.get("op").and_then(Json::as_str) == Some("inspect") {
                inspect(&store.read(), &operation)
            } else {
                store.transform(|state| changed(state, &operation))
            }
        });
    match result {
        Ok(state) => {
            print!("{}", jsondoc::render(&state));
            0
        }
        Err(error) => {
            eprintln!("flea: {}", error);
            2
        }
    }
}

// The caller supplies only visible indices; one bounded metadata read per local favourite.
fn inspect(state: &Json, operation: &Json) -> Result<Json, String> {
    let records = state
        .get("places")
        .and_then(|places| places.get("favourites"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    let indices = operation
        .get("indices")
        .and_then(Json::as_array)
        .ok_or("favorite inspection needs indices")?;
    const MAX_VISIBLE: usize = 128;
    if indices.len() > MAX_VISIBLE {
        return Err("too many favorites requested for one viewport".into());
    }
    let mut statuses = Vec::new();
    for item in indices {
        let number = item.as_f64().ok_or("favorite index must be a number")?;
        if number < 0.0 || number.fract() != 0.0 || number >= records.len() as f64 {
            continue;
        }
        let index = number as usize;
        let record = &records[index];
        let mut error = String::new();
        if let Some(path) = record.get("path").and_then(Json::as_str) {
            let local = if path == "~" || path.starts_with("~/") {
                std::env::var("HOME")
                    .ok()
                    .map(|home| format!("{}{}", home, &path[1..]))
            } else if path.starts_with('/') {
                Some(path.into())
            } else {
                None
            };
            if let Some(local) = local {
                match std::fs::metadata(&local) {
                    Ok(metadata) if metadata.is_dir() => {}
                    Ok(_) => error = "not a directory".into(),
                    Err(failure) => error = format!("unavailable ({:?})", failure.kind()),
                }
            }
        }
        statuses.push(Json::Obj(vec![
            ("index".into(), Json::Num(index.to_string())),
            ("record".into(), record.clone()),
            ("error".into(), Json::Str(error)),
        ]));
    }
    Ok(Json::Obj(vec![("statuses".into(), Json::Arr(statuses))]))
}

pub fn changed(state: &Json, operation: &Json) -> Result<Json, String> {
    let current = state
        .get("places")
        .and_then(|places| places.get("favourites"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    let mut next = current.to_vec();
    match operation.get("op").and_then(Json::as_str) {
        Some("add") => {
            let record = operation
                .get("record")
                .ok_or("favorites add needs a record")?;
            let label = record
                .get("label")
                .and_then(Json::as_str)
                .ok_or("favorite label must be text")?;
            let path = record
                .get("path")
                .and_then(Json::as_str)
                .ok_or("favorite path must be text")?;
            if !label
                .chars()
                .any(|character| !character.is_whitespace() && !character.is_control())
            {
                return Err("favorite label needs visible text".into());
            }
            if !valid_path(path) {
                return Err(
                    "favorite path must be absolute, ~/ relative, or a supported URI".into(),
                );
            }
            next.push(record.clone());
        }
        Some(action @ ("remove" | "move" | "rename")) => {
            if operation.get("expected").and_then(Json::as_array) != Some(current) {
                return Err("Favorites changed in another window; refresh before editing".into());
            }
            let index = index(operation, "index", next.len())?;
            match action {
                "remove" => {
                    next.remove(index);
                }
                "move" => {
                    let to = index_value(operation, "to")?;
                    if to >= next.len() {
                        return Err("favorite move destination is outside the list".into());
                    }
                    let record = next.remove(index);
                    next.insert(to, record);
                }
                _ => {
                    let label = operation
                        .get("label")
                        .and_then(Json::as_str)
                        .ok_or("favorite label must be text")?;
                    if label.trim().is_empty() {
                        return Err("favorite label needs visible text".into());
                    }
                    let record = next[index]
                        .as_object()
                        .ok_or("invalid favorite cannot be renamed; remove it explicitly")?;
                    let mut record = record.to_vec();
                    if let Some(pair) = record.iter_mut().find(|(key, _)| key == "label") {
                        pair.1 = Json::Str(label.into());
                    } else {
                        record.push(("label".into(), Json::Str(label.into())));
                    }
                    next[index] = Json::Obj(record);
                }
            }
        }
        _ => return Err("unknown favorites operation".into()),
    }
    uistate::patched(
        state,
        &Json::Obj(vec![(
            "places".into(),
            Json::Obj(vec![("favourites".into(), Json::Arr(next))]),
        )]),
    )
}

fn index_value(operation: &Json, key: &str) -> Result<usize, String> {
    let value = operation
        .get(key)
        .and_then(Json::as_f64)
        .ok_or_else(|| format!("favorite {} must be an index", key))?;
    if value < 0.0 || value.fract() != 0.0 || value > usize::MAX as f64 {
        return Err(format!("favorite {} must be a non-negative integer", key));
    }
    Ok(value as usize)
}

fn index(operation: &Json, key: &str, len: usize) -> Result<usize, String> {
    let index = index_value(operation, key)?;
    if index >= len {
        return Err("favorite index is outside the list".into());
    }
    Ok(index)
}

fn valid_path(path: &str) -> bool {
    if path.chars().any(char::is_control) {
        return false;
    }
    if path.starts_with('/') || path == "~" || path.starts_with("~/") {
        return true;
    }
    [
        // Every scheme ui/js/Protocols.js can build, ftps included: the dialog offers an FTPS chip,
        // and a location mounted through it could not be saved because this list had only ftp.
        "smb://", "sftp://", "ftp://", "ftps://", "dav://", "davs://", "afp://", "nfs://", "file://",
    ]
    .iter()
    .any(|scheme| {
        path.strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> Json {
        jsondoc::parse(text).expect("fixture JSON")
    }

    #[test]
    fn simultaneous_appends_merge_under_the_existing_state_lock() {
        let sandbox = crate::backend::testdir::TestDir::new("favourites-concurrent");
        let store = uistore::Store::at(&sandbox.dir("state"), &sandbox.dir("config"));
        std::thread::scope(|scope| {
            let mut jobs = Vec::new();
            for _ in 0..4 {
                jobs.push(scope.spawn(|| {
                    store.transform(|state| {
                        changed(
                            state,
                            &parse(r#"{"op":"add","record":{"label":"A","path":"/a"}}"#),
                        )
                    })
                }));
            }
            for job in jobs {
                job.join().unwrap().unwrap();
            }
        });
        assert_eq!(
            store
                .read()
                .get("places")
                .unwrap()
                .get("favourites")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn append_preserves_duplicates_invalid_records_and_unrelated_state() {
        let state = uistate::from_file(
            r#"{"hidden":true,"places":{"favourites":[{"label":"A","path":"/a"},17,{"label":"","path":"bad"}]}}"#,
        );
        let next = changed(
            &state,
            &parse(r#"{"op":"add","record":{"label":"A","path":"/a"}}"#),
        )
        .unwrap();
        let rows = next
            .get("places")
            .unwrap()
            .get("favourites")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], rows[3]);
        assert_eq!(rows[1], Json::Num("17".into()));
        assert_eq!(next.get("hidden"), Some(&Json::Bool(true)));
    }

    #[test]
    fn stale_edit_is_refused_and_invalid_new_records_do_not_land() {
        let state = uistate::from_file(r#"{"places":{"favourites":[{"label":"A","path":"/a"}]}}"#);
        assert!(changed(&state, &parse(r#"{"op":"remove","index":0,"expected":[]}"#)).is_err());
        for operation in [
            r#"{"op":"add","record":{"label":" ","path":"/a"}}"#,
            r#"{"op":"add","record":{"label":"A","path":"relative"}}"#,
            r#"{"op":"add","record":{"label":"A","path":"javascript:alert(1)"}}"#,
        ] {
            assert!(changed(&state, &parse(operation)).is_err());
        }
    }
}
