use super::*;

#[test]
fn pb_rest_inline_sizes_answer_their_absolute_rows_only() {
    let mut sizes = DirSizes::default();
    let rows = vec![
        RowFacts::default(),
        RowFacts {
            inline: Some(96),
            ..Default::default()
        },
    ];
    sizes.inlined(&rows, 10, 240);
    assert_eq!(sizes.size_for(10), None);
    assert_eq!(
        sizes.size_for(11),
        Some(Size {
            bytes: 96,
            partial: false
        })
    );
}
