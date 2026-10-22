use super::*;

// The asked paths as the request carries them, answered against an empty mount table.
fn asked(paths: &[&str]) -> String {
    answer(&paths.iter().map(|path| path.to_string()).collect::<Vec<_>>(), "", 0)
}

fn said(pairs: &[(&str, &str)]) -> String {
    let body: Vec<String> = pairs.iter().map(|(path, kind)| format!(r#""{}":"{}""#, path, kind)).collect();
    format!(r#"{{"t":"kind","kinds":{{{}}}}}"#, body.join(","))
}

#[test]
fn a_directory_reads_dir_and_a_file_reads_file() {
    let sandbox = crate::backend::testdir::TestDir::new("kindplain");
    let dir = sandbox.dir("folder");
    let file = sandbox.file("item", "original");
    assert_eq!(asked(&[dir.to_str().unwrap(), file.to_str().unwrap()]), said(&[(dir.to_str().unwrap(), "dir"), (file.to_str().unwrap(), "file")]), "the folder and the file answer their own kinds");
}

#[test]
fn a_symlink_answers_the_kind_of_the_directory_it_names() {
    let sandbox = crate::backend::testdir::TestDir::new("kindlink");
    let link = sandbox.path().join("link");
    std::os::unix::fs::symlink("/proc", &link).unwrap();
    assert_eq!(asked(&[link.to_str().unwrap()]), said(&[(link.to_str().unwrap(), "dir")]), "the link answers its target's kind, the way devs answers its device");
}

#[test]
fn a_missing_or_relative_path_answers_empty_and_repeats_are_asked_once() {
    let sandbox = crate::backend::testdir::TestDir::new("kindgone");
    let dir = sandbox.dir("folder");
    let asked_twice = answer(&[dir.to_str().unwrap().to_string(), dir.to_str().unwrap().to_string(), "relative".to_string(), (sandbox.path().join("gone").to_str().unwrap().to_string())], "", 7);
    assert_eq!(asked_twice, format!(r#"{{"t":"kind","kinds":{{"{}":"dir","relative":"","{}":""}}, "id":7}}"#, dir.to_str().unwrap(), sandbox.path().join("gone").to_str().unwrap()), "each path once, a relative path empty, a missing path empty, and the id rides along");
}
