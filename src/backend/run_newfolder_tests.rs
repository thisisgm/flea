use super::*;

fn mkdir(h: &mut Loop, name: Option<&str>, base: Option<&str>) -> String {
    let parent = h.dir.path().to_path_buf();
    mkdir_at(h, &parent, name, base)
}

fn mkdir_at(h: &mut Loop, parent: &Path, name: Option<&str>, base: Option<&str>) -> String {
    let mut fields = vec![
        ("c".into(), crate::jsondoc::Json::Str("mkdir".into())),
        (
            "path".into(),
            crate::jsondoc::Json::Str(parent.to_string_lossy().into_owned()),
        ),
    ];
    for (key, value) in [("name", name), ("defaultBase", base)] {
        if let Some(value) = value {
            fields.push((key.into(), crate::jsondoc::Json::Str(value.into())));
        }
    }
    h.request(&crate::jsondoc::render(&crate::jsondoc::Json::Obj(fields)));
    h.said()
}

#[test]
fn newfolder_localized_bases_collide_with_every_entry_kind_and_undo() {
    for base in [
        "Nouveau dossier",
        "新建文件夹",
        "مجلد جديد",
        "नया फ़ोल्डर",
        "Новая папка",
        "say \"hi\"",
    ] {
        let mut h = Loop::new("newfolder-localized");
        h.dir.dir(base);
        h.dir.file(&format!("{base} 2"), "keep");
        std::os::unix::fs::symlink("missing", h.dir.join(&format!("{base} 3"))).unwrap();
        let answer = mkdir(&mut h, None, Some(base));
        let made = h.dir.join(&format!("{base} 4"));
        assert_eq!(
            field_str(&answer, "path").as_deref(),
            made.to_str(),
            "{answer}"
        );
        assert!(made.is_dir());
        h.request(r#"{"c":"undo"}"#);
        assert!(h.said().contains(r#""ok":true"#));
        assert!(!made.exists());
        assert_eq!(
            std::fs::read_to_string(h.dir.join(&format!("{base} 2"))).unwrap(),
            "keep"
        );
        assert!(h
            .dir
            .join(&format!("{base} 3"))
            .symlink_metadata()
            .unwrap()
            .is_symlink());
    }
}

#[test]
fn newfolder_legacy_forms_and_exact_names_keep_their_contract() {
    let mut h = Loop::new("newfolder-legacy");
    for (name, expected) in [(None, "New Folder"), (Some(""), "New Folder 2")] {
        let answer = mkdir(&mut h, name, None);
        assert_eq!(
            field_str(&answer, "path").as_deref(),
            h.dir.join(expected).to_str()
        );
    }
    assert!(mkdir(&mut h, Some("exact"), Some("../ignored")).contains(r#""ok":true"#));
    assert!(mkdir(&mut h, Some("exact"), Some("新建文件夹")).contains("already exists"));
    assert!(!h.dir.join("exact 2").exists());
}

#[test]
fn newfolder_untrusted_bases_cannot_escape_or_create_invalid_names() {
    let mut h = Loop::new("newfolder-invalid");
    let parent = h.dir.dir("parent");
    let absolute = h.dir.join("absolute-escape").to_string_lossy().into_owned();
    let before = std::fs::read_dir(h.dir.path()).unwrap().count();
    for base in [
        "",
        ".",
        "..",
        "../escape",
        "sub/child",
        &absolute,
        "nul\0byte",
    ] {
        let answer = mkdir_at(&mut h, &parent, None, Some(base));
        assert_eq!(
            field_str(&answer, "t").as_deref(),
            Some("error"),
            "{base:?}: {answer}"
        );
        assert_eq!(std::fs::read_dir(h.dir.path()).unwrap().count(), before);
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 0);
    }
}

#[test]
fn newfolder_default_attempts_remain_bounded() {
    let mut h = Loop::new("newfolder-bounded");
    h.dir.file("新建文件夹", "keep");
    for n in 2..=999 {
        h.dir.file(&format!("新建文件夹 {n}"), "keep");
    }
    assert!(mkdir(&mut h, None, Some("新建文件夹")).contains("every default folder name"));
    assert!(!h.dir.join("新建文件夹 1000").exists());
}

#[test]
fn newfolder_filesystem_rules_apply_before_automatic_name_selection() {
    for fs in ["vfat", "exfat", "ntfs3"] {
        let mut h = Loop::new("newfolder-drive-rules");
        let body = format!(
            "1 0 8:1 / {} rw - {fs} /dev/test rw,windows_names\n",
            h.dir.path().display()
        );
        let _mount = crate::backend::iomount::test_hold_body(body);
        for base in ["CON", "a:b", "trail.", "trail ", "a\\b"] {
            let answer = mkdir(&mut h, None, Some(base));
            assert_eq!(
                field_str(&answer, "t").as_deref(),
                Some("error"),
                "{fs}: {base}: {answer}"
            );
            assert!(!h.dir.join(base).exists());
        }
    }
}

#[test]
fn newfolder_undo_refuses_a_replacement_identity() {
    let mut h = Loop::new("newfolder-undo-identity");
    assert!(mkdir(&mut h, None, Some("新建文件夹")).contains(r#""ok":true"#));
    let made = h.dir.join("新建文件夹");
    h.dir.assert_contains(&made);
    let held = std::fs::File::open(&made).unwrap();
    assert!(made.is_absolute() && !made.as_os_str().is_empty());
    std::fs::remove_dir(&made).unwrap();
    h.dir.dir("新建文件夹");
    h.request(r#"{"c":"undo"}"#);
    assert!(h.said().contains(r#""t":"error"#));
    assert!(made.is_dir());
    drop(held);
}
