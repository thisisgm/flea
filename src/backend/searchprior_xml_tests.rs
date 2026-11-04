use super::*;

fn reject_after_bookmark(fragments: &[&str]) {
    let bookmark = "<bookmark href='file:///work/z/report.rs'/>";
    let mut accepted = Vec::new();
    for fragment in fragments {
        for xml in [
            format!("<xbel>{bookmark}{fragment}</xbel>"),
            format!("<xbel>{bookmark}<info>{fragment}</info></xbel>"),
            format!("<xbel>{bookmark}</xbel>{fragment}"),
        ] {
            if xbel(&xml).is_some() { accepted.push(xml); }
        }
    }
    assert!(accepted.is_empty(), "malformed XML retained earlier priors: {accepted:?}");
}

#[test]
fn searchprior_fix2_processing_instructions_reject_invalid_targets_and_characters() {
    reject_after_bookmark(&[
        "<??>", "<?1bad?>", "<?-bad?>", "<?.bad?>", "<?target!?>",
        "<?target \u{0}?>", "<?target \u{ffff}?>", "<?xml version='1.0'?>", "<?XML?>",
    ]);
}

#[test]
fn searchprior_fix2_cdata_rejects_invalid_xml_characters() {
    reject_after_bookmark(&[
        "<![CDATA[\u{0}]]>", "<![CDATA[\u{b}]]>", "<![CDATA[\u{fffe}]]>", "<![CDATA[\u{ffff}]]>",
    ]);
}

#[test]
fn searchprior_fix2_text_rejects_raw_cdata_terminators() {
    reject_after_bookmark(&["]]>", "before]]>after"]);
}

#[test]
fn searchprior_fix2_comments_reject_invalid_characters_and_endings() {
    reject_after_bookmark(&["<!--\u{0}-->", "<!--\u{ffff}-->", "<!--trailing--->"]);
}

#[test]
fn searchprior_fix2_declarations_whitespace_and_character_references_are_validated() {
    for prefix in [
        "<?xml?>", "<?XML version='1.0'?>", " <?xml version='1.0'?>",
        "<?xml encoding='UTF-8'?>", "<?xml version='1.0' unknown='value'?>",
        "<?xml version='1.0' standalone='yes' encoding='UTF-8'?>",
        "<?xml version='1.0' standalone='maybe'?>",
    ] {
        assert!(xbel(&format!("{prefix}<xbel><bookmark href='file:///work/real'/></xbel>")).is_none(), "{prefix:?}");
    }
    reject_after_bookmark(&["<info\u{a0}id='bad'/>", "&#x+20;", "&#+32;"]);
}

#[test]
fn searchprior_fix2_corruption_after_the_record_cap_still_drops_the_provider() {
    let bookmarks = "<bookmark href='file:///work/real'/>".repeat(RECORDS);
    for fragment in ["<??>", "<![CDATA[\u{0}]]>", "]]>"] {
        assert!(xbel(&format!("<xbel>{bookmarks}{fragment}</xbel>")).is_none(), "{fragment:?}");
    }
}

#[test]
fn searchprior_fix2_valid_ignored_constructs_keep_real_bookmarks() {
    let text = concat!(
        "\u{feff}<?xml version='1.0' encoding='UTF-8' standalone='yes'?>",
        "<?xml-stylesheet href='a.css'?><?target?><xbel>",
        "<!-- <bookmark href='file:///fake'/> -->",
        "<?target <bookmark href='file:///fake'/> &unknown; ]]>?>",
        "<![CDATA[<bookmark href='file:///fake'/> &unknown;]]>",
        "<title>&#93;]&gt;</title><bookmark href='file:///work/real'/>",
        "</xbel><?after data?>",
    );
    assert_eq!(xbel(text), Some(vec![b"/work/real".to_vec()]));
}
