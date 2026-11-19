// SPDX-License-Identifier: GPL-3.0-only
//! Domain-free panel intent and prepared lifecycle transitions.
//!
//! These values never allocate a window or content resource, solve geometry,
//! activate native focus, or perform layout effects. Applications preserve their
//! retained editor/scroll/domain state when applying the returned host changes.
//! IDs must not be recycled for unrelated live content or owners.
use crate::layout::{NavigationToken, RegionId};

macro_rules! stable_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(pub u64);
    };
}
stable_id!(
    PanelId,
    "Stable identity of a panel role, independent of its host."
);
stable_id!(
    PaneId,
    "Stable identity of a pane; compaction does not change its group."
);
stable_id!(
    ContentId,
    "Stable identity of retained content, supplied by its service."
);
stable_id!(
    OwnerId,
    "Stable logical workspace owner, independent of presentation."
);
stable_id!(
    PresentationHostId,
    "Stable app-supplied detached presentation host."
);

/// Revision of intended state. Accepted geometry does not change this revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelRevision(u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PanelCapabilities {
    pub pop_out: bool,
    pub promote: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PresentationHost {
    #[default]
    Embedded,
    Detached(PresentationHostId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneForm {
    Expanded,
    Collapsed,
}

/// Immutable intent supplied to ordinary component queries. Accepted form is
/// separate, so a compact trial/acceptance cannot rewrite persisted user intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneSnapshot {
    pub panel: PanelId,
    pub pane: PaneId,
    pub revision: PanelRevision,
    pub user_collapsed: bool,
}

#[derive(Debug)]
pub struct PaneState {
    snapshot: PaneSnapshot,
    accepted_form: Option<PaneForm>,
}
impl PaneState {
    pub fn new(panel: PanelId, pane: PaneId) -> Self {
        Self {
            snapshot: PaneSnapshot {
                panel,
                pane,
                revision: PanelRevision(0),
                user_collapsed: false,
            },
            accepted_form: None,
        }
    }
    pub fn snapshot(&self) -> PaneSnapshot {
        self.snapshot
    }
    pub fn accepted_form(&self) -> Option<PaneForm> {
        self.accepted_form
    }
    pub fn set_user_collapsed(&mut self, collapsed: bool) {
        // Repeated explicit commands still supersede queued reveals.
        self.snapshot.user_collapsed = collapsed;
        self.snapshot.revision.0 = self
            .snapshot
            .revision
            .0
            .checked_add(1)
            .expect("pane revision exhausted");
        self.accepted_form = None;
    }
    /// Only the existing engine's accepted form is reported here. An expanded
    /// result cannot override deliberate collapse; explicit reveal can.
    pub fn accept_form(
        &mut self,
        snapshot: PaneSnapshot,
        form: PaneForm,
    ) -> Result<(), TransitionError> {
        if snapshot != self.snapshot {
            return Err(TransitionError::Stale);
        }
        if snapshot.user_collapsed && form == PaneForm::Expanded {
            return Err(TransitionError::UserCollapsed);
        }
        self.accepted_form = Some(form);
        Ok(())
    }
}

/// A persistent panel role owns a root content identity. That root may itself
/// describe an arbitrary pane composition owned by the application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelSnapshot {
    pub panel: PanelId,
    pub content: ContentId,
    pub owner: OwnerId,
    pub revision: PanelRevision,
    pub host: PresentationHost,
    pub displayed: bool,
    pub owner_alive: bool,
    pub capabilities: PanelCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    ClosedOwner,
    Unsupported,
    Stale,
    Busy,
    WrongDestination,
    UserCollapsed,
    InvalidReplacement,
}

#[derive(Debug)]
pub struct PanelState {
    snapshot: PanelSnapshot,
    next_transition: u64,
    pending_unlink: Option<u64>,
}
impl PanelState {
    pub fn new(
        panel: PanelId,
        content: ContentId,
        owner: OwnerId,
        capabilities: PanelCapabilities,
    ) -> Self {
        Self {
            snapshot: PanelSnapshot {
                panel,
                content,
                owner,
                revision: PanelRevision(0),
                host: PresentationHost::Embedded,
                displayed: false,
                owner_alive: true,
                capabilities,
            },
            next_transition: 0,
            pending_unlink: None,
        }
    }
    pub fn snapshot(&self) -> PanelSnapshot {
        self.snapshot
    }
    fn check_alive(&self) -> Result<(), TransitionError> {
        if self.snapshot.owner_alive {
            Ok(())
        } else {
            Err(TransitionError::ClosedOwner)
        }
    }
    fn changed(&mut self) {
        self.snapshot.revision.0 = self
            .snapshot
            .revision
            .0
            .checked_add(1)
            .expect("panel revision exhausted");
        self.pending_unlink = None;
    }
    /// Both the persistent opener and any local closer use this state. A hidden
    /// detached panel remembers its host; reopening is not return-to-parent.
    pub fn set_displayed(&mut self, displayed: bool) -> Result<(), TransitionError> {
        self.check_alive()?;
        // A close while a hidden panel has a pending reveal must cancel it,
        // even though the stored visibility already equals the command.
        self.snapshot.displayed = displayed;
        self.changed();
        Ok(())
    }
    /// Commit an already prepared host. The caller must preserve retained native
    /// state and apply rehosting only after this revision check succeeds.
    pub fn pop_out(
        &mut self,
        expected: PanelSnapshot,
        host: PresentationHostId,
    ) -> Result<(), TransitionError> {
        self.check_alive()?;
        if expected != self.snapshot {
            return Err(TransitionError::Stale);
        }
        if !self.snapshot.capabilities.pop_out {
            return Err(TransitionError::Unsupported);
        }
        self.snapshot.host = PresentationHost::Detached(host);
        self.snapshot.displayed = true;
        self.changed();
        Ok(())
    }
    pub fn return_to_parent(&mut self) -> Result<(), TransitionError> {
        self.check_alive()?;
        self.snapshot.host = PresentationHost::Embedded;
        self.changed();
        Ok(())
    }
    /// Apply the owner's close policy. Content is not destroyed here. This
    /// terminal state invalidates every pending operation, including unlink.
    pub fn close_owner(&mut self) {
        if self.snapshot.owner_alive {
            self.snapshot.owner_alive = false;
            self.snapshot.displayed = false;
            self.changed();
        }
    }
    /// `navigation` must come from the shared request for `destination.section`.
    /// The shared token intentionally owns cancellation, not a second panel
    /// navigation or focus registry. Preparing has no visibility effects.
    pub fn prepare_reveal(
        &self,
        pane: &PaneState,
        destination: RevealDestination,
        navigation: NavigationToken,
    ) -> Result<PreparedReveal, TransitionError> {
        self.check_alive()?;
        if destination.panel != self.snapshot.panel
            || destination.content != self.snapshot.content
            || destination.pane != pane.snapshot.pane
            || pane.snapshot.panel != self.snapshot.panel
        {
            return Err(TransitionError::WrongDestination);
        }
        if !navigation.is_current() {
            return Err(TransitionError::Stale);
        }
        Ok(PreparedReveal {
            panel: self.snapshot,
            pane: pane.snapshot,
            destination,
            navigation,
        })
    }
    /// Open/expand intent together, then ask the app to select/reveal the target.
    /// The effect remains cancellable; it does not assert accepted visibility or
    /// establish another focus/interaction owner.
    pub fn commit_reveal(
        &mut self,
        pane: &mut PaneState,
        prepared: PreparedReveal,
    ) -> Result<RevealEffect, TransitionError> {
        self.check_alive()?;
        if prepared.panel != self.snapshot
            || prepared.pane != pane.snapshot
            || !prepared.navigation.is_current()
        {
            return Err(TransitionError::Stale);
        }
        self.snapshot.displayed = true;
        self.changed();
        pane.set_user_collapsed(false);
        Ok(RevealEffect {
            destination: prepared.destination,
            host: self.snapshot.host,
            panel: self.snapshot,
            pane: pane.snapshot,
            navigation: prepared.navigation,
        })
    }
    /// Reserve one promotion. The application prepares both resources before
    /// calling commit_unlink. No resource is allocated by this reservation.
    pub fn begin_unlink(&mut self) -> Result<UnlinkTicket, TransitionError> {
        self.check_alive()?;
        if !self.snapshot.capabilities.promote {
            return Err(TransitionError::Unsupported);
        }
        if self.pending_unlink.is_some() {
            return Err(TransitionError::Busy);
        }
        self.next_transition = self
            .next_transition
            .checked_add(1)
            .expect("transition sequence exhausted");
        self.pending_unlink = Some(self.next_transition);
        Ok(UnlinkTicket {
            panel: self.snapshot,
            sequence: self.next_transition,
        })
    }
    pub fn unlink_is_current(&self, ticket: &UnlinkTicket) -> bool {
        self.snapshot.owner_alive
            && ticket.panel == self.snapshot
            && self.pending_unlink == Some(ticket.sequence)
    }
    /// Cancel/fail preparation without changing content or ownership. Resources
    /// already obtained remain the caller's responsibility to release.
    pub fn cancel_unlink(&mut self, ticket: UnlinkTicket) -> Result<(), TransitionError> {
        if !self.unlink_is_current(&ticket) {
            return Err(TransitionError::Stale);
        }
        self.pending_unlink = None;
        Ok(())
    }
    /// The app guarantees these prepared resources can be committed together
    /// without another fallible creation step. On rejection both are returned
    /// untouched, including after owner closure. Successful payloads are moved,
    /// never cloned. The app commits its service associations and host transfer
    /// as the same transaction; there is no generic native-resource rollback.
    pub fn commit_unlink<D, R>(
        &mut self,
        ticket: UnlinkTicket,
        prepared: PreparedPromotion<D, R>,
    ) -> Result<PromotionCommit<D, R>, RejectedPromotion<D, R>> {
        if !self.unlink_is_current(&ticket) {
            return Err(RejectedPromotion {
                reason: TransitionError::Stale,
                prepared,
            });
        }
        if prepared.replacement.content == self.snapshot.content
            || prepared.destination.owner == self.snapshot.owner
            || prepared.destination.owner == prepared.replacement.owner
            || prepared.replacement.owner != self.snapshot.owner
        {
            self.pending_unlink = None;
            return Err(RejectedPromotion {
                reason: TransitionError::InvalidReplacement,
                prepared,
            });
        }
        let promoted = PromotedContent {
            content: self.snapshot.content,
            owner: prepared.destination.owner,
            host: prepared.destination.host,
        };
        self.snapshot.content = prepared.replacement.content;
        self.snapshot.host = prepared.replacement.host;
        self.snapshot.displayed = prepared.replacement.displayed;
        self.snapshot.capabilities = prepared.replacement.capabilities;
        self.changed();
        Ok(PromotionCommit {
            promoted,
            destination: prepared.destination.resource,
            replacement: prepared.replacement.resource,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevealDestination {
    pub panel: PanelId,
    pub pane: PaneId,
    pub content: ContentId,
    pub section: RegionId,
    pub target: Option<RegionId>,
}

#[derive(Debug)]
pub struct PreparedReveal {
    panel: PanelSnapshot,
    pane: PaneSnapshot,
    destination: RevealDestination,
    navigation: NavigationToken,
}

#[derive(Debug)]
pub struct RevealEffect {
    pub destination: RevealDestination,
    pub host: PresentationHost,
    panel: PanelSnapshot,
    pane: PaneSnapshot,
    navigation: NavigationToken,
}
impl RevealEffect {
    /// Recheck immediately before queued selection, scrolling or native host
    /// activation. Visibility/current-section reporting still comes from the
    /// shared navigation state and accepted geometry, never this effect.
    pub fn is_current(&self, panel: &PanelState, pane: &PaneState) -> bool {
        self.panel == panel.snapshot && self.pane == pane.snapshot && self.navigation.is_current()
    }
}

/// Deliberately not Clone: a preparation reservation has one completion path.
#[derive(Debug)]
pub struct UnlinkTicket {
    panel: PanelSnapshot,
    sequence: u64,
}

#[derive(Debug)]
pub struct PreparedDestination<D> {
    pub owner: OwnerId,
    pub host: PresentationHost,
    pub resource: D,
}

#[derive(Debug)]
pub struct PreparedReplacement<R> {
    pub content: ContentId,
    pub owner: OwnerId,
    pub host: PresentationHost,
    pub displayed: bool,
    pub capabilities: PanelCapabilities,
    pub resource: R,
}

#[derive(Debug)]
pub struct PreparedPromotion<D, R> {
    pub destination: PreparedDestination<D>,
    pub replacement: PreparedReplacement<R>,
}

#[derive(Debug)]
pub struct PromotedContent {
    pub content: ContentId,
    pub owner: OwnerId,
    pub host: PresentationHost,
}

#[derive(Debug)]
pub struct PromotionCommit<D, R> {
    /// The user's view follows this content in its independent destination.
    pub promoted: PromotedContent,
    pub destination: D,
    pub replacement: R,
}

#[derive(Debug)]
pub struct RejectedPromotion<D, R> {
    pub reason: TransitionError,
    /// Caller releases both unused prepared resources, even on stale completion.
    pub prepared: PreparedPromotion<D, R>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::NavigationState;

    fn panel() -> PanelState {
        PanelState::new(
            PanelId(1),
            ContentId(2),
            OwnerId(3),
            PanelCapabilities {
                pop_out: true,
                promote: true,
            },
        )
    }
    fn pane() -> PaneState {
        PaneState::new(PanelId(1), PaneId(4))
    }
    fn destination() -> RevealDestination {
        RevealDestination {
            panel: PanelId(1),
            pane: PaneId(4),
            content: ContentId(2),
            section: RegionId(5),
            target: Some(RegionId(6)),
        }
    }
    fn resources() -> PreparedPromotion<String, String> {
        PreparedPromotion {
            destination: PreparedDestination {
                owner: OwnerId(10),
                host: PresentationHost::Detached(PresentationHostId(11)),
                resource: "destination".into(),
            },
            replacement: PreparedReplacement {
                content: ContentId(12),
                owner: OwnerId(3),
                host: PresentationHost::Embedded,
                displayed: false,
                capabilities: PanelCapabilities::default(),
                resource: "replacement".into(),
            },
        }
    }
    #[test]
    fn accepted_compaction_never_rewrites_user_collapse() {
        let mut pane = pane();
        let initial = pane.snapshot();
        pane.accept_form(initial, PaneForm::Collapsed).unwrap();
        assert!(!pane.snapshot().user_collapsed);
        pane.accept_form(initial, PaneForm::Expanded).unwrap();
        pane.set_user_collapsed(true);
        assert_eq!(
            pane.accept_form(initial, PaneForm::Expanded),
            Err(TransitionError::Stale)
        );
        assert_eq!(
            pane.accept_form(pane.snapshot(), PaneForm::Expanded),
            Err(TransitionError::UserCollapsed)
        );
        pane.accept_form(pane.snapshot(), PaneForm::Collapsed)
            .unwrap();
        assert!(pane.snapshot().user_collapsed);
    }
    #[test]
    fn hide_reopen_remembers_host_without_changing_owner_or_content() {
        let mut panel = panel();
        panel
            .pop_out(panel.snapshot(), PresentationHostId(9))
            .unwrap();
        panel.set_displayed(false).unwrap();
        panel.set_displayed(true).unwrap();
        assert_eq!(
            panel.snapshot().host,
            PresentationHost::Detached(PresentationHostId(9))
        );
        assert_eq!(panel.snapshot().owner, OwnerId(3));
        assert_eq!(panel.snapshot().content, ContentId(2));
        panel.return_to_parent().unwrap();
        assert_eq!(panel.snapshot().host, PresentationHost::Embedded);
    }
    #[test]
    fn cancelled_or_stale_reveal_does_not_open_or_expand() {
        let mut panel = panel();
        let mut pane = pane();
        pane.set_user_collapsed(true);
        let mut navigation = NavigationState::default();
        let generation = navigation.request(RegionId(5));
        let reveal = panel
            .prepare_reveal(&pane, destination(), navigation.token(generation))
            .unwrap();
        navigation.browse();
        assert!(matches!(
            panel.commit_reveal(&mut pane, reveal),
            Err(TransitionError::Stale)
        ));
        assert!(!panel.snapshot().displayed);
        assert!(pane.snapshot().user_collapsed);
        let generation = navigation.request(RegionId(5));
        let reveal = panel
            .prepare_reveal(&pane, destination(), navigation.token(generation))
            .unwrap();
        panel
            .pop_out(panel.snapshot(), PresentationHostId(9))
            .unwrap();
        assert!(matches!(
            panel.commit_reveal(&mut pane, reveal),
            Err(TransitionError::Stale)
        ));
    }
    #[test]
    fn repeated_explicit_close_cancels_reveal_while_still_hidden() {
        let mut panel = panel();
        let mut pane = pane();
        let mut navigation = NavigationState::default();
        let generation = navigation.request(RegionId(5));
        let reveal = panel
            .prepare_reveal(&pane, destination(), navigation.token(generation))
            .unwrap();
        panel.set_displayed(false).unwrap();
        assert!(matches!(
            panel.commit_reveal(&mut pane, reveal),
            Err(TransitionError::Stale)
        ));
        assert!(!panel.snapshot().displayed);
    }
    #[test]
    fn reveal_opens_and_expands_but_queued_effect_remains_cancellable() {
        let mut panel = panel();
        let mut pane = pane();
        pane.set_user_collapsed(true);
        let mut navigation = NavigationState::default();
        let generation = navigation.request(RegionId(5));
        let reveal = panel
            .prepare_reveal(&pane, destination(), navigation.token(generation))
            .unwrap();
        let effect = panel.commit_reveal(&mut pane, reveal).unwrap();
        assert!(panel.snapshot().displayed);
        assert!(!pane.snapshot().user_collapsed);
        assert!(effect.is_current(&panel, &pane));
        navigation.request(RegionId(99));
        assert!(!effect.is_current(&panel, &pane));
    }
    #[test]
    fn failed_preparation_and_duplicate_reservation_preserve_original() {
        let mut panel = panel();
        let original = panel.snapshot();
        let ticket = panel.begin_unlink().unwrap();
        assert!(matches!(panel.begin_unlink(), Err(TransitionError::Busy)));
        panel.cancel_unlink(ticket).unwrap();
        assert_eq!(panel.snapshot(), original);
        assert!(panel.begin_unlink().is_ok());
    }
    #[test]
    fn promotion_transfers_identity_and_installs_fresh_content() {
        let mut panel = panel();
        let ticket = panel.begin_unlink().unwrap();
        let commit = panel.commit_unlink(ticket, resources()).unwrap();
        assert_eq!(commit.promoted.content, ContentId(2));
        assert_eq!(commit.promoted.owner, OwnerId(10));
        assert_eq!(commit.destination, "destination");
        assert_eq!(commit.replacement, "replacement");
        assert_eq!(panel.snapshot().content, ContentId(12));
        assert_eq!(panel.snapshot().owner, OwnerId(3));
        assert!(!panel.snapshot().displayed);
    }
    #[test]
    fn stale_completion_returns_both_resources_without_resurrecting_owner() {
        let mut panel = panel();
        let ticket = panel.begin_unlink().unwrap();
        panel.close_owner();
        let closed = panel.snapshot();
        let rejected = panel.commit_unlink(ticket, resources()).unwrap_err();
        assert_eq!(rejected.reason, TransitionError::Stale);
        assert_eq!(rejected.prepared.destination.resource, "destination");
        assert_eq!(rejected.prepared.replacement.resource, "replacement");
        assert_eq!(panel.snapshot(), closed);
        assert_eq!(panel.set_displayed(true), Err(TransitionError::ClosedOwner));
    }
    #[test]
    fn superseded_preparation_cannot_cancel_or_commit_a_newer_reservation() {
        let mut panel = panel();
        let stale = panel.begin_unlink().unwrap();
        panel.set_displayed(false).unwrap();
        let current = panel.begin_unlink().unwrap();
        assert_eq!(panel.cancel_unlink(stale), Err(TransitionError::Stale));
        assert!(panel.unlink_is_current(&current));
        panel.commit_unlink(current, resources()).unwrap();
    }
    #[test]
    fn duplicate_completion_cannot_replace_content_twice() {
        let mut panel = panel();
        let ticket = panel.begin_unlink().unwrap();
        // Simulate a duplicated callback at the adapter boundary.
        let replay = UnlinkTicket {
            panel: ticket.panel,
            sequence: ticket.sequence,
        };
        panel.commit_unlink(ticket, resources()).unwrap();
        let committed = panel.snapshot();
        let rejected = panel.commit_unlink(replay, resources()).unwrap_err();
        assert_eq!(rejected.reason, TransitionError::Stale);
        assert_eq!(panel.snapshot(), committed);
        assert_eq!(rejected.prepared.replacement.resource, "replacement");
    }
    #[test]
    fn invalid_replacement_cannot_duplicate_original_content() {
        let mut panel = panel();
        let original = panel.snapshot();
        let ticket = panel.begin_unlink().unwrap();
        let mut prepared = resources();
        prepared.replacement.content = original.content;
        let rejected = panel.commit_unlink(ticket, prepared).unwrap_err();
        assert_eq!(rejected.reason, TransitionError::InvalidReplacement);
        assert_eq!(panel.snapshot(), original);
        assert!(panel.begin_unlink().is_ok());
    }
    #[test]
    fn popout_and_promotion_are_independent_capabilities() {
        let mut panel = PanelState::new(
            PanelId(1),
            ContentId(2),
            OwnerId(3),
            PanelCapabilities {
                pop_out: true,
                promote: false,
            },
        );
        panel
            .pop_out(panel.snapshot(), PresentationHostId(9))
            .unwrap();
        assert!(matches!(
            panel.begin_unlink(),
            Err(TransitionError::Unsupported)
        ));
        let snapshot = panel.snapshot();
        panel.set_displayed(false).unwrap();
        assert_eq!(
            panel.pop_out(snapshot, PresentationHostId(8)),
            Err(TransitionError::Stale)
        );
    }
}
