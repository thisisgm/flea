use super::*;
use crate::backend::testdir::TestDir;
use crate::jsondoc::Json;
use crate::tui::{actions, input::Key, keymap::Map, model::Model, wire};
use std::os::unix::{ffi::OsStringExt, fs::{symlink, PermissionsExt}};

#[test]
fn cwd_options_refuse_missing_invalid_duplicate_and_non_tui_arguments() {
    for flag in ["--cwd-file", "--chooser-file", "--tui-keymap"] {
        for tail in [None, Some(""), Some("--gui")] {
            let mut args = vec![flag.into()];
            if let Some(tail) = tail { args.push(tail.into()); }
            assert!(Options::default().take(&args, &mut 0).is_err());
        }
    }
    for name in ["default", "vim", "mac", "windows", "yazi"] {
        let mut options = Options::default();
        assert!(options.take(&["--tui-keymap".into(), name.into()], &mut 0).unwrap());
        assert_eq!(options.keymap.as_deref(), Some(name));
        assert!(options.validate(true, false).is_ok());
        assert!(options.validate(false, false).is_err());
        assert!(options.validate(true, true).is_err());
    }
    assert!(Options::default().take(&["--tui-keymap".into(), "unknown".into()], &mut 0).is_err());
    let mut options = Options::default();
    let args = ["--cwd-file".into(), "result".into()];
    options.take(&args, &mut 0).unwrap();
    assert!(options.cwd_file.as_ref().unwrap().is_absolute());
    assert!(options.take(&args, &mut 0).is_err());
    options.chooser_file = options.cwd_file.clone();
    assert!(options.validate(true, false).is_err());
}

#[test]
fn cwd_and_chooser_outputs_are_private_atomic_lines_and_never_follow_output_symlinks() {
    let dir = TestDir::new("cwd-output");
    let victim = dir.file("victim", "preserve");
    let output = dir.join("result");
    symlink(&victim, &output).unwrap();
    write_paths(&output, [dir.path()]).unwrap();
    assert_eq!(fs::read_to_string(&victim).unwrap(), "preserve");
    assert!(!output.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(output.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    let old = fs::File::open(&output).unwrap();
    let selected = [dir.join("one"), dir.join("two")];
    write_paths(&output, selected.iter().map(PathBuf::as_path)).unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(), format!("{}\n{}\n", selected[0].display(), selected[1].display()));
    let mut old_body = String::new();
    std::io::Read::read_to_string(&mut &old, &mut old_body).unwrap();
    assert_eq!(old_body, format!("{}\n", dir.path().display()));
    let bad = dir.path().join(OsString::from_vec(vec![0xff]));
    for path in [bad, dir.join("new\nline"), PathBuf::from("relative")] {
        assert!(write_paths(&output, [path.as_path()]).is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), format!("{}\n{}\n", selected[0].display(), selected[1].display()));
    }
    assert!(write_paths(&dir.join("missing/out"), [dir.path()]).is_err());
    let folder = dir.dir("folder");
    assert!(write_paths(&folder, [dir.path()]).is_err());
    assert!(!fs::read_dir(dir.path()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn cwd_cancellation_signals_and_no_cwd_quit_preserve_existing_outputs() {
    let dir = TestDir::new("cwd-exit");
    let output = dir.file("cwd", "old");
    let chooser = dir.file("chosen", "old selection");
    let options = Options { cwd_file: Some(output.clone()), chooser_file: Some(chooser.clone()), keymap: None };
    assert_eq!(options.finish(Some(dir.path()), None, true, false).unwrap(), 1);
    assert_eq!(options.finish(Some(dir.path()), Some(&["/one".into()]), false, false).unwrap(), 130);
    assert_eq!(fs::read_to_string(&output).unwrap(), "old");
    assert_eq!(fs::read_to_string(&chooser).unwrap(), "old selection");
    let normal = Options { chooser_file: None, ..options };
    assert!(normal.finish(None, None, true, false).is_err());
    assert_eq!(normal.finish(Some(dir.path()), None, true, true).unwrap(), 0);
    assert_eq!(fs::read_to_string(&output).unwrap(), "old");
    assert_eq!(normal.finish(Some(dir.path()), None, true, false).unwrap(), 0);
    assert_eq!(fs::read_to_string(&output).unwrap(), format!("{}\n", dir.path().display()));
}

#[test]
fn cwd_tracks_successful_listing_and_keeps_it_through_failure_search_and_last_tab_close() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    assert_eq!(model.last_directory, None);
    model.open(PathBuf::from("/listing"), &mut wire).unwrap();
    model.receive(crate::jsondoc::parse(r#"{"t":"listed","n":0}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(model.last_directory.as_deref(), Some(Path::new("/listing")));
    model.open(PathBuf::from("/missing"), &mut wire).unwrap();
    model.receive(crate::jsondoc::parse(r#"{"t":"error","where":"scan","path":"/missing","msg":"not found"}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(model.last_directory.as_deref(), Some(Path::new("/listing")));
    model.path = PathBuf::from("/search-scope");
    actions::key(&mut model, &Key::character('w', ""), &Map::load(), &mut wire).unwrap();
    assert!(model.quit);
    assert_eq!(model.last_directory.as_deref(), Some(Path::new("/listing")));
    wire::finish(wire, reader);
}

#[test]
fn cwd_chooser_confirms_backend_paths_and_refuses_a_short_or_multiline_reply() {
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
      for reply in [vec!["/listing/file"], vec![], vec!["/listing/new\nline"]] {
        let (mut wire, reader) = wire::echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.choosing = true;
        model.preset = preset.into();
        model.total = 1;
        model.rows.insert(0, crate::tui::model::Row::parse(&crate::jsondoc::parse(r#"{"n":"file"}"#).unwrap(), &[]));
        actions::key(&mut model, &Key::named("Return", ""), &Map::load(), &mut wire).unwrap();
        assert!(model.choose_pending);
        let value = Json::Obj(vec![("t".into(), wire::word("paths")), ("paths".into(), Json::Arr(reply.iter().map(|p| wire::word(p)).collect()))]);
        model.receive(value, &mut wire).unwrap();
        assert_eq!(model.quit, reply == ["/listing/file"]);
        assert_eq!(model.chosen.is_some(), reply == ["/listing/file"]);
        if !model.quit { assert!(!model.error.is_empty()); }
        wire::finish(wire, reader);
      }
    }
}

// An output in a folder that does not exist is refused at launch, naming the flag and the folder, not after the session.
#[test]
fn an_output_whose_folder_is_missing_is_refused_before_the_session() {
    let dir = TestDir::new("cwd-missing-folder");
    for flag in ["--cwd-file", "--chooser-file"] {
        let mut options = Options::default();
        let output = dir.join("missing/out");
        options.take(&[flag.into(), output.to_string_lossy().into_owned()], &mut 0).unwrap();
        let error = options.validate(true, false).unwrap_err();
        assert_eq!(error, format!("The folder for {flag} does not exist: {}", dir.join("missing").display()));
        let mut options = Options::default();
        options.take(&[flag.into(), dir.join("out").to_string_lossy().into_owned()], &mut 0).unwrap();
        assert!(options.validate(true, false).is_ok(), "{flag} in a folder that exists");
    }
}
