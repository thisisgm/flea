use super::on_phone;

#[test]
fn a_phone_path_is_its_scheme_and_host_under_any_session_gvfs() {
    let pixel = "mtp://pixel/";
    assert!(on_phone("/run/user/1000/gvfs/mtp:host=pixel", pixel));
    assert!(on_phone(
        "/run/user/1000/gvfs/mtp:host=pixel/DCIM/a.jpg",
        pixel
    ));
    assert!(on_phone(
        "/run/user/1001/gvfs/mtp:host=pixel,port=1/a",
        pixel
    ));
    assert!(!on_phone("/run/user/1000/gvfs/mtp:host=pixel2/a", pixel));
    assert!(!on_phone("/run/user/1000/gvfs/gphoto2:host=pixel/a", pixel));
    assert!(!on_phone("/home/u/gvfs/mtp:host=pixel/a", pixel));
    assert!(!on_phone("/run/user/1000/gvfs/mtp:host=pixel/a", "mtp:///"));
    let camera = "gphoto2://usb%3A001%2C014/";
    assert!(on_phone(
        "/run/user/1000/gvfs/gphoto2:host=usb%3A001%2C014/store",
        camera
    ));
    assert!(on_phone(
        "/run/user/1000/gvfs/afc:host=abc,port=3/Documents",
        "afc://abc:3/"
    ));
}
