use std::any::Any;

use crate::protocol::KasaneRequest;
use crate::scroll::ScrollPlan;
use crate::state::DirtyFlags;

use super::{Command, PluginId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapEffects {
    pub redraw: DirtyFlags,
}

impl Default for BootstrapEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
        }
    }
}

impl BootstrapEffects {
    pub fn merge(&mut self, other: Self) {
        self.redraw |= other.redraw;
    }
}

pub struct SessionReadyEffects {
    pub redraw: DirtyFlags,
    pub commands: Vec<SessionReadyCommand>,
    pub scroll_plans: Vec<ScrollPlan>,
}

impl Default for SessionReadyEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

impl SessionReadyEffects {
    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.commands.append(&mut other.commands);
        self.scroll_plans.append(&mut other.scroll_plans);
    }
}

pub enum SessionReadyCommand {
    SendToKakoune(KasaneRequest),
    Paste,
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
}

#[derive(Default)]
pub struct InitBatch {
    pub effects: BootstrapEffects,
}

#[derive(Default)]
pub struct ReadyBatch {
    pub effects: SessionReadyEffects,
}

pub struct RuntimeEffects {
    pub redraw: DirtyFlags,
    pub commands: Vec<Command>,
    pub scroll_plans: Vec<ScrollPlan>,
}

impl Default for RuntimeEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct RuntimeBatch {
    pub effects: RuntimeEffects,
}

impl RuntimeEffects {
    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.commands.append(&mut other.commands);
        self.scroll_plans.append(&mut other.scroll_plans);
    }
}
