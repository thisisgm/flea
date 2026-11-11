// Print and PDF conversion share the same bounded, normalized page selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pages {
    All,
    Ranges(Vec<(u32, u32)>),
}

pub const MAX_PAGES: u32 = 10_000;
pub const MAX_RANGE_BYTES: usize = 1024;
pub const RANGE_ERROR: &str = "Enter All or a page range such as 1-3, 5.";

impl Pages {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("all") {
            return Ok(Self::All);
        }
        if text.len() > MAX_RANGE_BYTES {
            return Err(RANGE_ERROR.into());
        }
        let number = |s: &str| -> Result<u32, String> {
            let s = s.trim();
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(RANGE_ERROR.into());
            }
            s.parse::<u32>()
                .ok()
                .filter(|n| (1..=MAX_PAGES).contains(n))
                .ok_or_else(|| RANGE_ERROR.into())
        };
        let mut ranges = Vec::new();
        for part in text.split(',') {
            let range = match part.split_once('-') {
                Some((a, b)) => (number(a)?, number(b)?),
                None => {
                    let n = number(part)?;
                    (n, n)
                }
            };
            if range.0 > range.1 {
                return Err(RANGE_ERROR.into());
            }
            ranges.push(range);
        }
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (a, b) in ranges {
            if let Some(last) = merged.last_mut().filter(|last| a <= last.1 + 1) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
        }
        Ok(Self::Ranges(merged))
    }

    pub fn cups(&self) -> Option<String> {
        match self {
            Self::All => None,
            Self::Ranges(ranges) => Some(
                ranges
                    .iter()
                    .map(|(a, b)| {
                        if a == b {
                            a.to_string()
                        } else {
                            format!("{a}-{b}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        }
    }

    pub fn selected(&self, count: u32) -> Result<Vec<u32>, String> {
        if !(1..=MAX_PAGES).contains(&count) {
            return Err(format!(
                "PDF conversion supports between 1 and {MAX_PAGES} pages."
            ));
        }
        match self {
            Self::All => Ok((1..=count).collect()),
            Self::Ranges(ranges) if ranges.iter().all(|(_, end)| *end <= count) => {
                Ok(ranges.iter().flat_map(|(a, b)| *a..=*b).collect())
            }
            _ => Err(format!("This document has {count} pages.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_and_convert_share_sorted_deduplicated_ranges() {
        let pages = Pages::parse("5, 1-3, 2, 6-7").unwrap();
        assert_eq!(pages.cups().as_deref(), Some("1-3,5-7"));
        assert_eq!(pages.selected(8).unwrap(), [1, 2, 3, 5, 6, 7]);
        assert!(pages.selected(6).is_err());
        assert_eq!(Pages::parse(" All ").unwrap().cups(), None);
        assert_eq!(Pages::parse("").unwrap().selected(2).unwrap(), [1, 2]);
    }

    #[test]
    fn hostile_and_ambiguous_ranges_are_refused_without_expansion() {
        for text in [
            "0",
            "2-1",
            "1,",
            ",1",
            "1--3",
            "-1",
            "+1",
            "1 2",
            "10001",
            "4294967295",
            "1-2-3",
            "all,1",
        ] {
            assert_eq!(Pages::parse(text).unwrap_err(), RANGE_ERROR, "{text}");
        }
        assert!(Pages::parse(&"1,".repeat(600)).is_err());
        assert!(Pages::All.selected(u32::MAX).is_err());
    }
}
