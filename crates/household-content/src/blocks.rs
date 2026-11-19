// SPDX-License-Identifier: GPL-3.0-only

//! The text's analysis kept up to date edit by edit (option C, decided by
//! Juliet on 15 Oct): a keystroke re-parses the block it is in, not the
//! document.
//!
//! The text is cut into *chunks*: runs of whole lines, each starting at a
//! line where nothing is open — a block at the top level, after a blank
//! line, in the first column, and the maths rewriting not inside a fence or
//! a `\[`. These are the scanner's checkpoints: CommonMark reads what follows
//! such a line the same whatever came before it, so each chunk can be parsed
//! on its own, by pulldown-cmark (the one authority on what the text means;
//! no second Markdown grammar), and its leaves are the same as a parse of
//! the whole text gives.
//!
//! After an edit the chunks from the one before the edited line are parsed
//! again, as far as the first old checkpoint after the edit that is still
//! one: there the state matches and the rest is as it was. A keystroke in a
//! paragraph parses one or two blocks; opening a ``` fence parses on until
//! the state matches again, which may be the end of the text.
//!
//! What reaches past a chunk is kept as document-wide indexes: link
//! reference definitions (a reference is resolved from them, through
//! pulldown-cmark's broken-link callback), footnote definitions (put before
//! a chunk that refers to them), and a `---` rule at the text's start
//! that would become front matter if a closing line came later (the parse
//! looks ahead to the end of the text for one; front matter is only ever
//! at the start). When a definition changes, the chunks that
//! use its label are parsed again; a label defined in two chunks, where the
//! first wins, sends every edit to a whole parse until it is gone.
//!
//! `differential` below checks it: after every edit of random edit scripts
//! the analysis equals a parse of the whole text.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use unicase::UniCase;

use crate::Dialect;
use crate::syntax::{self, Analysis, ContainerKind, Extras, LeafKind};

/// Link reference definitions by label: destination and title
type Definitions = HashMap<UniCase<String>, (String, String)>;

/// A run of whole lines that starts where nothing is open
#[derive(Clone, Debug)]
struct Chunk {
    /// Changes whenever the chunk is parsed again
    id: u64,
    /// The text's revision it was parsed at
    revision: u64,
    start: usize,
    /// Its leaves and containers in the analysis: where they start, and
    /// how many
    leaf_start: usize,
    leaves: usize,
    container_start: usize,
    containers: usize,
    /// Link reference definitions in it: label, destination, title; or,
    /// with `unknown_defs`, not known for certain (see [`definitions`])
    defs: Vec<(String, String, String)>,
    unknown_defs: bool,
    /// The labels of the references in it
    refs: HashSet<UniCase<String>>,
    /// Footnote definitions' labels, and whether it has a `[^`
    footnotes: Vec<String>,
    footnote_refs: bool,
    /// It has a `---` line whose reading depends on whether a closing line
    /// comes later in the text: a rule at the text's start that one would
    /// make front matter
    rule: bool,
}

/// What an edit changed in the analysis: the leaves at `leaves` (in the
/// analysis as it was before this splice) are now `count` leaves, and every
/// offset after them moved by `delta` bytes. An edit makes one or more, in
/// order
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    pub leaves: Range<usize>,
    pub count: usize,
    pub delta: isize,
    /// The leaves are as many as before, each of the kind it was, in the
    /// same containers, and as many lines long: only their text changed
    /// (what depends on the text's shape alone, the folds, holds)
    pub same_shape: bool,
}

impl Splice {
    /// Where leaf `old` is after the splice, if it was outside it
    pub fn map(&self, old: usize) -> Option<usize> {
        if old < self.leaves.start {
            Some(old)
        } else if old >= self.leaves.end {
            Some(old - self.leaves.end + self.leaves.start + self.count)
        } else {
            None
        }
    }
}

/// The analysis, and the chunks it is kept in
#[derive(Clone, Debug)]
pub struct Blocks {
    analysis: Analysis,
    chunks: Vec<Chunk>,
    next_id: u64,
    /// A link label defined in more than one chunk: the first definition
    /// wins, which a chunk parsed alone cannot see, so every edit parses
    /// the whole text until it is gone
    irregular: bool,
    /// How the text is read
    dialect: Dialect,
    /// For tests and benchmarks: bytes parsed by the last edit
    pub parsed: usize,
}

/// A parse of `text[range]` (with the line after it, to see whether a
/// block starts there), its offsets the text's
struct Region {
    analysis: Analysis,
    extras: Extras,
    /// The line after the range starts a block with nothing open
    ends_clean: bool,
}

impl Blocks {
    /// The analysis of `text`, a document, made from a parse of the whole
    /// of it
    pub fn new(text: &str, revision: u64) -> Self {
        Self::new_in(text, revision, Dialect::Document)
    }

    /// [`Self::new`] for a text in `dialect`
    pub fn new_in(text: &str, revision: u64, dialect: Dialect) -> Self {
        let mut blocks = Blocks {
            analysis: Analysis::default(),
            chunks: Vec::new(),
            next_id: 1,
            irregular: false,
            dialect,
            parsed: 0,
        };
        blocks.rebuild(text, revision);
        blocks
    }

    pub fn analysis(&self) -> &Analysis {
        &self.analysis
    }

    /// How many chunks the text is in (for tests and benchmarks)
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Parse the whole text again
    fn rebuild(&mut self, text: &str, revision: u64) {
        let region = self.parse_region(text, 0..text.len(), &HashMap::new(), &[]);
        self.parsed = text.len();
        let chunks = self.split(text, 0..text.len(), &region, revision, 0, 0);
        self.analysis = region.analysis;
        self.chunks = chunks;
        self.irregular = self.duplicate_labels();
    }

    /// Bring the analysis up to `text`, which is the text it was for with
    /// `old` (a range of that text, which held `removed`) replaced by
    /// `inserted` bytes. What changed, as splices of the leaves
    pub fn edit(
        &mut self,
        text: &str,
        old: Range<usize>,
        removed: &str,
        inserted: usize,
        revision: u64,
    ) -> Vec<Splice> {
        let delta = inserted as isize - old.len() as isize;
        let before = self.analysis.leaves.len();
        self.parsed = 0;
        if self.irregular || self.chunks.is_empty() {
            self.rebuild(text, revision);
            return vec![Splice {
                leaves: 0..before,
                count: self.analysis.leaves.len(),
                delta,
                same_shape: false,
            }];
        }
        // From the chunk with the line before the edited one: that line's
        // being blank is part of the edited line's being a checkpoint
        let line = syntax::line_start(text, old.start.min(text.len()));
        let mut first = if line == 0 {
            0
        } else {
            self.chunk_at(line - 1)
        };
        // A closing `---` or `...` line typed makes an earlier open rule
        // front matter, down to it
        let new_end = syntax::line_end(text, (old.start + inserted).min(text.len()));
        if self.chunks[..first].iter().any(|c| c.rule) {
            let at = old.start + inserted;
            let was = format!(
                "{}{removed}{}",
                &text[line..old.start],
                &text[at.min(new_end)..new_end]
            );
            if has_closer(&text[line..new_end]) || has_closer(&was) {
                first = self.chunks.iter().position(|c| c.rule).unwrap_or(first);
            }
        }
        // The old checkpoints after the edit, tried in turn: the first
        // that is still one ends the parse
        let after = self
            .chunks
            .partition_point(|c| c.start < old.end || c.start <= old.start)
            .max(first + 1);
        let old_globals = self.global_state();
        let (mut splice, added) = self.reparse(text, first, after, delta, revision);
        // A line break made or taken away changes a leaf's lines
        let new_range = old.start..old.start + inserted;
        splice.same_shape &=
            !removed.contains('\n') && !text.get(new_range).is_some_and(|t| t.contains('\n'));
        let mut splices = vec![splice];
        // Definitions changed: the chunks elsewhere that use them, again
        let new_globals = self.global_state();
        if self.duplicate_labels() {
            let before = self.analysis.leaves.len();
            self.rebuild(text, revision);
            splices.push(Splice {
                leaves: 0..before,
                count: self.analysis.leaves.len(),
                delta: 0,
                same_shape: false,
            });
            return splices;
        }
        let changed: HashSet<&UniCase<String>> = old_globals
            .defs
            .iter()
            .filter(|(k, v)| new_globals.defs.get(*k) != Some(v))
            .map(|(k, _)| k)
            .chain(
                new_globals
                    .defs
                    .keys()
                    .filter(|k| !old_globals.defs.contains_key(*k)),
            )
            .collect();
        let footnotes_changed = old_globals.footnotes != new_globals.footnotes;
        if !changed.is_empty() || footnotes_changed {
            let again: Vec<usize> = (0..self.chunks.len())
                .filter(|&i| i < first || i >= first + added)
                .filter(|&i| {
                    let c = &self.chunks[i];
                    (footnotes_changed && c.footnote_refs)
                        || c.refs.iter().any(|r| changed.contains(r))
                })
                .collect();
            // From the last, so that the earlier ones' indices hold; each
            // as far as it reaches (its blocks are as they were, as
            // definitions change no block, but front matter may look past it)
            for i in again.into_iter().rev() {
                splices.push(self.reparse(text, i, i + 1, 0, revision).0);
            }
            if self.duplicate_labels() {
                let before = self.analysis.leaves.len();
                self.rebuild(text, revision);
                splices.push(Splice {
                    leaves: 0..before,
                    count: self.analysis.leaves.len(),
                    delta: 0,
                    same_shape: false,
                });
            }
        }
        splices
    }

    /// Parse again from chunk `first`, as far as the first of the old
    /// checkpoints from chunk `after` on (moved by `delta` bytes) that is
    /// still one: tried in turn, one, then two further, four, …
    fn reparse(
        &mut self,
        text: &str,
        first: usize,
        after: usize,
        delta: isize,
        revision: u64,
    ) -> (Splice, usize) {
        let start = self.chunks[first].start;
        let mut step = 1;
        let mut candidate = after;
        let (end_chunk, region) = loop {
            let (end, end_chunk) = match self.chunks.get(candidate) {
                Some(c) => ((c.start as isize + delta) as usize, candidate),
                None => (text.len(), self.chunks.len()),
            };
            let outside = |i: usize| i < first || i >= end_chunk;
            let (defs, footnotes) = self.globals(&outside);
            let region = self.parse_region(text, start..end, &defs, &footnotes);
            self.parsed += end - start;
            if end == text.len() {
                break (end_chunk, region);
            }
            // Still a checkpoint, and no front matter reaching past it (an
            // open rule has no closing line up to the line after `end`)
            let closer = if region.extras.open_rules.is_empty() {
                None
            } else {
                closer_after(text, end)
            };
            if region.ends_clean && is_checkpoint(text, end) && closer.is_none() {
                break (end_chunk, region);
            }
            candidate = match closer {
                // Past the closing line that makes a rule front matter
                Some(c) => (candidate + 1).max(
                    self.chunks
                        .partition_point(|ch| (ch.start as isize + delta) as usize <= c),
                ),
                None => {
                    let next = candidate + step;
                    step *= 2;
                    next
                }
            }
            .min(self.chunks.len());
        };
        let end = self
            .chunks
            .get(end_chunk)
            .map_or(text.len(), |c| (c.start as isize + delta) as usize);
        let (leaf_start, container_start) = (
            self.chunks[first].leaf_start,
            self.chunks[first].container_start,
        );
        let old_leaves: usize = self.chunks[first..end_chunk].iter().map(|c| c.leaves).sum();
        let old_containers: usize = self.chunks[first..end_chunk]
            .iter()
            .map(|c| c.containers)
            .sum();
        let new_chunks = self.split(
            text,
            start..end,
            &region,
            revision,
            leaf_start,
            container_start,
        );
        let count = region.analysis.leaves.len();
        let same_shape = same_shape(
            &self.analysis,
            leaf_start..leaf_start + old_leaves,
            container_start..container_start + old_containers,
            &region.analysis,
        );
        splice_analysis(
            &mut self.analysis,
            leaf_start..leaf_start + old_leaves,
            container_start..container_start + old_containers,
            region.analysis,
            delta,
        );
        let dl = count as isize - old_leaves as isize;
        let dc = new_chunks.iter().map(|c| c.containers).sum::<usize>() as isize
            - old_containers as isize;
        let added = new_chunks.len();
        self.chunks.splice(first..end_chunk, new_chunks);
        for c in &mut self.chunks[first + added..] {
            c.start = (c.start as isize + delta) as usize;
            c.leaf_start = (c.leaf_start as isize + dl) as usize;
            c.container_start = (c.container_start as isize + dc) as usize;
        }
        (
            Splice {
                leaves: leaf_start..leaf_start + old_leaves,
                count,
                delta,
                same_shape,
            },
            added,
        )
    }

    /// The chunk byte `at` is in
    fn chunk_at(&self, at: usize) -> usize {
        self.chunks
            .partition_point(|c| c.start <= at)
            .saturating_sub(1)
    }

    /// The link definitions and footnote labels of the chunks `outside`
    /// says are not being parsed: the first definition of each label
    fn globals(&self, outside: &dyn Fn(usize) -> bool) -> (Definitions, Vec<String>) {
        let mut defs = HashMap::new();
        let mut footnotes = Vec::new();
        for (_, c) in self.chunks.iter().enumerate().filter(|(i, _)| outside(*i)) {
            for (label, dest, title) in &c.defs {
                defs.entry(UniCase::new(label.clone()))
                    .or_insert_with(|| (dest.clone(), title.clone()));
            }
            footnotes.extend(c.footnotes.iter().cloned());
        }
        (defs, footnotes)
    }

    fn global_state(&self) -> Globals {
        let (defs, footnotes) = self.globals(&|_| true);
        let footnotes = footnotes.into_iter().map(UniCase::new).collect();
        Globals { defs, footnotes }
    }

    /// A link label defined in two chunks, or definitions not known
    fn duplicate_labels(&self) -> bool {
        let mut seen = HashSet::new();
        for c in &self.chunks {
            if c.unknown_defs {
                return true;
            }
            let own: HashSet<UniCase<String>> = c
                .defs
                .iter()
                .map(|(l, _, _)| UniCase::new(l.clone()))
                .collect();
            for label in own {
                if !seen.insert(label) {
                    return true;
                }
            }
        }
        false
    }

    /// Parse `text[range]`, which starts at a checkpoint, with the line
    /// after it; references resolved from `defs`, and `footnotes` (defined
    /// elsewhere) put before it if it refers to any
    fn parse_region(
        &self,
        text: &str,
        range: Range<usize>,
        defs: &HashMap<UniCase<String>, (String, String)>,
        footnotes: &[String],
    ) -> Region {
        let body = &text[range.clone()];
        let mut source = String::new();
        if !footnotes.is_empty() && body.contains("[^") {
            for label in footnotes {
                source.push_str(&format!("[^{label}]: x\n\n"));
            }
            // A comment ends the definitions, so that an indented first
            // line (at the text's start) is not read as theirs
            source.push_str("<!-- -->\n\n");
        }
        let lead = source.len();
        source.push_str(body);
        // The line after, to see whether a block starts there
        let next = if range.end < text.len() {
            syntax::line_end(text, range.end)
        } else {
            range.end
        };
        source.push_str(&text[range.end..next]);
        let mut refs = |label: &str| defs.get(&UniCase::new(label.to_string())).cloned();
        // Front matter only at the text's start
        let (mut analysis, mut extras) = syntax::analyse_region_in(
            &source,
            (range.start == 0).then_some(lead),
            &mut refs,
            self.dialect,
        );
        let stop = lead + body.len();
        let last = range.end >= text.len();
        // Is the line after where a block starts with nothing open?
        let ends_clean = last
            || (extras.tops.contains(&stop)
                && !extras.held.iter().any(|h| h.start <= stop && stop < h.end));
        // Only what is in the range (at the text's end, what is at its very
        // end too: the closing blank line), at the text's offsets
        let inside = |at: usize| at >= lead && (at < stop || (last && at == stop));
        let first_container = analysis
            .containers
            .iter()
            .position(|c| inside(c.range.start))
            .unwrap_or(analysis.containers.len());
        analysis.leaves.retain(|l| inside(l.range.start));
        analysis.containers.retain(|c| inside(c.range.start));
        let shift = |at: usize| at - lead + range.start;
        for leaf in &mut analysis.leaves {
            shift_leaf(leaf, range.start as isize - lead as isize);
            for c in &mut leaf.containers {
                *c -= first_container;
            }
        }
        for c in &mut analysis.containers {
            shift_container(c, range.start as isize - lead as isize);
        }
        extras.tops.retain(|&t| inside(t));
        extras.open_rules.retain(|&t| inside(t));
        for t in extras.tops.iter_mut().chain(extras.open_rules.iter_mut()) {
            *t = shift(*t);
        }
        extras.defs.retain(|(at, ..)| inside(*at));
        for (at, ..) in &mut extras.defs {
            *at = shift(*at);
        }
        extras.refs.retain(|(at, _)| inside(*at));
        for (at, _) in &mut extras.refs {
            *at = shift(*at);
        }
        extras.footnotes.retain(|(at, _)| inside(*at));
        for (at, _) in &mut extras.footnotes {
            *at = shift(*at);
        }
        extras.held = extras
            .held
            .iter()
            .filter(|h| h.end > lead)
            .map(|h| shift(h.start.max(lead))..shift(h.end.min(stop.max(lead))))
            .collect();
        Region {
            analysis,
            extras,
            ends_clean,
        }
    }

    /// The region `range` of `text`, parsed, cut into chunks at its
    /// checkpoints; its leaves and containers start at these indices
    fn split(
        &mut self,
        text: &str,
        range: Range<usize>,
        region: &Region,
        revision: u64,
        leaf_start: usize,
        container_start: usize,
    ) -> Vec<Chunk> {
        let held = |at: usize| {
            region
                .extras
                .held
                .iter()
                .any(|h| h.start <= at && at < h.end)
        };
        let mut cuts: Vec<usize> = region
            .extras
            .tops
            .iter()
            .copied()
            .filter(|&t| t > range.start && t < range.end && is_checkpoint(text, t) && !held(t))
            .collect();
        cuts.dedup();
        let mut chunks = Vec::with_capacity(cuts.len() + 1);
        let mut starts = vec![range.start];
        starts.extend(cuts);
        let leaves = &region.analysis.leaves;
        let containers = &region.analysis.containers;
        let (mut l, mut c) = (0usize, 0usize);
        for (n, &start) in starts.iter().enumerate() {
            let end = starts.get(n + 1).copied().unwrap_or(range.end);
            let last = n + 1 == starts.len();
            let l_end = if last {
                leaves.len()
            } else {
                l + leaves[l..].partition_point(|leaf| leaf.range.start < end)
            };
            let c_end = if last {
                containers.len()
            } else {
                c + containers[c..].partition_point(|k| k.range.start < end)
            };
            let within = |at: usize| at >= start && (at < end || last);
            let chunk = Chunk {
                id: self.next_id,
                revision,
                start,
                leaf_start: leaf_start + l,
                leaves: l_end - l,
                container_start: container_start + c,
                containers: c_end - c,
                defs: Vec::new(),
                unknown_defs: false,
                refs: region
                    .extras
                    .refs
                    .iter()
                    .filter(|(at, _)| within(*at))
                    .map(|(_, l)| UniCase::new(l.clone()))
                    .collect(),
                footnotes: region
                    .extras
                    .footnotes
                    .iter()
                    .filter(|(at, _)| within(*at))
                    .map(|(_, l)| l.clone())
                    .collect(),
                footnote_refs: text[start..end].contains("[^"),
                rule: region.extras.open_rules.iter().any(|&r| within(r)),
            };
            let mut chunk = chunk;
            if starts.len() == 1 {
                chunk.defs = region
                    .extras
                    .defs
                    .iter()
                    .map(|(_, l, d, t)| (l.clone(), d.clone(), t.clone()))
                    .collect();
            } else {
                let (defs, all) = definitions(
                    text,
                    start..end,
                    &region.extras.defs,
                    chunk.rule,
                    self.dialect,
                );
                chunk.defs = defs;
                chunk.unknown_defs = !all;
            }
            self.next_id += 1;
            chunks.push(chunk);
            l = l_end;
            c = c_end;
        }
        chunks
    }

    /// The chunks' starts and ids, for tests
    #[cfg(test)]
    fn checkpoints(&self) -> Vec<usize> {
        self.chunks.iter().map(|c| c.start).collect()
    }

    /// Each chunk's range of `text` (the text the analysis is of), in
    /// order, if every one reads the same alone as in the text for a parse
    /// of other options than the analysis' (the preview's): none has a
    /// definition (a reference reaches across chunks), front matter, or a
    /// rule front matter's reading depends on. None otherwise
    pub fn plain_chunks(&self, text: &str) -> Option<Vec<Range<usize>>> {
        if text.contains("]:")
            || self.chunks.iter().any(|c| c.rule)
            || self
                .analysis
                .leaves
                .iter()
                .any(|l| l.kind == LeafKind::FrontMatter)
        {
            return None;
        }
        let mut out = Vec::with_capacity(self.chunks.len());
        for (i, c) in self.chunks.iter().enumerate() {
            let end = self.chunks.get(i + 1).map_or(text.len(), |n| n.start);
            out.push(c.start..end);
        }
        Some(out)
    }

    /// Each chunk's id and the revision it was parsed at, in order
    pub fn chunk_ids(&self) -> Vec<(u64, u64)> {
        self.chunks.iter().map(|c| (c.id, c.revision)).collect()
    }
}

/// The link reference definitions of `text[range]`, one chunk of a region
/// parsed as a whole, whose definitions are `found` (where, label,
/// destination, title), and whether they are all of them. pulldown-cmark
/// keeps only the first of a label's definitions, so a second, in this
/// chunk, is found by parsing the chunk alone, where it may have one. Not if
/// the chunk's reading depends on what comes after it (a `rule`): then
/// what was found, not known to be all
fn definitions(
    text: &str,
    range: Range<usize>,
    found: &[(usize, String, String, String)],
    rule: bool,
    dialect: Dialect,
) -> (Vec<(String, String, String)>, bool) {
    let mut defs: Vec<(String, String, String)> = found
        .iter()
        .filter(|(at, ..)| range.contains(at))
        .map(|(_, l, d, t)| (l.clone(), d.clone(), t.clone()))
        .collect();
    if !text[range.clone()].contains("]:") {
        return (defs, true);
    }
    if rule {
        return (defs, false);
    }
    let (_, extras) = syntax::analyse_region_in(
        &text[range.clone()],
        (range.start == 0).then_some(0),
        &mut |_| None,
        dialect,
    );
    for (_, label, dest, title) in extras.defs {
        if !defs
            .iter()
            .any(|(l, ..)| UniCase::new(l.as_str()) == UniCase::new(label.as_str()))
        {
            defs.push((label, dest, title));
        }
    }
    (defs, true)
}

/// The document-wide indexes, to see what an edit changed in them
struct Globals {
    defs: HashMap<UniCase<String>, (String, String)>,
    footnotes: HashSet<UniCase<String>>,
}

/// A line where a chunk may start, by its text: a line start after a blank
/// line, its first character not a space
fn is_checkpoint(text: &str, at: usize) -> bool {
    if at == 0 || at >= text.len() || text.as_bytes()[at - 1] != b'\n' {
        return false;
    }
    if text.as_bytes()[at].is_ascii_whitespace() {
        return false;
    }
    let before = syntax::line_start(text, at - 1);
    text[before..at - 1]
        .bytes()
        .all(|b| b == b' ' || b == b'\t' || b == b'\r')
}

/// A line closing front matter in `text`
fn has_closer(text: &str) -> bool {
    let bytes = text.as_bytes();
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .any(|at| syntax::is_metadata_closer(&bytes[at.min(bytes.len())..]))
}

/// The first line closing front matter starting after the line `from` is on
fn closer_after(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = syntax::line_end(text, from.min(text.len()));
    while at < text.len() {
        at += 1;
        if syntax::is_metadata_closer(&bytes[at..]) {
            return Some(at);
        }
        at = syntax::line_end(text, at);
    }
    None
}

/// Whether `new` (a region's analysis, its containers numbered from 0) has
/// the shape of `leaves` and `containers` of `old`: as many leaves, each of
/// the same kind (heading level, code language) in the same containers,
/// and the containers of the same kinds, depths and numbers
fn same_shape(
    old: &Analysis,
    leaves: Range<usize>,
    containers: Range<usize>,
    new: &Analysis,
) -> bool {
    use std::mem::discriminant;
    if leaves.len() != new.leaves.len() || containers.len() != new.containers.len() {
        return false;
    }
    fn kind(k: &LeafKind) -> (std::mem::Discriminant<LeafKind>, u8, &str) {
        match k {
            LeafKind::Heading(n) => (std::mem::discriminant(k), *n, ""),
            LeafKind::Code { lang } => (std::mem::discriminant(k), 0, lang.as_str()),
            k => (std::mem::discriminant(k), 0, ""),
        }
    }
    let leaves_alike = old.leaves[leaves].iter().zip(&new.leaves).all(|(a, b)| {
        kind(&a.kind) == kind(&b.kind)
            && a.containers.len() == b.containers.len()
            && a.containers
                .iter()
                .zip(&b.containers)
                .all(|(x, y)| x.checked_sub(containers.start) == Some(*y))
    });
    let containers_alike = old.containers[containers]
        .iter()
        .zip(&new.containers)
        .all(|(a, b)| {
            a.depth == b.depth
                && match (&a.kind, &b.kind) {
                    (
                        ContainerKind::Item {
                            number: n,
                            tight: t,
                            ..
                        },
                        ContainerKind::Item {
                            number: m,
                            tight: u,
                            ..
                        },
                    ) => n == m && t == u,
                    (x, y) => discriminant(x) == discriminant(y),
                }
        });
    leaves_alike && containers_alike
}

fn shift_range(r: &mut Range<usize>, by: isize) {
    r.start = (r.start as isize + by) as usize;
    r.end = (r.end as isize + by) as usize;
}

pub fn shift_leaf(leaf: &mut syntax::Leaf, by: isize) {
    if by == 0 {
        return;
    }
    shift_range(&mut leaf.range, by);
    for p in &mut leaf.prefixes {
        shift_range(p, by);
    }
    for m in &mut leaf.marks {
        shift_range(&mut m.range, by);
        shift_range(&mut m.reveal, by);
    }
    for i in &mut leaf.inlines {
        shift_range(&mut i.range, by);
    }
    if let LeafKind::Table(table) = &mut leaf.kind {
        for row in &mut table.rows {
            for cell in row {
                shift_range(cell, by);
            }
        }
    }
}

fn shift_container(c: &mut syntax::Container, by: isize) {
    if by == 0 {
        return;
    }
    shift_range(&mut c.range, by);
    if let ContainerKind::Item {
        first_leaf: Some(at),
        ..
    } = &mut c.kind
    {
        *at = (*at as isize + by) as usize;
    }
}

/// Replace `leaves` and `containers` of `analysis` with `new`'s (whose
/// containers are numbered from 0), moving everything after by `delta`
/// bytes
fn splice_analysis(
    analysis: &mut Analysis,
    leaves: Range<usize>,
    containers: Range<usize>,
    mut new: Analysis,
    delta: isize,
) {
    let dc = new.containers.len() as isize - containers.len() as isize;
    for leaf in &mut new.leaves {
        for c in &mut leaf.containers {
            *c += containers.start;
        }
    }
    let new_leaves = new.leaves.len();
    let new_containers = new.containers.len();
    analysis.leaves.splice(leaves.clone(), new.leaves);
    analysis
        .containers
        .splice(containers.clone(), new.containers);
    for leaf in &mut analysis.leaves[leaves.start + new_leaves..] {
        shift_leaf(leaf, delta);
        if dc != 0 {
            for c in &mut leaf.containers {
                *c = (*c as isize + dc) as usize;
            }
        }
    }
    for c in &mut analysis.containers[containers.start + new_containers..] {
        shift_container(c, delta);
    }
}

/// The differential test's random documents and edits, for tests here
/// and in the apps (Cerea's worker uses them)
#[doc(hidden)]
pub mod testing {
    use std::ops::Range;

    use crate::syntax;

    pub struct Rng(pub u64);

    impl Rng {
        pub fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
        pub fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % n.max(1) as u64) as usize
        }
    }

    /// Pieces of Markdown edits are made of: every structure that changes
    /// what follows it, and plain text
    pub const PIECES: &[&str] = &[
        "```",
        "```\n",
        "~~~\n",
        "\n```\n",
        "- ",
        "* ",
        "1. ",
        "2) ",
        "> ",
        ">",
        "\n",
        "\n\n",
        "  ",
        "    ",
        "\t",
        "| a | b |\n",
        "|---|---|\n",
        "|",
        "---",
        "\n---\n",
        "===",
        "\n===\n",
        "# ",
        "## ",
        "[x]: /url\n",
        "[x]",
        "[y]: <a b> \"t\"\n",
        "[y][]",
        "[^1]",
        "[^1]: note\n",
        "<!--",
        "-->",
        "<div>\n",
        "</div>\n",
        "<script>\n",
        "</script>\n",
        "$$\n",
        "$",
        "\\[",
        "\\]",
        "\\(",
        "\\)",
        "\\begin{align}\n",
        "\\end{align}\n",
        "**",
        "*",
        "_",
        "`",
        "==",
        "<u>",
        "</u>",
        "![i](a.png)",
        "...\n",
        "word",
        "Some text ",
        "x",
        "+++\n",
        "---\nk: v\n",
        "- [ ] ",
        "&amp;",
        "\\",
        "  \n",
        "é",
        "[ẞ]: /s\n",
        "[SS]",
        "\r\n",
        "<pre>\n",
        "</pre>\n",
        "- ---\n",
        "* ---\n",
        "> ---\n",
        "\n...\n",
        "[^Note]",
        "[^note]: n\n",
        "   ",
        "  - ",
        "\n    ",
        // Display formulas whose lines look like Markdown (Terminal PR 145)
        "\\begin{cases}\n",
        "\\end{cases}\n",
        "=\n",
        "- x\n",
        "\\]\n",
        "> $$\n",
    ];

    pub fn random_doc(rng: &mut Rng) -> String {
        let blocks = [
            "# Title\n\n",
            "Some text in a paragraph,\nwrapped onto a second line.\n\n",
            "- one\n- two\n  - nested\n\n",
            "1. first\n2. second\n\n",
            "> quoted\n> more\n\n",
            "```rust\nlet x = 1;\n```\n\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n",
            "Setext\n===\n\n",
            "---\n\n",
            "$$\nx^2\n$$\n\n",
            "A [link][x] and [y][] and [^1].\n\n",
            "[x]: /url\n\n",
            "[^1]: A footnote.\n\n",
            "<div>\nhtml\n</div>\n\n",
            "    indented code\n\n",
            "\\[\n\\int f\n\\]\n\n",
            "Text with $x$ and \\(y\\) maths.\n\n",
            "- [ ] task\n- [x] done\n\n",
            "![alt](img.png)\n\n",
            "Term ==marked== <u>under</u>.\n\n",
            "Text, then a formula:\n\\[\nx\n=\ny\n\\]\nAnd on.\n\n",
            "So:\n\\begin{cases}\na \\\\\n= b\n\\end{cases}\nhence.\n\n",
            "$$\n- a\n\n+ b\n$$\n\n",
            "> $$\n> a\n> =\n> b\n> $$\n\n",
            "- item\n\n  \\[\n  a\n  =\n  b\n  \\]\n\n",
        ];
        let mut s = String::new();
        if rng.below(6) == 0 {
            s.push_str("---\ntitle: T\n---\n\n");
        }
        for _ in 0..rng.below(30) + 2 {
            s.push_str(blocks[rng.below(blocks.len())]);
        }
        s
    }

    fn char_floor(text: &str, mut at: usize) -> usize {
        at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// One random edit: a piece typed somewhere (often at a line start),
    /// a range deleted, or a range replaced
    pub fn random_edit(rng: &mut Rng, text: &str) -> (Range<usize>, String) {
        let len = text.len();
        let mut at = char_floor(text, rng.below(len + 1));
        if rng.below(2) == 0 {
            at = syntax::line_start(text, at);
        }
        match rng.below(4) {
            0 | 1 => (at..at, PIECES[rng.below(PIECES.len())].to_string()),
            2 => {
                let end = char_floor(text, at + rng.below(12));
                (at..end.max(at), String::new())
            }
            _ => {
                let end = char_floor(text, at + rng.below(6));
                (at..end.max(at), PIECES[rng.below(PIECES.len())].to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    /// Edit `text` at `range` with `insert`, both ways, and check them
    fn check_edit(
        blocks: &mut Blocks,
        text: &mut String,
        range: Range<usize>,
        insert: &str,
        revision: u64,
        script: &[String],
    ) {
        let before = blocks.analysis.leaves.clone();
        let removed = text[range.clone()].to_string();
        text.replace_range(range.clone(), insert);
        let splices = blocks.edit(text, range.clone(), &removed, insert.len(), revision);
        let whole = syntax::analyse_in(text, blocks.dialect);
        if blocks.analysis != whole {
            let at = blocks
                .analysis
                .leaves
                .iter()
                .zip(&whole.leaves)
                .position(|(a, b)| a != b);
            panic!(
                "the incremental analysis differs from a whole parse after {} edits\nlast: {range:?} → {insert:?}\ntext: {text:?}\nfirst differing leaf: {at:?}\nincremental: {:#?}\nwhole: {:#?}\nscript: {script:#?}",
                script.len(),
                at.map(|i| &blocks.analysis.leaves[i]),
                at.map(|i| &whole.leaves[i]),
            );
        }
        // The splices account for the leaves outside them
        let mut mapped: Vec<Option<usize>> = (0..before.len()).map(Some).collect();
        for s in &splices {
            for m in &mut mapped {
                *m = m.and_then(|i| s.map(i));
            }
        }
        for (old, new) in mapped.iter().enumerate() {
            if let Some(new) = new {
                let (a, b) = (&before[old], &blocks.analysis.leaves[*new]);
                assert_eq!(
                    std::mem::discriminant(&a.kind),
                    std::mem::discriminant(&b.kind),
                    "leaf {old} → {new} kept by the splices {splices:?} changed kind"
                );
                assert_eq!(
                    a.range.len(),
                    b.range.len(),
                    "leaf {old} → {new} kept by the splices {splices:?} changed length"
                );
            }
        }
    }

    /// The differential test: random documents, random edit scripts
    /// (structural pieces typed at line starts — fences, list markers,
    /// tables, setext underlines, quotes, definitions, front matter — and
    /// deletions); after every edit the analysis kept up to date equals a
    /// parse of the whole text. `CEREA_FUZZ=<n>` runs more scripts
    #[test]
    fn differential() {
        let scripts: u64 = std::env::var("CEREA_FUZZ")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(300);
        for dialect in [Dialect::Document, Dialect::Chat] {
            for seed in 0..scripts {
                let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) + 1);
                let mut text = random_doc(&mut rng);
                let mut blocks = Blocks::new_in(&text, 0, dialect);
                assert_eq!(
                    blocks.analysis,
                    syntax::analyse_in(&text, dialect),
                    "{dialect:?} seed {seed}: the first parse"
                );
                let mut script = Vec::new();
                for step in 0..60 {
                    let (range, insert) = random_edit(&mut rng, &text);
                    script.push(format!(
                        "{dialect:?} seed {seed} step {step}: {range:?} → {insert:?}"
                    ));
                    check_edit(&mut blocks, &mut text, range, &insert, step + 1, &script);
                }
            }
        }
    }

    /// Streaming is an edit at the end: a random document appended to in
    /// random pieces is, after every piece, as a parse of the whole
    #[test]
    fn appending_is_as_a_whole_parse() {
        for seed in 0..200u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) + 7);
            let whole = random_doc(&mut rng);
            let mut text = String::new();
            let mut blocks = Blocks::new_in(&text, 0, Dialect::Chat);
            let mut script = Vec::new();
            let mut at = 0;
            while at < whole.len() {
                let mut end = (at + 1 + rng.below(40)).min(whole.len());
                while !whole.is_char_boundary(end) {
                    end += 1;
                }
                let end_now = text.len();
                script.push(format!("seed {seed}: + {:?}", &whole[at..end]));
                check_edit(
                    &mut blocks,
                    &mut text,
                    end_now..end_now,
                    &whole[at..end],
                    script.len() as u64,
                    &script,
                );
                at = end;
            }
        }
    }

    /// Typing a paragraph in a long document parses only around it
    #[test]
    fn a_keystroke_parses_its_block() {
        let mut text = String::from("# Many\n\n");
        for i in 0..500 {
            text.push_str(&format!("## Section {i}\n\nSome text in section {i}.\n\n"));
        }
        let mut blocks = Blocks::new(&text, 0);
        assert!(blocks.chunk_count() > 1000, "{}", blocks.chunk_count());
        let at = text.find("section 250.").unwrap() + 7;
        let ids = blocks.chunk_ids();
        for (n, c) in "abc".chars().enumerate() {
            let at = at + n;
            text.insert(at, c);
            let splices = blocks.edit(&text, at..at, "", 1, n as u64 + 1);
            assert!(blocks.parsed < 200, "parsed {} bytes", blocks.parsed);
            assert_eq!(splices.len(), 1);
            assert!(splices[0].leaves.len() <= 4, "{splices:?}");
        }
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        // The chunks far from the edit are the ones parsed at first
        let now = blocks.chunk_ids();
        assert_eq!(now[..400], ids[..400]);
        assert_eq!(now[now.len() - 400..], ids[ids.len() - 400..]);
    }

    /// Opening a fence parses on to where the state matches again: the
    /// text's end, if nothing closes it; closing it again, the same
    #[test]
    fn a_fence_parses_until_it_closes() {
        let mut text = String::new();
        for i in 0..50 {
            text.push_str(&format!("Paragraph {i}.\n\n"));
        }
        let mut blocks = Blocks::new(&text, 0);
        let at = text.find("Paragraph 10.").unwrap();
        text.insert_str(at, "```\n");
        blocks.edit(&text, at..at, "", 4, 1);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        assert!(
            blocks.parsed >= text.len() - at,
            "to the end: {}",
            blocks.parsed
        );
        // A closing fence further on: parsed to it, and the rest kept
        let close = text.find("Paragraph 20.").unwrap();
        text.insert_str(close, "```\n\n");
        blocks.edit(&text, close..close, "", 5, 2);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        let last = text.find("Paragraph 45.").unwrap();
        text.insert(last + 2, 'x');
        blocks.edit(&text, last + 2..last + 2, "", 1, 3);
        assert!(
            blocks.parsed < 100,
            "after the closed fence, a block: {}",
            blocks.parsed
        );
        assert_eq!(blocks.analysis, syntax::analyse(&text));
    }

    /// A reference and its definition in different chunks, the definition
    /// changed: the reference follows
    #[test]
    fn definitions_reach_across_chunks() {
        let mut text =
            String::from("A [link][x] here.\n\nMore.\n\n[x]: /one\n\nEnd [^n].\n\n[^n]: Note.\n");
        let mut blocks = Blocks::new(&text, 0);
        assert!(
            blocks.checkpoints().len() >= 4,
            "{:?}",
            blocks.checkpoints()
        );
        let at = text.find("/one").unwrap() + 1;
        text.replace_range(at..at + 3, "two");
        blocks.edit(&text, at..at + 3, "one", 3, 1);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        let link = blocks.analysis.leaves[0]
            .inlines
            .iter()
            .find(|i| matches!(i.kind, syntax::InlineKind::Link { .. }))
            .unwrap();
        assert_eq!(link.kind, syntax::InlineKind::Link { url: "/two".into() });
        // The footnote's definition taken away: its reference is text
        let def = text.find("[^n]: Note.").unwrap();
        text.replace_range(def..def + 1, "");
        blocks.edit(&text, def..def + 1, "[", 0, 2);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
    }

    /// Front matter only at the text's start: a `---` / `key: value` /
    /// `---` block typed in the middle, or an open rule there closed later,
    /// is read as CommonMark reads it, the same edit by edit as whole
    #[test]
    fn front_matter_typed_mid_document_is_not_front_matter() {
        let mut text = String::from("Intro.\n\nMore.\n\nEnd.\n");
        let mut blocks = Blocks::new(&text, 0);
        let at = text.find("More.").unwrap();
        let typed = "---\ntitle: x\n---\n\n";
        text.insert_str(at, typed);
        blocks.edit(&text, at..at, "", typed.len(), 1);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        assert!(
            blocks
                .analysis
                .leaves
                .iter()
                .all(|l| l.kind != LeafKind::FrontMatter),
            "{:?}",
            blocks.analysis.leaves
        );
        // At the start it is front matter, typed or taken away
        text.insert_str(0, typed);
        blocks.edit(&text, 0..0, "", typed.len(), 2);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        assert_eq!(blocks.analysis.leaves[0].kind, LeafKind::FrontMatter);
        assert_eq!(
            blocks
                .analysis
                .leaves
                .iter()
                .filter(|l| l.kind == LeafKind::FrontMatter)
                .count(),
            1
        );
        text.replace_range(0..typed.len(), "");
        blocks.edit(&text, 0..typed.len(), typed, 0, 3);
        assert_eq!(blocks.analysis, syntax::analyse(&text));
        assert!(
            blocks
                .analysis
                .leaves
                .iter()
                .all(|l| l.kind != LeafKind::FrontMatter)
        );
    }
}
