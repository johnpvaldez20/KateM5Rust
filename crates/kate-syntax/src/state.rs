// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Interned context stack for one highlighting definition.

use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;

use crate::definition::ContextId;

/// One stack frame: the context plus captures for dynamic rules pushed with it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Frame {
    pub context: ContextId,
    pub captures: Option<Arc<[String]>>,
}

/// The per-line-end highlighting state: the context stack. Interned because most lines share a few states.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct State {
    frames: SmallVec<[Frame; 8]>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct StateId(pub u32);

pub struct StateInterner {
    states: Vec<State>,
    ids: HashMap<State, StateId>,
}

impl State {
    pub(crate) fn top(&self) -> &Frame {
        self.frames.last().expect("context stack is never empty")
    }

    pub(crate) fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    /// Pop `n` frames. The initial frame stays.
    /// Returns false when the pop count would remove that frame.
    pub(crate) fn pop_frames(&mut self, n: u8) -> bool {
        if n == 0 {
            return true;
        }
        let len = self.frames.len();
        let survived = len > usize::from(n);
        let keep = len.saturating_sub(usize::from(n)).max(1);
        self.frames.truncate(keep);
        survived
    }
}

impl StateInterner {
    pub fn new(initial: ContextId) -> Self {
        let state = State {
            frames: smallvec::smallvec![Frame {
                context: initial,
                captures: None,
            }],
        };
        let mut ids = HashMap::new();
        ids.insert(state.clone(), StateId(0));
        Self {
            states: vec![state],
            ids,
        }
    }

    pub fn intern(&mut self, state: State) -> StateId {
        if let Some(id) = self.ids.get(&state).copied() {
            return id;
        }
        let id = StateId(u32::try_from(self.states.len()).unwrap_or(u32::MAX));
        self.ids.insert(state.clone(), id);
        self.states.push(state);
        id
    }

    pub fn get(&self, id: StateId) -> &State {
        &self.states[id.0 as usize]
    }

    pub fn initial(&self) -> StateId {
        StateId(0)
    }
}
