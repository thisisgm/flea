use crate::backend::{
    child, sandbox, testdir::TestDir, thumbargv, thumbbind, thumbs, thumbspec::Spec,
};
use std::path::PathBuf;

// FLEA_TEST_F3D_PREFIX permits an unpacked private /usr for the explicit native proof.
#[test]
#[ignore = "requires f3d 3.5, Mesa llvmpipe and a working bwrap namespace"]
fn real_f3d_thumbnail_exec_path_renders_a_mesh_under_llvmpipe() {
    let prefix = std::env::var_os("FLEA_TEST_F3D_PREFIX").map(PathBuf::from);
    let program = prefix
        .as_ref()
        .map(|p| p.join("bin/f3d"))
        .unwrap_or_else(|| PathBuf::from("/usr/bin/f3d"));
    assert!(
        program.is_file(),
        "f3d must be present for this explicit proof"
    );
    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "this proof requires the jail"
    );
    let dir = TestDir::new("real-f3d");
    let input = dir.file("mesh.obj", &sphere());
    let output = dir.file("thumbnail.png", "");
    let spec = Spec {
        exec: vec![
            program.to_string_lossy().into_owned(),
            "--config=thumbnail".to_string(),
            "--load-plugins=assimp".to_string(),
            "--verbose=quiet".to_string(),
            "--output=%o".to_string(),
            "--resolution=%s,%s".to_string(),
            "%i".to_string(),
        ],
    };
    let (abs, inner) = thumbargv::argv(&spec, &input, &output, thumbs::THUMB_SIZE).unwrap();
    let mut full = thumbbind::wrap(&inner, &abs, &output).unwrap();
    let mut extra = vec![
        "--setenv".to_string(),
        "GALLIUM_DRIVER".to_string(),
        "llvmpipe".to_string(),
        "--setenv".to_string(),
        "LP_NUM_THREADS".to_string(),
        "32".to_string(),
    ];
    if let Some(prefix) = prefix {
        let prefix = prefix.canonicalize().unwrap();
        let path = prefix.to_string_lossy().into_owned();
        extra.extend([
            "--ro-bind".to_string(),
            path.clone(),
            path,
            "--setenv".to_string(),
            "LD_LIBRARY_PATH".to_string(),
            prefix.join("lib").to_string_lossy().into_owned(),
            "--setenv".to_string(),
            "F3D_PLUGINS_PATH".to_string(),
            prefix
                .join("lib/f3d/plugins")
                .to_string_lossy()
                .into_owned(),
        ]);
    }
    // Keep the production argv, status descriptor, timeout and limits beneath the proof-only library binds.
    full.splice(full.len() - inner.len()..full.len() - inner.len(), extra);
    assert!(full.iter().any(|a| a == "--as=3221225472"));
    assert!(full.iter().any(|a| a == "--cpu=30"));
    assert!(matches!(
        child::run_with_timeout(&full, thumbs::JOB_TIMEOUT),
        child::Ran::Succeeded
    ));
    let png = std::fs::read(&output).unwrap();
    assert!(png.len() > 100);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 256);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 256);
    assert!(full.iter().any(|a| a == sandbox::BWRAP));
    eprintln!("real f3d wrote {} bytes, PNG 256x256", png.len());
}

fn sphere() -> String {
    use std::fmt::Write;
    let mut mesh = String::new();
    let n = 220;
    for i in 0..n {
        let phi = std::f64::consts::PI * (i as f64 + 0.5) / n as f64;
        for j in 0..n {
            let theta = 2.0 * std::f64::consts::PI * j as f64 / n as f64;
            writeln!(
                mesh,
                "v {} {} {}",
                phi.sin() * theta.cos(),
                phi.cos(),
                phi.sin() * theta.sin()
            )
            .unwrap();
        }
    }
    for i in 0..n - 1 {
        for j in 0..n {
            let a = i * n + j + 1;
            let b = i * n + (j + 1) % n + 1;
            let (c, d) = (a + n, b + n);
            writeln!(mesh, "f {a} {c} {d}\nf {a} {d} {b}").unwrap();
        }
    }
    mesh
}
