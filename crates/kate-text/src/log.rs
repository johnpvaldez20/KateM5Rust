// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Applied transactions, oldest first.
//!
//! Entry `i` produced revision `first_revision + i + 1`.
//! A lock keeps every transaction after that revision.
//! With no lock, the log keeps the last transaction only.

use std::collections::{BTreeMap, VecDeque};

use crate::anchor::StaleAnchor;
use crate::edit::Transaction;
use crate::offset::Revision;

/// Applied transactions that anchors can still replay.
pub struct EditLog {
    entries: VecDeque<Transaction>,
    first_revision: Revision,
    locks: BTreeMap<Revision, u32>,
}

impl EditLog {
    /// Creates an empty log whose current revision is `at`.
    pub fn new(at: Revision) -> Self {
        Self {
            entries: VecDeque::new(),
            first_revision: at,
            locks: BTreeMap::new(),
        }
    }

    /// Appends `tx` when `produced` is the next revision, then prunes.
    ///
    /// A `produced` value that is not the next revision leaves the log unchanged.
    pub fn push(&mut self, tx: Transaction, produced: Revision) {
        let Some(expected) = self.tip().0.checked_add(1) else {
            return;
        };
        if produced.0 != expected {
            return;
        }
        self.entries.push_back(tx);
        self.prune();
    }

    /// Counts one lock on `revision`.
    ///
    /// A revision outside the kept range does not change the log.
    /// A lock does not drop history. The next `push` or `unlock` prunes.
    pub fn lock(&mut self, revision: Revision) {
        if !self.contains(revision) {
            return;
        }
        let count = self.locks.get(&revision).copied().unwrap_or(0);
        self.locks.insert(revision, count.saturating_add(1));
    }

    /// Drops one lock on `revision`, then prunes.
    ///
    /// A revision that is not locked does not change the log.
    pub fn unlock(&mut self, revision: Revision) {
        let Some(count) = self.locks.get(&revision).copied() else {
            return;
        };
        if count > 1 {
            self.locks.insert(revision, count - 1);
            return;
        }
        self.locks.remove(&revision);
        self.prune();
    }

    /// Returns the oldest revision that `entries_since` accepts.
    pub fn oldest_available(&self) -> Revision {
        self.first_revision
    }

    /// Returns transactions that transform revisions after `revision`.
    ///
    /// The result is `Err` when `revision` is outside the kept range.
    pub fn entries_since(
        &self,
        revision: Revision,
    ) -> Result<impl Iterator<Item = &Transaction> + '_, StaleAnchor> {
        let start = self.entry_index(revision)?;
        Ok(self.entries.range(start..))
    }

    fn tip(&self) -> Revision {
        let len = u64::try_from(self.entries.len()).unwrap_or(u64::MAX);
        Revision(self.first_revision.0.saturating_add(len))
    }

    fn contains(&self, revision: Revision) -> bool {
        revision >= self.first_revision && revision <= self.tip()
    }

    fn entry_index(&self, revision: Revision) -> Result<usize, StaleAnchor> {
        let oldest = self.oldest_available();
        if revision < oldest || revision > self.tip() {
            return Err(StaleAnchor {
                requested: revision,
                oldest_available: oldest,
            });
        }
        let distance = revision.0 - oldest.0;
        match usize::try_from(distance) {
            Ok(start) if start <= self.entries.len() => Ok(start),
            _ => Err(StaleAnchor {
                requested: revision,
                oldest_available: oldest,
            }),
        }
    }

    fn oldest_lock(&self) -> Option<Revision> {
        let tip = self.tip();
        self.locks
            .keys()
            .copied()
            .find(|revision| *revision >= self.first_revision && *revision <= tip)
    }

    fn retention_floor(&self) -> Revision {
        if let Some(locked) = self.oldest_lock() {
            return locked;
        }
        let tip = self.tip();
        if self.entries.is_empty() || tip.0 == 0 {
            return self.first_revision;
        }
        Revision(tip.0 - 1)
    }

    fn prune(&mut self) {
        let floor = self.retention_floor();
        if floor <= self.first_revision {
            return;
        }
        let distance = floor.0 - self.first_revision.0;
        let Ok(drop) = usize::try_from(distance) else {
            return;
        };
        let drop = drop.min(self.entries.len());
        let Ok(added) = u64::try_from(drop) else {
            return;
        };
        if drop == 0 {
            return;
        }
        self.entries.drain(..drop);
        self.first_revision = Revision(self.first_revision.0.saturating_add(added));
    }
}
