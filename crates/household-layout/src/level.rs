// SPDX-License-Identifier: GPL-3.0-only

//! The four compaction levels. A level ordered from above is a ceiling:
//! the most compromise a parent permits its child to make. The names and
//! their order are household-widgets'.

/// The most compromise a parent permits its content to make.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Level {
    /// The content as given, every distance at or above its ideal.
    #[default]
    Preferred,
    /// Re-arrangement that keeps every word and every ideal distance.
    Mild,
    /// Squeezing below an ideal, never below the least distance; flagged.
    Strong,
    /// Everything the box's family permits, and no more: never hiding
    /// what is essential.
    AllPermitted,
}

impl Level {
    /// The levels in order of increasing compromise.
    pub const ALL: [Self; 4] = [
        Self::Preferred,
        Self::Mild,
        Self::Strong,
        Self::AllPermitted,
    ];

    /// The next level of compromise, if there is one.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Preferred => Some(Self::Mild),
            Self::Mild => Some(Self::Strong),
            Self::Strong => Some(Self::AllPermitted),
            Self::AllPermitted => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_by_compromise() {
        assert!(Level::Preferred < Level::Mild);
        assert!(Level::Strong < Level::AllPermitted);
        assert_eq!(Level::ALL.map(Level::next).last().copied().flatten(), None);
    }
}
