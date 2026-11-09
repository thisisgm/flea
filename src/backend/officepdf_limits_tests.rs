use super::*;

// Independent child limits catch both the installed runner and its address-space argument changing.
const LIMITED_PDF: &str = r#"exec /usr/bin/python3 -c '
import resource, sys
address_space_bytes = 8589934592
unlimited = (resource.RLIM_INFINITY, resource.RLIM_INFINITY)
assert resource.getrlimit(resource.RLIMIT_AS) == (address_space_bytes, address_space_bytes), "Office must have a finite 8 GiB address-space cap"
assert resource.getrlimit(resource.RLIMIT_CPU) == unlimited, "Office has no CPU cap: the wall clock bounds it"
with open(sys.argv[1], "xb") as output:
    output.write(b"%PDF-1.4 stub")
' "$2/out.pdf""#;

#[test]
fn the_production_runner_converts_through_the_real_jail() {
    if skipped() {
        return;
    }
    let d = TestDir::new("officepdf-jail");
    let service = Arc::clone(production());
    let jail: Runner =
        Box::new(move |inner, input, work, stop| (service.runner)(inner, input, work, stop));
    let r = rig_in(&d, LIMITED_PDF, CONVERT_TIMEOUT, jail);
    let src = d.file("a.docx", "doc");
    assert_eq!(r.ask(1, &src), converting_line(1));
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    assert_eq!(r.end().1, ready_line(1, &pdf));
    assert_eq!(std::fs::read(&pdf).unwrap(), b"%PDF-1.4 stub");
}
