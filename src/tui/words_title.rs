//! Typed title runs: a semantic Text keeps prose, data and number runs apart until the final title fit.
use super::super::cells::{self, Run};
use super::Text;
use crate::i18n::I18n;

// Unicode noncharacters stand in for the holes, so the catalog's own %N substitution places every argument.
const HOLE: u32 = 0xfdd0;
const HOLES: usize = 32;

impl Text {
    // The same words render() draws, split by role; the data stays as stored and is sanitized only when fitted.
    pub fn spans(&self, i18n: &I18n) -> Vec<Span> {
        let mut out = Vec::new();
        self.push_spans(i18n, &mut out);
        out
    }
    fn push_spans(&self, i18n: &I18n, out: &mut Vec<Span>) {
        match self {
            Self::Data(text) => out.push((Some(Run::Name), text.clone())),
            Self::Key(key) => prose(out, &i18n.tr(key)),
            Self::Args(key, args) => template(self, i18n, key, None, args, out),
            Self::Plural(key, count, args) => template(self, i18n, key, Some(*count), args, out),
            Self::Count(_) | Self::Size(_) => out.push((Some(Run::Number), self.render(i18n))),
            Self::Joined(parts) => parts.iter().for_each(|part| part.push_spans(i18n, out)),
            Self::Cause(_) => out.push((Some(Run::Name), self.render(i18n))),
        }
    }
}

fn prose(out: &mut Vec<Span>, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some((None, last)) => last.push_str(text),
        _ => out.push((None, text.into())),
    }
}

fn hole(c: char) -> Option<usize> {
    (c as u32)
        .checked_sub(HOLE)
        .map(|index| index as usize)
        .filter(|index| *index < HOLES)
}

// Sample input: "%3 :%2 אל %1" with Args [Data, Data, Size] gives [Number, prose " :", Name, prose " אל ", Name].
fn template(
    whole: &Text,
    i18n: &I18n,
    key: &str,
    count: Option<usize>,
    args: &[Text],
    out: &mut Vec<Span>,
) {
    let fill = |values: &[&str]| match count {
        Some(count) => i18n.trn_args(key, count.min(i64::MAX as usize) as i64, values),
        None => i18n.tr_args(key, values),
    };
    // A catalog value that already holds a noncharacter, or more arguments than holes, keeps the whole text as data.
    let blanks = vec![""; args.len()];
    if args.len() > HOLES || fill(&blanks).chars().any(|c| hole(c).is_some()) {
        out.push((Some(Run::Name), whole.render(i18n)));
        return;
    }
    let holes: Vec<String> = (0..args.len())
        .map(|index| {
            char::from_u32(HOLE + index as u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .collect();
    let holes: Vec<&str> = holes.iter().map(String::as_str).collect();
    // A plural's count fills %1 in the locale's own digits; each of its appearances is a number run.
    let number = count.map(|count| {
        i18n.locale()
            .format_count(count.min(i64::MAX as usize) as i64)
    });
    let mut text = String::new();
    for c in fill(&holes).chars() {
        let Some(index) = hole(c) else {
            text.push(c);
            continue;
        };
        counted(out, &std::mem::take(&mut text), number.as_deref());
        args[index].push_spans(i18n, out);
    }
    counted(out, &text, number.as_deref());
}

fn counted(out: &mut Vec<Span>, text: &str, number: Option<&str>) {
    let Some(number) = number.filter(|number| !number.is_empty()) else {
        return prose(out, text);
    };
    let mut parts = text.split(number);
    prose(out, parts.next().unwrap_or(""));
    for part in parts {
        out.push((Some(Run::Number), number.into()));
        prose(out, part);
    }
}

// A title as typed runs: None is trusted prose, Some names the policy role of a data, number or key run.
pub type Span = (Option<Run>, String);
// Fits cells before adding zero-cell isolates, so every cut stays balanced and an isolated cut keeps its ellipsis in its run.
pub fn typed(mark: &str, spans: &[Span], columns: usize, isolating: bool, bold: bool) -> String {
    if columns == 0 {
        return String::new();
    }
    let total = cells::text_width(mark)
        + spans
            .iter()
            .map(|(_, text)| cells::text_width(text))
            .sum::<usize>();
    let cut = total > columns;
    let limit = if cut { columns - 1 } else { columns };
    let mut shown: Vec<(Option<usize>, char)> = Vec::new();
    let mut used = 0;
    let runs = spans.iter().enumerate().flat_map(|(index, (_, text))| {
        cells::characters(text).map(move |(c, n)| (Some(index), c, n))
    });
    for (group, c, n) in cells::characters(mark)
        .map(|(c, n)| (None, c, n))
        .chain(runs)
    {
        if used + n > limit {
            break;
        }
        shown.push((group, c));
        used += n;
    }
    let last = shown
        .iter()
        .rev()
        .find(|&&(group, c)| group.is_some() && cells::width(c) > 0)
        .map(|&(group, _)| group);
    // A run shown whole was not cut, so an isolated ellipsis after it stays outside its isolate.
    let whole = last.flatten().is_some_and(|index| {
        shown
            .iter()
            .filter(|&&(group, _)| group == Some(index))
            .count()
            == cells::characters(&spans[index].1).count()
    });
    let end = shown.len();
    shown.resize(end + limit - used, (None, ' '));
    if cut {
        // Plain titles keep the ellipsis in the last cell; an isolated one ends its own run, before the blank a wide glyph left.
        shown.insert(
            if isolating { end } else { shown.len() },
            (last.flatten(), '…'),
        );
    }
    let outside = isolating && cut && whole;
    let mut out = String::new();
    let mut bolded = false;
    let mut open: Option<(usize, char)> = None;
    for (at, (group, c)) in shown.into_iter().enumerate() {
        let apart = outside && at == end;
        if open.is_some_and(|(index, _)| apart || Some(index) != group) {
            out.push(open.take().map_or(' ', |(_, close)| close));
        }
        if bold && group.is_some() != bolded {
            out.push_str(if group.is_some() {
                "\x1b[1m"
            } else {
                "\x1b[22m"
            });
            bolded = group.is_some();
        }
        if let Some(index) = group.filter(|_| isolating && open.is_none() && !apart) {
            if let Some((start, close)) = spans[index].0.and_then(cells::markers) {
                out.push(start);
                open = Some((index, close));
            }
        }
        out.push(c);
    }
    if let Some((_, close)) = open {
        out.push(close);
    }
    if bolded {
        out.push_str("\x1b[22m");
    }
    out
}

#[cfg(test)]
#[path = "words_title_tests.rs"]
mod tests;
