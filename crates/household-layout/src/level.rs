// SPDX-License-Identifier: GPL-3.0-only

//! How much a part may compromise, and how it arranges its immediate parts.

/// The maximum compromise a parent permits its content to make.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Level {
    #[default]
    Preferred,
    Mild,
    Strong,
    AllPermitted,
}

impl Level {
    /// Representations in preference order.
    pub const ALL: [Self; 4] = [
        Self::Preferred,
        Self::Mild,
        Self::Strong,
        Self::AllPermitted,
    ];
}

/// An arrangement applies only to immediate children, which remain atomic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrangement {
    Row,
    Column,
    Wrap,
}
