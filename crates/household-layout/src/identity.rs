// SPDX-License-Identifier: GPL-3.0-only

//! A box's stable identity, and the field or datum of the input that it
//! draws. An identity comes from what the box is (the title, the value
//! label of one series at one category), never from where it was drawn or
//! in what order, so it survives a new layout: a selection, an open editor
//! or a menu stays attached to the same thing.

/// A box's stable identity within one diagram or window.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub String);

impl Id {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity of a part of this box.
    pub fn child(&self, part: impl AsRef<str>) -> Self {
        Self(format!("{}/{}", self.0, part.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where in the input a box's content comes from, as its user names it
/// (for a chart, a path such as `/title/text`). The crate never reads it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Source(pub String);

impl Source {
    pub fn new(source: impl Into<String>) -> Self {
        Self(source.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
