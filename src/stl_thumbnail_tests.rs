use super::*;
use crate::backend::testdir::TestDir;
use std::io::Cursor;
use std::path::Path;

// A synthetic tetrahedron, unrelated to any user model.
const POINTS: [[f32; 3]; 4] = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
const FACES: [[usize; 3]; 4] = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];

fn args(input: &Path, output: &Path, size: u32) -> impl Iterator<Item = OsString> {
    [
        input.as_os_str().to_owned(),
        output.as_os_str().to_owned(),
        size.to_string().into(),
    ]
    .into_iter()
}

fn ascii() -> String {
    let mut text = String::from("solid tetrahedron\n");
    for face in FACES {
        text.push_str("facet normal 0 0 0\nouter loop\n");
        for index in face {
            let [x, y, z] = POINTS[index];
            text.push_str(&format!("vertex {x} {y} {z}\n"));
        }
        text.push_str("endloop\nendfacet\n");
    }
    text.push_str("endsolid tetrahedron\n");
    text
}

fn binary() -> Vec<u8> {
    let mut bytes = vec![0; 80];
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    for face in FACES {
        bytes.extend_from_slice(&[0; 12]);
        for index in face {
            for value in POINTS[index] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0; 2]);
    }
    bytes
}

#[test]
fn ascii_and_binary_stl_render_visible_bounded_pngs() {
    let dir = TestDir::new("stl-render");
    for (name, bytes, size) in [
        ("ascii.stl", ascii().into_bytes(), 96),
        ("binary.stl", binary(), 256),
    ] {
        let input = dir.join(name);
        std::fs::write(&input, bytes).unwrap();
        let output = dir.file("thumbnail.png", "previous output must be replaced");
        run(args(&input, &output, size)).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(std::fs::read(output).unwrap()))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (size, size));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let pixels = &pixels[..info.buffer_size()];
        let visible = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0 && p[..3].iter().any(|c| *c > 0))
            .count();
        assert!(
            visible > (size * size / 20) as usize,
            "the model must be visible"
        );
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 0),
            "the background stays transparent"
        );
    }
}

#[test]
fn malformed_and_oversized_stl_do_not_replace_output() {
    let dir = TestDir::new("stl-refusal");
    let output = dir.file("thumbnail.png", "original");
    let malformed = dir.file(
        "broken.stl",
        "solid broken\nfacet normal 0 0 1\nouter loop\nvertex 0 0",
    );
    let oversized = dir.file("oversized.stl", "");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&oversized)
        .unwrap()
        .set_len(model3d::MAX_INPUT as u64 + 1)
        .unwrap();
    for input in [malformed, oversized] {
        assert!(run(args(&input, &output, 256)).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"original");
    }
}
