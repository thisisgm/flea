//! Adapted from yazi-fs/src/normalizer.rs, MIT, copyright 2023 sxyazi.
//! The license is retained in docs/licenses/yazi-normalizer-MIT.txt.
use std::{borrow::Cow, mem};

use regex_syntax::ast::{
    self, Alternation, Ast, ClassUnicode, ClassUnicodeKind, Concat, Flag, Flags, Group, GroupKind,
    Literal, LiteralKind, Repetition, RepetitionKind, RepetitionOp, Span,
};
use unicode_normalization::UnicodeNormalization;

pub(super) struct Normalizer;

impl Normalizer {
    pub(super) fn normalize(pat: &str) -> Result<Cow<'_, str>, Box<ast::Error>> {
        if !Self::must_normalize(pat) {
            return Ok(pat.into());
        }

        let mut ast = ast::parse::Parser::new().parse(pat).map_err(Box::new)?;
        Ok(if Self::normalize_ast(&mut ast, &mut true) {
            ast.to_string().into()
        } else {
            pat.into()
        })
    }

    fn must_normalize(pat: &str) -> bool {
        if !pat.is_ascii() || pat.contains('.') {
            return true;
        }

        pat.as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'\\' && matches!(pair[1], b'x' | b'u' | b'U'))
    }

    fn normalize_ast(ast: &mut Ast, unicode: &mut bool) -> bool {
        match ast {
            Ast::Dot(span) if *unicode => {
                *ast = Self::normalize_dot(**span);
                true
            }
            Ast::Literal(literal) => {
                let (normalized, changed) =
                    Self::normalize_literals(vec![Ast::Literal(literal.clone())], *unicode);
                if changed {
                    *ast = normalized;
                }
                changed
            }
            Ast::Repetition(repetition) => Self::normalize_ast(&mut repetition.ast, unicode),
            Ast::Group(group) => {
                let old = *unicode;
                if let GroupKind::NonCapturing(flags) = &group.kind {
                    *unicode = flags.flag_state(Flag::Unicode).unwrap_or(*unicode);
                }

                let changed = Self::normalize_ast(&mut group.ast, unicode);
                *unicode = old;
                changed
            }
            Ast::Alternation(alternation) => {
                let mut changed = false;
                for ast in &mut alternation.asts {
                    changed |= Self::normalize_ast(ast, unicode);
                }
                changed
            }
            Ast::Concat(concat) => Self::normalize_concat(concat, unicode),
            Ast::Flags(flags) => {
                *unicode = flags.flags.flag_state(Flag::Unicode).unwrap_or(*unicode);
                false
            }
            _ => false,
        }
    }

    fn normalize_dot(span: Span) -> Ast {
        let mark = Ast::class_unicode(ClassUnicode {
            span,
            negated: false,
            kind: ClassUnicodeKind::OneLetter('M'),
        });
        let marks = Ast::repetition(Repetition {
            span,
            op: RepetitionOp {
                span: Span::splat(span.end),
                kind: RepetitionKind::ZeroOrMore,
            },
            greedy: true,
            ast: Box::new(mark),
        });

        // Rewrite `.` as `(?:.\p{M}*)`: a decomposed character is a base
        // character followed by combining marks, so match the whole sequence.
        Ast::group(Group {
            span,
            kind: GroupKind::NonCapturing(Flags {
                span,
                items: vec![],
            }),
            ast: Box::new(
                Concat {
                    span,
                    asts: vec![Ast::dot(span), marks],
                }
                .into_ast(),
            ),
        })
    }

    fn normalize_concat(concat: &mut Concat, unicode: &mut bool) -> bool {
        let mut changed = false;
        let mut asts = Vec::with_capacity(concat.asts.len());
        let mut literals = vec![];

        for mut ast in mem::take(&mut concat.asts) {
            if matches!(ast, Ast::Literal(_)) {
                literals.push(ast);
                continue;
            }

            if !literals.is_empty() {
                let (literal, normalized) =
                    Self::normalize_literals(mem::take(&mut literals), *unicode);
                asts.push(literal);
                changed |= normalized;
            }

            changed |= Self::normalize_ast(&mut ast, unicode);
            asts.push(ast);
        }

        if !literals.is_empty() {
            let (literal, normalized) = Self::normalize_literals(literals, *unicode);
            asts.push(literal);
            changed |= normalized;
        }

        concat.asts = asts;
        changed
    }

    fn normalize_literals(literals: Vec<Ast>, unicode: bool) -> (Ast, bool) {
        let span = Span::new(
            literals.first().unwrap().span().start,
            literals.last().unwrap().span().end,
        );

        // In `(?-u:...)`, which disables Unicode, literals and `\xNN` escapes are byte
        // sequences, so expanding `ä` into the UTF-8 bytes for `a\u{308}` would
        // change the regex.
        if !unicode {
            return (
                Concat {
                    span,
                    asts: literals,
                }
                .into_ast(),
                false,
            );
        }

        let chars = || {
            literals.iter().map(|literal| {
                let Ast::Literal(literal) = literal else {
                    unreachable!()
                };
                literal.c
            })
        };

        let raw_is_nfc = chars().eq(chars().nfc());
        let raw_is_nfd = chars().eq(chars().nfd());
        if raw_is_nfc && raw_is_nfd {
            return (
                Concat {
                    span,
                    asts: literals,
                }
                .into_ast(),
                false,
            );
        }

        let mut asts = Vec::with_capacity(3);
        let nfc = chars().nfc().collect::<String>();
        let nfd = chars().nfd().collect::<String>();

        asts.push(
            Concat {
                span,
                asts: literals,
            }
            .into_ast(),
        );
        if !raw_is_nfc {
            asts.push(Self::literal_ast(&nfc, span));
        }
        if !raw_is_nfd && nfc != nfd {
            asts.push(Self::literal_ast(&nfd, span));
        }

        // Rewrite a literal sequence as `(?:raw|NFC|NFD)`, keeping the original
        // form and adding only distinct canonical forms.
        let group = Ast::group(Group {
            span,
            kind: GroupKind::NonCapturing(Flags {
                span,
                items: vec![],
            }),
            ast: Box::new(Alternation { span, asts }.into_ast()),
        });
        (group, true)
    }

    fn literal_ast(text: &str, span: Span) -> Ast {
        Concat {
            span,
            asts: text
                .chars()
                .map(|c| {
                    Ast::literal(Literal {
                        span,
                        kind: LiteralKind::Verbatim,
                        c,
                    })
                })
                .collect(),
        }
        .into_ast()
    }
}
