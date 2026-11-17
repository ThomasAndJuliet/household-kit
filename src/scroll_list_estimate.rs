// SPDX-License-Identifier: GPL-3.0-only

//! Cheap height estimates for list items that have not been laid out.
//!
//! An estimate stands in for an item's height until the list lays the item
//! out at the current width. It comes from what the item holds, summarised
//! without shaping any text ([`ContentSummary`]): running text by its
//! characters, monospaced text by its lines' lengths, and other content by
//! a size in body ems. The list corrects estimates per [`EstimateClass`] as
//! it measures items, so the summary only has to be roughly proportional.

/// A piece of an item's content, for estimating its height
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    /// Running text: its characters, at a size relative to the body's
    Prose { chars: u32, scale: f32 },
    /// Monospaced lines, by their character counts, on a plate
    Mono { lines: Vec<u32>, scale: f32 },
    /// Something of about this height, in body ems: a formula, an image,
    /// a chart or a table
    Fixed(f32),
}

/// What an item holds, as far as its height goes
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentSummary {
    pub segments: Vec<Segment>,
    /// Room taken from the width by what stands beside the content, in
    /// pixels
    pub indent: f32,
}

impl ContentSummary {
    /// Markdown not yet parsed: its paragraphs and its fenced code, from
    /// the source's lines
    pub fn of_markdown(text: &str) -> Self {
        let mut segments = Vec::new();
        let mut prose = 0usize;
        let mut code: Option<Vec<u32>> = None;
        for line in text.lines() {
            let fence =
                line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
            match (&mut code, fence) {
                (Some(lines), false) => lines.push(line.chars().count() as u32),
                (Some(_), true) => segments.push(Segment::Mono {
                    lines: code.take().unwrap_or_default(),
                    scale: MONO_SCALE,
                }),
                (None, true) => {
                    if prose > 0 {
                        segments.push(Segment::Prose {
                            chars: prose as u32,
                            scale: 1.0,
                        });
                        prose = 0;
                    }
                    code = Some(Vec::new());
                }
                (None, false) if line.trim().is_empty() => {
                    if prose > 0 {
                        segments.push(Segment::Prose {
                            chars: prose as u32,
                            scale: 1.0,
                        });
                        prose = 0;
                    }
                }
                (None, false) => prose += line.chars().count() + 1,
            }
        }
        if let Some(lines) = code {
            segments.push(Segment::Mono {
                lines,
                scale: MONO_SCALE,
            });
        }
        if prose > 0 || segments.is_empty() {
            segments.push(Segment::Prose {
                chars: prose as u32,
                scale: 1.0,
            });
        }
        Self {
            segments,
            indent: 0.0,
        }
    }

    /// One line of running text, as a row's title is
    pub fn of_text(text: &str, scale: f32) -> Self {
        Self {
            segments: vec![Segment::Prose {
                chars: text.chars().count() as u32,
                scale,
            }],
            indent: 0.0,
        }
    }

    /// Monospaced text, as a tool's output shows: its lines exactly
    pub fn mono(text: &str, scale: f32) -> Segment {
        Segment::Mono {
            lines: line_lengths(text),
            scale,
        }
    }
}

/// Code's size against the body's, as code blocks are set
pub const MONO_SCALE: f32 = 0.88;

fn line_lengths(text: &str) -> Vec<u32> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n')
        .map(|line| line.chars().count() as u32)
        .collect()
}

/// The text measures an estimate needs, as the list's body text is set
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// Pixels per em of body text
    pub size: f32,
    /// Line height, as a multiple of the size
    pub line_height: f32,
    /// The body font's average advance, in ems
    pub advance: f32,
    /// The monospaced font's advance, in ems
    pub mono_advance: f32,
    /// Code's size against the body's
    pub code_scale: f32,
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self {
            size: 15.0,
            line_height: 1.5,
            advance: 0.5,
            mono_advance: 0.6,
            code_scale: 1.0,
        }
    }
}

impl TextMetrics {
    /// The height `summary` takes at `width`, by its content alone
    pub fn estimate(&self, summary: &ContentSummary, width: f32) -> f32 {
        let size = self.size;
        let width = (width - summary.indent).max(size * 4.0);
        let mut height = 0.0;
        for (i, segment) in summary.segments.iter().enumerate() {
            if i > 0 {
                height += size * 0.7;
            }
            height += match segment {
                Segment::Prose { chars, scale } => {
                    let line = size * scale * self.line_height;
                    let per_line = (width / (size * scale * self.advance)).max(1.0);
                    let lines = (*chars as f32 / per_line).ceil().max(1.0);
                    lines * line
                }
                Segment::Mono { lines, scale } => {
                    // On a plate padded by half an em, one line per line
                    // and per wrap of a long one
                    let mono = size * scale * self.code_scale;
                    let pad = size * 0.5;
                    let columns = ((width - 2.0 * pad) / (mono * self.mono_advance))
                        .floor()
                        .max(1.0) as u32;
                    let rows: u32 = lines.iter().map(|len| len.div_ceil(columns).max(1)).sum();
                    rows.max(1) as f32 * mono * self.line_height + 2.0 * pad
                }
                Segment::Fixed(ems) => ems * size,
            };
        }
        height
    }
}

/// Items whose estimates err alike: the list learns one correction per
/// class from the items of it that it lays out. The app chooses the
/// classes; items of different shapes (open and closed cards, rows and
/// headings) belong in different ones
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EstimateClass(pub u8);

/// How tall an item is likely to be before it is laid out
#[derive(Clone, Debug, PartialEq)]
pub enum Estimate {
    /// Its content, at the list's width, and `around` body ems of room
    /// around it (a label, a header). The list learns the ratio of laid-out
    /// to estimated content per class
    Content {
        summary: ContentSummary,
        around: f32,
    },
    /// About `ems` body ems tall, whatever the width, as a closed card or
    /// a one-line row is. The list learns the actual height per class
    Fixed { ems: f32 },
}

impl Default for Estimate {
    fn default() -> Self {
        Self::Content {
            summary: ContentSummary::default(),
            around: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monospaced_lines_are_counted_exactly() {
        let metrics = TextMetrics {
            size: 10.0,
            line_height: 1.5,
            advance: 0.5,
            mono_advance: 0.6,
            code_scale: 1.0,
        };
        // 10 px per em, code at 8.8 px, 5.28 px per column: 100 px of plate
        // less 10 px of padding holds 17 columns
        let summary = ContentSummary {
            segments: vec![Segment::Mono {
                lines: vec![17, 18, 0, 35],
                scale: 0.88,
            }],
            indent: 0.0,
        };
        let rows = 1 + 2 + 1 + 3;
        let expected = rows as f32 * 8.8 * 1.5 + 10.0;
        assert!((metrics.estimate(&summary, 100.0) - expected).abs() < 0.01);
    }

    #[test]
    fn markdown_splits_into_prose_and_fenced_code() {
        let summary = ContentSummary::of_markdown("one two\nthree\n\n```\nab\nabcd\n```\nafter");
        assert_eq!(
            summary.segments,
            vec![
                Segment::Prose {
                    chars: 14,
                    scale: 1.0
                },
                Segment::Mono {
                    lines: vec![2, 4],
                    scale: MONO_SCALE
                },
                Segment::Prose {
                    chars: 6,
                    scale: 1.0
                },
            ]
        );
    }
}
