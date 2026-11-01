// CUPS destination and option parsing, plus the exact per-file print command.
use super::pages::Pages;
use std::path::Path;

pub const NO_PRINTER: &str = "No printer is set up.";
pub const MIN_COPIES: usize = 1;
pub const MAX_COPIES: usize = 999;
const MAX_DESTINATION_BYTES: usize = 255;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub name: String,
    pub default: bool,
}

// lpstat -e sample: Brother
pub fn destinations(list: &str, default: &str) -> Vec<Destination> {
    // lpstat -d sample: system default destination: Brother
    let default = default
        .lines()
        .find_map(|line| line.strip_prefix("system default destination: "))
        .unwrap_or("")
        .trim();
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<_> = list
        .lines()
        .map(str::trim)
        .filter(|name| valid_destination(name) && seen.insert((*name).to_string()))
        .map(|name| Destination {
            name: name.into(),
            default: name == default,
        })
        .collect();
    result.sort_by_key(|d| !d.default);
    result
}

// lpstat -l -p supplies the human name as Description, while the destination remains the queue ID.
// Sample input: "printer Brother is idle.  enabled since Sat 31 Oct\n\tDescription: Brother HL-L3270CDW\n".
pub fn printer_name(details: &str, queue: &str) -> String {
    details
        .lines()
        .find_map(|line| line.trim().strip_prefix("Description:"))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(queue)
        .into()
}

pub fn valid_destination(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_DESTINATION_BYTES
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub duplex: bool,
    pub two_sided: bool,
    pub color: bool,
    pub grayscale: bool,
}

// lpoptions -p Brother -l sample: Duplex/Two-sided: None *DuplexNoTumble DuplexTumble
pub fn capabilities(text: &str) -> Capabilities {
    let mut caps = Capabilities::default();
    for line in text.lines() {
        let Some((key, choices)) = line.split_once(':') else {
            continue;
        };
        let key = key.split('/').next().unwrap_or("").to_ascii_lowercase();
        if key == "duplex" || key == "sides" {
            let two = |s: &str| {
                matches!(
                    s.to_ascii_lowercase().as_str(),
                    "duplexnotumble"
                        | "duplextumble"
                        | "two-sided-long-edge"
                        | "two-sided-short-edge"
                )
            };
            caps.duplex |= choices
                .split_whitespace()
                .any(|s| two(s.trim_start_matches('*')));
            caps.two_sided |= choices
                .split_whitespace()
                .filter_map(|s| s.strip_prefix('*'))
                .any(two);
        }
        if matches!(
            key.as_str(),
            "colormodel" | "colormode" | "print-color-mode" | "outputmode"
        ) {
            let color = |s: &str| {
                matches!(
                    s.to_ascii_lowercase().as_str(),
                    "rgb" | "cmyk" | "cmy" | "color" | "colour" | "srgb" | "adobergb" | "rgba"
                )
            };
            let gray = |s: &str| {
                matches!(
                    s.to_ascii_lowercase().as_str(),
                    "gray" | "grey" | "grayscale" | "monochrome" | "black" | "blackandwhite"
                )
            };
            caps.color |= choices
                .split_whitespace()
                .any(|s| color(s.trim_start_matches('*')));
            caps.grayscale |= choices
                .split_whitespace()
                .filter_map(|s| s.strip_prefix('*'))
                .any(gray);
        }
    }
    caps
}

#[derive(Clone, Debug)]
pub struct Options {
    pub destination: String,
    pub copies: usize,
    pub pages: Pages,
    pub two_sided: bool,
    pub grayscale: bool,
}

pub fn argv(
    file: &Path,
    image: bool,
    paged: bool,
    options: &Options,
) -> Result<Vec<String>, String> {
    if !valid_destination(&options.destination) {
        return Err("Choose a printer.".into());
    }
    if !(MIN_COPIES..=MAX_COPIES).contains(&options.copies) {
        return Err(format!(
            "Copies must be between {MIN_COPIES} and {MAX_COPIES}."
        ));
    }
    let mut argv = vec![
        "lp".into(),
        "-d".into(),
        options.destination.clone(),
        "-n".into(),
        options.copies.to_string(),
    ];
    if let Some(pages) = paged.then(|| options.pages.cups()).flatten() {
        argv.extend(["-P".into(), pages]);
    }
    if options.two_sided {
        argv.extend(["-o".into(), "sides=two-sided-long-edge".into()]);
    }
    if options.grayscale {
        argv.extend(["-o".into(), "print-color-mode=monochrome".into()]);
    }
    if image {
        argv.extend(["-o".into(), "fit-to-page".into()]);
    }
    argv.extend(["--".into(), file.to_string_lossy().into_owned()]);
    Ok(argv)
}

pub fn receipt(name: &str, printer: &str) -> String {
    format!("Sent {name} to {printer}")
}

pub fn refusal(name: &str, reason: &str) -> String {
    format!(
        "{name} was not printed: {}.",
        reason.trim().trim_end_matches('.')
    )
}

pub fn setup_argv() -> Vec<String> {
    vec!["system-config-printer".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adv_e_r1_duplex_options_accumulate_in_both_orders() {
        let duplex = "Duplex/Two-sided: None *DuplexNoTumble DuplexTumble\n";
        let sides = "sides/Sides: *one-sided\n";
        for text in [format!("{duplex}{sides}"), format!("{sides}{duplex}")] {
            let caps = capabilities(&text);
            assert!(
                caps.duplex,
                "supported duplex must survive either option order: {text}"
            );
            assert!(
                caps.two_sided,
                "the duplex default must survive either option order: {text}"
            );
        }
    }

    #[test]
    fn vf4_r1_printer_name_falls_back_to_the_queue_without_a_description() {
        let named =
            "printer Brother is idle.  enabled since Sat\n\tDescription: Brother HL-L3270CDW \n";
        assert_eq!(printer_name(named, "Brother"), "Brother HL-L3270CDW");
        for details in ["", "printer Brother is idle.\n", "\tDescription:   \n"] {
            assert_eq!(printer_name(details, "Brother"), "Brother", "{details:?}");
        }
    }

    #[test]
    fn the_system_default_is_first_without_losing_destination_order() {
        let got = destinations(
            "Office\nBrother\nOffice\nKitchen\n",
            "system default destination: Brother\n",
        );
        assert_eq!(
            got.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["Brother", "Office", "Kitchen"]
        );
        assert!(got[0].default);
        assert!(!got[1].default);
        assert!(destinations("", "").is_empty());
        assert!(!valid_destination("-o"));
        assert!(!valid_destination("bad\nprinter"));
    }

    #[test]
    fn duplex_and_color_capabilities_keep_the_reported_default() {
        assert_eq!(
            capabilities(
                "Duplex/2-Sided: None *DuplexNoTumble DuplexTumble\nColorModel/Color: *RGB Gray\n"
            ),
            Capabilities {
                duplex: true,
                two_sided: true,
                color: true,
                grayscale: false
            }
        );
        assert_eq!(
            capabilities("Duplex/Two-sided: *None\nColorModel/Color: *Gray\n"),
            Capabilities {
                grayscale: true,
                ..Capabilities::default()
            }
        );
        assert_eq!(capabilities(""), Capabilities::default());
        assert!(capabilities("print-color-mode/Color: color *monochrome").color);
    }

    #[test]
    fn lp_matches_the_spec_and_never_reads_the_path_as_an_option() {
        assert_eq!(setup_argv(), ["system-config-printer"]);
        let options = Options {
            destination: "Brother".into(),
            copies: 3,
            pages: Pages::parse("1-3, 5").unwrap(),
            two_sided: true,
            grayscale: true,
        };
        assert_eq!(
            argv(Path::new("/x/-report.pdf"), false, true, &options).unwrap(),
            [
                "lp",
                "-d",
                "Brother",
                "-n",
                "3",
                "-P",
                "1-3,5",
                "-o",
                "sides=two-sided-long-edge",
                "-o",
                "print-color-mode=monochrome",
                "--",
                "/x/-report.pdf"
            ]
        );
        let image = argv(Path::new("/x/a.jpg"), true, false, &options).unwrap();
        assert!(!image.iter().any(|a| a == "-P"));
        assert_eq!(
            &image[image.len() - 4..],
            ["-o", "fit-to-page", "--", "/x/a.jpg"]
        );
        assert_eq!(
            receipt("report.pdf", "Brother HL-L3270CDW"),
            "Sent report.pdf to Brother HL-L3270CDW"
        );
        assert_eq!(
            refusal("report.pdf", "printer unavailable.\n"),
            "report.pdf was not printed: printer unavailable."
        );
    }
}
