use super::*;

fn settings(layout: Layout, spacing: Spacing) -> Settings {
    Settings { layout, spacing, background: Background::White, columns: 2 }
}

#[test]
fn spacing_is_zero_three_or_eight_percent_of_the_shared_edge() {
    assert_eq!(Spacing::None.pixels(1000), 0);
    assert_eq!(Spacing::Small.pixels(1000), 30);
    assert_eq!(Spacing::Large.pixels(1000), 80);
    assert_eq!(Spacing::Small.pixels(150), 4, "board source rounds half to even");
    assert_eq!(Spacing::Small.pixels(50), 2);
    assert_eq!(settings(Layout::Row, Spacing::None).edge((800, 600)), 600);
    assert_eq!(settings(Layout::Column, Spacing::None).edge((800, 600)), 800);
    assert!(!background_enabled(Layout::Grid, Spacing::None, 4, 2));
    assert!(background_enabled(Layout::Grid, Spacing::None, 3, 2));
    assert!(background_enabled(Layout::Row, Spacing::Small, 3, 2));
}

#[test]
fn row_and_column_match_the_shared_edge_before_adding_gutters() {
    let input = Path::new("/work/in.jpg");
    let out = Path::new("/work/scaled.png");
    let row = scale_argv(input, out, (800, 600), &settings(Layout::Row, Spacing::Small), false);
    assert_eq!(row, ["magick", "--", "/work/in.jpg[0]", "-auto-orient", "-resize", "x600", "-background", "white", "-gravity", "West", "-splice", "18x0", "/work/scaled.png"]);
    let column = scale_argv(input, out, (800, 600), &settings(Layout::Column, Spacing::Large), false);
    assert!(column.windows(2).any(|p| p == ["-resize", "800"]));
    assert!(column.windows(2).any(|p| p == ["-splice", "0x64"]));
    assert!(!scale_argv(input, out, (800, 600), &settings(Layout::Row, Spacing::Small), true).contains(&"-splice".into()));
    assert_eq!(shared_size(Layout::Row, &[(800, 600), (400, 200)]).unwrap(), (800, 200));
    assert_eq!(shared_size(Layout::Column, &[(800, 600), (400, 200)]).unwrap(), (400, 600));
    assert_eq!(shared_size(Layout::Grid, &[(800, 200), (400, 600)]).unwrap(), (800, 600));
}

#[test]
fn joining_uses_append_or_montage_in_the_supplied_order() {
    let inputs = [PathBuf::from("/work/b.png"), PathBuf::from("/work/a.png")];
    let out = Path::new("/work/result.png");
    let row = join_argv(&inputs, out, (800, 600), &settings(Layout::Row, Spacing::None));
    assert!(row.contains(&"+append".into()));
    let column = join_argv(&inputs, out, (800, 600), &settings(Layout::Column, Spacing::None));
    assert!(column.contains(&"-append".into()));
    let row = join_argv(&inputs, out, (800, 600), &settings(Layout::Row, Spacing::Small));
    assert!(row.windows(2).any(|pair| pair == ["-border", "18"]));
    assert_eq!(join_argv(&inputs, out, (800, 600), &settings(Layout::Grid, Spacing::Small)),
        ["montage", "--", "/work/b.png", "--", "/work/a.png", "-background", "white", "-tile", "2x", "-geometry", "+0+0", "/work/result.png"]);
    let commands = join_commands(&inputs, out, (800, 600), &settings(Layout::Grid, Spacing::Small));
    assert_eq!(commands.len(), 2);
    assert!(commands[1].windows(2).any(|pair| pair == ["-splice", "18x18"]));
}

#[test]
fn defaults_follow_the_first_format_and_jpeg_refuses_transparency() {
    let ask = Ask::parse(r#"{"paths":["/photos/first.jpg","/photos/next.png"]}"#);
    assert_eq!(ask.format, "jpg");
    assert_eq!(ask.background, "White");
    assert!(Settings::parse(&ask.layout, &ask.spacing, &ask.background, &ask.format, ask.columns).is_ok());
    let bad = Ask::parse(r#"{"format":"jpg","background":"Transparent"}"#);
    assert!(Settings::parse(&bad.layout, &bad.spacing, &bad.background, &bad.format, bad.columns).is_err());
    let bad = Ask::parse(r#"{"layout":"Grid","columns":0}"#);
    assert!(Settings::parse(&bad.layout, &bad.spacing, &bad.background, &bad.format, bad.columns).is_err());
}

#[test]
fn pdf_is_a_combine_format_that_keeps_transparency() {
    let ask = Ask::parse(r#"{"paths":["/photos/first.png","/photos/next.png"],"format":"pdf"}"#);
    assert_eq!(ask.background, "Transparent");
    assert!(Settings::parse(&ask.layout, &ask.spacing, &ask.background, &ask.format, ask.columns).is_ok());
    assert_eq!(printconvert::combined_name(&ask.paths[0], &ask.format), "first (combined).pdf");
    let row = join_argv(&[PathBuf::from("/work/a.png"), PathBuf::from("/work/b.png")], Path::new("/work/output.pdf"), (800, 600), &settings(Layout::Row, Spacing::None));
    assert_eq!(row[row.len() - 3..], ["+repage", "+gravity", "/work/output.pdf"], "the page is the whole composite, not the first image's canvas or a Letter sheet");
    let grid = join_commands(&[PathBuf::from("/work/a.png"), PathBuf::from("/work/b.png")], Path::new("/work/output.pdf"), (800, 600), &settings(Layout::Grid, Spacing::Small));
    assert_eq!(grid[1][grid[1].len() - 3..], ["+repage", "+gravity", "/work/output.pdf"]);
    assert!(!row.contains(&"remove".into()), "only JPEG flattens its alpha");
}
