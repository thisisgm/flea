use crate::backend::{iomount, mime::Db, testdir::TestDir};
use std::sync::Arc;

#[test]
fn list_size_policy_keeps_hidden_partitions_and_legacy_folder_order() {
    let root = TestDir::new("tui-hidden-size");
    let path = root.dir("listing");
    for name in ["A", "Z", ".hidden"] { root.dir(&format!("listing/{name}")); }
    root.file("listing/A/large", &"x".repeat(8192));
    root.file("listing/small.txt", "1234");
    root.file("listing/large.txt", "123456789");
    root.file("listing/.small", "1");
    let list = |policy: bool, hidden_last: bool, desc: bool, hidden: bool| {
        let request = format!(r#"{{"c":"list","by":"size","foldersByName":{policy},"hiddenLast":{hidden_last},"desc":{desc},"foldersFirst":true}}"#);
        let (tx, _rx) = crate::backend::events::channel();
        iomount::list_dir(path.display().to_string(), hidden, 32, request, Arc::new(Db::from_str("")), -1, tx,
            "1 0 8:1 / / rw - ext4 /dev/test rw\n").unwrap()
    };
    for hidden_last in [false, true] {
        for desc in [false, true] {
            let done = list(true, hidden_last, desc, true);
            let names: Vec<_> = (0..done.listing.len()).map(|n| done.listing.name(n)).collect();
            let expected = match (hidden_last, desc) {
                (false, false) => vec![".hidden", "A", "Z", ".small", "small.txt", "large.txt"],
                (false, true) => vec![".hidden", "A", "Z", "large.txt", "small.txt", ".small"],
                (true, false) => vec!["A", "Z", "small.txt", "large.txt", ".hidden", ".small"],
                (true, true) => vec!["A", "Z", "large.txt", "small.txt", ".hidden", ".small"],
            };
            assert_eq!(names, expected, "hiddenLast={hidden_last} desc={desc}");
            assert!(done.sized.is_empty(), "the TUI policy never walks folder sizes");
        }
    }
    for desc in [false, true] {
        let done = list(false, false, desc, false);
        let names: Vec<_> = (0..done.listing.len()).map(|n| done.listing.name(n)).collect();
        assert_eq!(names, if desc { vec!["A", "Z", "large.txt", "small.txt"] }
            else { vec!["Z", "A", "small.txt", "large.txt"] }, "legacy list size order stays unchanged");
    }
}
