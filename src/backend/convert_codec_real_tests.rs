use super::*;

fn has_encoder(formats: &str, codec: &str) -> bool {
    formats.lines().any(|line| {
        let columns: Vec<_> = line.split_whitespace().collect();
        columns.len() >= 3
            && columns[0].trim_end_matches('*') == codec
            && columns[2].as_bytes().get(1) == Some(&b'w')
    })
}

#[test]
fn convert_codec_real_outputs_match_the_requested_encoder_or_refuse() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    assert!(
        ready(&["magick", "bwrap", "prlimit", "pdfinfo"]),
        "codec proof needs real tools"
    );
    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "codec proof needs the production jail"
    );
    let mut rig = Rig::new(false);
    let source = rig.dir.join("photo.png");
    tool(
        &rig.dir,
        vec![
            "magick".into(),
            "-size".into(),
            "12x8".into(),
            "xc:red".into(),
            "-set".into(),
            "comment".into(),
            "retained-metadata".into(),
            source.display().to_string(),
        ],
    );
    let original = std::fs::read(&source).unwrap();
    let formats = tool(
        &rig.dir,
        vec!["magick".into(), "-list".into(), "format".into()],
    );
    let version = tool(&rig.dir, vec!["magick".into(), "-version".into()]);
    println!("{version}");
    for (format, codec) in [
        ("jpg", "JPEG"),
        ("png", "PNG"),
        ("webp", "WEBP"),
        ("avif", "AVIF"),
        ("heic", "HEIC"),
        ("tiff", "TIFF"),
        ("bmp", "BMP"),
        ("pdf", "PDF"),
    ] {
        let output = rig.dir.join(&printconvert::converted_name(&source, format));
        let command = format!(
            r#"{{"c":"convertbatch","requestId":81,"path":"{}","format":"{format}","strip":true}}"#,
            source.display()
        );
        let replies = rig.ask(&command, "convertdone");
        let done = replies.last().unwrap();
        assert_eq!(field_usize(done, "requestId"), Some(81));
        if !has_encoder(&formats, codec) {
            assert!(
                !field_bool(done, "ok"),
                "an unavailable {codec} encoder must refuse: {done}"
            );
            assert!(
                !output.exists(),
                "an unavailable encoder must publish no fallback bytes"
            );
            assert!(crate::json::field_str_array(done, "paths").is_empty());
            let error = field_str(done, "err").unwrap();
            assert!(
                error.contains("no encode delegate for this image format"),
                "{error}"
            );
            assert!(
                error.to_ascii_uppercase().contains(codec),
                "codec-specific refusal: {error}"
            );
            println!("{format}: truthful unavailable-encoder refusal: {error}");
            continue;
        }
        assert!(
            field_bool(done, "ok"),
            "available {codec} encoder failed: {done}"
        );
        let bytes = std::fs::read(&output).unwrap();
        match format {
            "jpg" => assert!(bytes.starts_with(&[0xff, 0xd8, 0xff])),
            "png" => assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n")),
            "webp" => {
                assert!(bytes.starts_with(b"RIFF"));
                assert_eq!(&bytes[8..12], b"WEBP");
            }
            "avif" | "heic" => {
                assert_eq!(&bytes[4..8], b"ftyp");
                let brands = &bytes[8..u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize];
                let expected: &[[u8; 4]] = if format == "avif" {
                    &[*b"avif", *b"avis"]
                } else {
                    &[
                        *b"heic", *b"heix", *b"heim", *b"heis", *b"hevc", *b"hevx", *b"hevm",
                        *b"hevs",
                    ]
                };
                assert!(brands
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|brand| expected.iter().any(|v| brand == v)));
            }
            "tiff" => assert!(bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")),
            "bmp" => assert!(bytes.starts_with(b"BM")),
            "pdf" => assert!(bytes.starts_with(b"%PDF-")),
            _ => unreachable!(),
        }
        if format == "pdf" {
            let info = tool(
                &rig.dir,
                vec!["pdfinfo".into(), "--".into(), output.display().to_string()],
            );
            assert!(info
                .lines()
                .any(|line| line.strip_prefix("Pages:").is_some_and(|n| n.trim() == "1")));
            println!(
                "pdf: one page; {} bytes; magic {:?}",
                bytes.len(),
                &bytes[..bytes.len().min(32)]
            );
            continue;
        }
        let facts = tool(
            &rig.dir,
            vec![
                "magick".into(),
                "identify".into(),
                "-format".into(),
                "%m %w %h %[comment]".into(),
                output.display().to_string(),
            ],
        );
        if format != "pdf" {
            assert!(
                facts.starts_with(&format!("{codec} 12 8")),
                "{format}: {facts}"
            );
            assert!(
                !facts.contains("retained-metadata"),
                "{format}: metadata was not stripped"
            );
        }
        println!(
            "{format}: {facts}; {} bytes; magic {:?}",
            bytes.len(),
            &bytes[..bytes.len().min(32)]
        );
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn convert_codec_real_metadata_remains_opt_in_to_stripping() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    assert!(
        ready(&["magick", "bwrap", "prlimit"]),
        "metadata proof needs real tools"
    );
    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "metadata proof needs the production jail"
    );
    let mut rig = Rig::new(false);
    for (name, strip) in [("retained", false), ("stripped", true)] {
        let source = rig.dir.join(&format!("{name}.png"));
        tool(
            &rig.dir,
            vec![
                "magick".into(),
                "-size".into(),
                "12x8".into(),
                "xc:red".into(),
                "-set".into(),
                "comment".into(),
                "retained-metadata".into(),
                source.display().to_string(),
            ],
        );
        let original = std::fs::read(&source).unwrap();
        let replies = rig.ask(&format!(r#"{{"c":"convertbatch","requestId":82,"path":"{}","format":"jpg","strip":{strip}}}"#, source.display()), "convertdone");
        assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
        let output = rig.dir.join(&format!("{name} (converted).jpg"));
        let facts = tool(
            &rig.dir,
            vec![
                "magick".into(),
                "identify".into(),
                "-format".into(),
                "%m %w %h %[comment]".into(),
                output.display().to_string(),
            ],
        );
        assert!(std::fs::read(&output)
            .unwrap()
            .starts_with(&[0xff, 0xd8, 0xff]));
        assert!(facts.starts_with("JPEG 12 8"), "{facts}");
        assert_eq!(facts.contains("retained-metadata"), !strip, "{facts}");
        assert_eq!(std::fs::read(source).unwrap(), original);
        println!("strip={strip}: {facts}");
    }
}
