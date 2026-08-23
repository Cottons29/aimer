use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use smallvec::SmallVec;

use crate::base::BuildContext;
use crate::components::element::{
    Element, ElementId, complete_generated_tree_reconciliation, identities_are_compatible,
    structural_children,
};

/// Describes how one compatible element pair was selected during planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationMatchKind {
    /// The old and candidate root elements are compatible.
    Root,
    /// Siblings were paired by an equal reconciliation key.
    Keyed,
    /// Unkeyed siblings were paired at the same position.
    Positional,
}

/// One immutable old-to-candidate element match.
pub struct ReconciliationMatch<'a> {
    old: &'a dyn Element,
    new: &'a dyn Element,
    kind: ReconciliationMatchKind,
}

impl ReconciliationMatch<'_> {
    /// Returns how this pair was selected.
    #[inline]
    pub const fn kind(&self) -> ReconciliationMatchKind {
        self.kind
    }

    /// Returns the logical identity that commit transfers from the old node.
    #[inline]
    pub fn old_id(&self) -> ElementId {
        self.old.id()
    }

    /// Returns the candidate node's identity before commit.
    #[inline]
    pub fn candidate_id(&self) -> ElementId {
        self.new.id()
    }
}

/// A side-effect-free structural reconciliation plan.
///
/// Planning walks the old and disconnected candidate trees without adopting
/// runtime state, transferring identities, changing focus, or advancing the
/// installed-tree generation. Keyed siblings match by key regardless of order;
/// unkeyed siblings match only at the same position.
pub struct ReconciliationPlan<'a> {
    old_root: &'a dyn Element,
    new_root: &'a dyn Element,
    matches: Vec<ReconciliationMatch<'a>>,
}

impl<'a> ReconciliationPlan<'a> {
    /// Returns the immutable matches in parent-before-child order.
    #[inline]
    pub fn matches(&self) -> &[ReconciliationMatch<'a>] {
        &self.matches
    }

    /// Revalidates every match before any commit-time mutation occurs.
    pub fn validate(&self) -> Result<(), ReconciliationPlanError> {
        let mut old_nodes = HashSet::with_capacity(self.matches.len());
        let mut new_nodes = HashSet::with_capacity(self.matches.len());
        for (index, element_match) in self.matches.iter().enumerate() {
            if !identities_are_compatible(element_match.old, element_match.new) {
                return Err(ReconciliationPlanError::IncompatibleMatch { index });
            }
            let old_pointer = data_pointer(element_match.old);
            if !old_nodes.insert(old_pointer) {
                return Err(ReconciliationPlanError::DuplicateOldNode { index });
            }
            let new_pointer = data_pointer(element_match.new);
            if !new_nodes.insert(new_pointer) {
                return Err(ReconciliationPlanError::DuplicateCandidateNode { index });
            }
            match element_match.kind {
                ReconciliationMatchKind::Root if index == 0 => {}
                ReconciliationMatchKind::Root => {
                    return Err(ReconciliationPlanError::UnexpectedRootMatch { index });
                }
                ReconciliationMatchKind::Keyed
                    if element_match.old.reconciliation_key().is_some()
                        && element_match.old.reconciliation_key()
                            == element_match.new.reconciliation_key() => {}
                ReconciliationMatchKind::Positional
                    if element_match.old.reconciliation_key().is_none()
                        && element_match.new.reconciliation_key().is_none() => {}
                _ => return Err(ReconciliationPlanError::InvalidMatchKind { index }),
            }
        }
        Ok(())
    }

    /// Carries runtime state and commits this candidate at the current safe point.
    ///
    /// Validation completes before the first mutation. The established Aimer
    /// state carry then runs once. Because state carry can replace retained
    /// children, identity matches are resolved again against the resulting
    /// candidate before focus cleanup and tree-generation advancement.
    pub fn commit(self, ctx: &BuildContext) -> Result<(), ReconciliationPlanError> {
        self.validate()?;
        let old_root = self.old_root;
        let new_root = self.new_root;
        drop(self);
        crate::widget::stateful::carry_child_state(old_root, new_root, ctx);
        plan_element_reconciliation(old_root, new_root).apply_identities();
        complete_generated_tree_reconciliation(old_root, new_root);
        Ok(())
    }

    pub(crate) fn commit_generated_tree(self) -> Result<(), ReconciliationPlanError> {
        self.validate()?;
        self.apply_identities();
        complete_generated_tree_reconciliation(self.old_root, self.new_root);
        Ok(())
    }

    pub(crate) fn apply_identities(&self) {
        for element_match in &self.matches {
            if let Some(old_id) = element_match.old.element_id() {
                element_match.new.set_element_id(old_id);
            }
        }
    }
}


/// A structural plan invariant changed between planning and commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationPlanError {
    /// A planned pair no longer has compatible type, name, and key identity.
    IncompatibleMatch { index: usize },
    /// One old node was assigned to more than one candidate node.
    DuplicateOldNode { index: usize },
    /// One candidate node was assigned from more than one old node.
    DuplicateCandidateNode { index: usize },
    /// A root match appeared anywhere except the first plan entry.
    UnexpectedRootMatch { index: usize },
    /// A match classification disagrees with the pair's keys.
    InvalidMatchKind { index: usize },
}

impl fmt::Display for ReconciliationPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleMatch { index } => {
                write!(formatter, "reconciliation match {index} became incompatible")
            }
            Self::DuplicateOldNode { index } => {
                write!(formatter, "reconciliation match {index} reuses an old node")
            }
            Self::DuplicateCandidateNode { index } => write!(
                formatter,
                "reconciliation match {index} reuses a candidate node"
            ),
            Self::UnexpectedRootMatch { index } => {
                write!(formatter, "reconciliation match {index} is an unexpected root")
            }
            Self::InvalidMatchKind { index } => {
                write!(formatter, "reconciliation match {index} has an invalid kind")
            }
        }
    }
}

impl Error for ReconciliationPlanError {}

/// Computes compatible old-to-candidate matches without mutating either tree.
pub fn plan_element_reconciliation<'a>(
    old: &'a dyn Element,
    new: &'a dyn Element,
) -> ReconciliationPlan<'a> {
    let mut matches = Vec::new();
    if identities_are_compatible(old, new) {
        let mut visited_pairs = HashSet::new();
        collect_matches(
            old,
            new,
            ReconciliationMatchKind::Root,
            &mut matches,
            &mut visited_pairs,
        );
    }
    ReconciliationPlan {
        old_root: old,
        new_root: new,
        matches,
    }
}

fn collect_matches<'a>(
    old: &'a dyn Element,
    new: &'a dyn Element,
    kind: ReconciliationMatchKind,
    matches: &mut Vec<ReconciliationMatch<'a>>,
    visited_pairs: &mut HashSet<(*const (), *const ())>,
) {
    if !visited_pairs.insert((data_pointer(old), data_pointer(new))) {
        return;
    }
    matches.push(ReconciliationMatch { old, new, kind });
    let old_children = structural_children(old);
    let new_children = structural_children(new);
    let mut claimed = SmallVec::<[bool; 8]>::from_elem(false, old_children.len());

    for (new_index, new_child) in new_children.iter().copied().enumerate() {
        let selected = if let Some(new_key) = new_child.reconciliation_key() {
            old_children
                .iter()
                .enumerate()
                .position(|(old_index, old_child)| {
                    !claimed[old_index]
                        && old_child.reconciliation_key() == Some(new_key)
                        && identities_are_compatible(*old_child, new_child)
                })
                .map(|old_index| (old_index, ReconciliationMatchKind::Keyed))
        } else {
            old_children.get(new_index).and_then(|old_child| {
                (!claimed[new_index]
                    && old_child.reconciliation_key().is_none()
                    && identities_are_compatible(*old_child, new_child))
                .then_some((new_index, ReconciliationMatchKind::Positional))
            })
        };

        if let Some((old_index, kind)) = selected {
            claimed[old_index] = true;
            collect_matches(
                old_children[old_index],
                new_child,
                kind,
                matches,
                visited_pairs,
            );
        }
    }
}

#[inline]
fn data_pointer(element: &dyn Element) -> *const () {
    element as *const dyn Element as *const ()
}