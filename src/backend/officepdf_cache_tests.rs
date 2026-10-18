use super::*;

#[test]
fn review_a_failure_from_the_old_office_profile_is_retried() {
    let d = TestDir::new("officepdf-profile-upgrade");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let key = Store::key(&src);
    let marker = r.svc.store.fail_path(&key);
    d.assert_contains(&marker);
    // The old profile wrote the raw diagnostic without a version or a profile header.
    Store::publish(
        &marker,
        mtime_of(&src),
        &mut "corrupted double-linked list".as_bytes(),
    )
    .unwrap();
    assert_eq!(
        r.ask(1, &src),
        converting_line(1),
        "the corrected Office profile must retry an unchanged document that failed under the old cap"
    );
    let pdf = r.svc.store.pdf_path(&key);
    assert_eq!(r.end().1, ready_line(1, &pdf));
    assert_eq!(std::fs::read(&pdf).unwrap(), b"%PDF-1.4 stub");
    assert_eq!(r.ask(2, &src), ready_line(2, &pdf));
    assert_eq!(r.jobs(), 1);
}

#[test]
fn a_failure_from_another_profile_is_retried() {
    let d = TestDir::new("officepdf-other-profile");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let key = Store::key(&src);
    let marker = r.svc.store.fail_path(&key);
    d.assert_contains(&marker);
    Store::publish(
        &marker,
        mtime_of(&src),
        &mut "flea-office-profile-v2\nold failure".as_bytes(),
    )
    .unwrap();
    assert_eq!(r.ask(1, &src), converting_line(1));
    let pdf = r.svc.store.pdf_path(&key);
    assert_eq!(r.end().1, ready_line(1, &pdf));
    assert_eq!(r.jobs(), 1);
}

#[test]
fn stale_failure_profiles_keep_valid_cached_pdfs() {
    for message in ["legacy failure", "flea-office-profile-v1\nold failure", "flea-office-profile-v2\nold failure"] {
        let d = TestDir::new("officepdf-profile-pdf");
        let r = rig(&d, BROKEN, CONVERT_TIMEOUT);
        let src = d.file("a.docx", "doc");
        let key = Store::key(&src);
        let marker = r.svc.store.fail_path(&key);
        let pdf = r.svc.store.pdf_path(&key);
        d.assert_contains(&marker);
        d.assert_contains(&pdf);
        Store::publish(&marker, mtime_of(&src), &mut message.as_bytes()).unwrap();
        Store::publish(&pdf, mtime_of(&src), &mut "%PDF-1.4 cached".as_bytes()).unwrap();
        assert_eq!(r.ask(1, &src), ready_line(1, &pdf));
        assert_eq!(
            r.jobs(),
            0,
            "a cached PDF survives failure-profile upgrades"
        );
        assert_eq!(std::fs::read(&pdf).unwrap(), b"%PDF-1.4 cached");
        assert_eq!(mtime_of(&pdf), mtime_of(&src));
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            message,
            "legacy markers are ignored without deleting user cache entries"
        );
    }
}
