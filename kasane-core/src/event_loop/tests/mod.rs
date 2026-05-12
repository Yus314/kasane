use super::*;

use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::layout::SplitDirection;
use crate::plugin::{
    AppView, AppliedWinnerDelta, Command, PluginAuthorities, PluginDescriptor, PluginId,
    PluginRank, PluginRevision, PluginSource, StdinMode,
};
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use crate::session::SessionSpec;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{SurfacePlacementRequest, SurfaceRegistrationError};
use crate::test_support::TestSurfaceBuilder;

mod dispatch;
mod session;
mod surface;

pub(super) struct TestPlugin {
    pub id: PluginId,
    pub allow_spawn: bool,
    pub authorities: PluginAuthorities,
}

impl crate::plugin::Plugin for TestPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_authorities(self.authorities);
        if !self.allow_spawn {
            r.deny_process_spawn();
        }
    }
}

pub(super) struct RuntimeMessagePlugin;

impl crate::plugin::Plugin for RuntimeMessagePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("runtime-message".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_update_tier2(|_state, msg, _app| {
            if msg.downcast_ref::<u32>() != Some(&11) {
                return ((), crate::plugin::ProcessCapableEffects::none());
            }
            let mut effects = crate::plugin::ProcessCapableEffects::redraw(DirtyFlags::INFO);
            effects
                .base
                .commands
                .push(crate::plugin::KakouneSideCommand::request_redraw(
                    DirtyFlags::STATUS,
                ));
            effects.base.base.scroll_plans.push(ScrollPlan {
                total_amount: 2,
                line: 3,
                column: 5,
                frame_interval_ms: 12,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            });
            ((), effects)
        });
    }
}

pub(super) struct TextInputPlugin;

impl crate::plugin::Plugin for TextInputPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("text-input-plugin".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_text_input(|_state, text, _app| {
            if text == "kana" {
                Some(((), vec![Command::RequestRedraw(DirtyFlags::INFO)]))
            } else {
                None
            }
        });
    }
}

pub(super) struct NoopTimer;

impl TimerScheduler for NoopTimer {
    fn schedule_timer(
        &self,
        _timer_id: u64,
        _delay: Duration,
        _target: PluginId,
        _payload: Box<dyn Any + Send>,
    ) {
    }
    fn cancel_timer(&self, _timer_id: u64) {}
}

#[derive(Default)]
pub(super) struct NoopSessionRuntime {
    pub writer: Vec<u8>,
}

impl SessionRuntime for NoopSessionRuntime {
    fn spawn_session(
        &mut self,
        _spec: SessionSpec,
        _activate: bool,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) {
    }

    fn close_session(
        &mut self,
        _key: Option<&str>,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) -> bool {
        false
    }

    fn switch_session(
        &mut self,
        _key: &str,
        _state: &mut AppState,
        _dirty: &mut DirtyFlags,
        _initial_resize_sent: &mut bool,
    ) {
    }
}

impl SessionHost for NoopSessionRuntime {
    fn active_writer(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

#[derive(Default)]
pub(super) struct RecordingDispatcher {
    pub spawned: Vec<(PluginId, u64, String, Vec<String>, StdinMode)>,
}

impl crate::plugin::ProcessDispatcher for RecordingDispatcher {
    fn spawn(
        &mut self,
        plugin_id: &PluginId,
        job_id: u64,
        program: &str,
        args: &[String],
        stdin_mode: StdinMode,
    ) {
        self.spawned.push((
            plugin_id.clone(),
            job_id,
            program.to_string(),
            args.to_vec(),
            stdin_mode,
        ));
    }

    fn write(&mut self, _plugin_id: &PluginId, _job_id: u64, _data: &[u8]) {}

    fn close_stdin(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

    fn kill(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

    fn resize_pty(&mut self, _plugin_id: &PluginId, _job_id: u64, _rows: u16, _cols: u16) {}

    fn remove_finished_job(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
}

pub(super) struct SurfacePlugin;

impl crate::plugin::Plugin for SurfacePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("surface-plugin".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_surfaces(|_state| {
            vec![TestSurfaceBuilder::new(crate::surface::SurfaceId(200)).build()]
        });
        r.declare_workspace_request(crate::workspace::Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        });
    }
}

pub(super) struct ReplacementSurfacePlugin;

impl crate::plugin::Plugin for ReplacementSurfacePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("surface-plugin".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_surfaces(|_state| {
            vec![TestSurfaceBuilder::new(crate::surface::SurfaceId(200)).build()]
        });
        r.declare_workspace_request(crate::workspace::Placement::SplitFocused {
            direction: SplitDirection::Horizontal,
            ratio: 0.4,
        });
    }
}

pub(super) struct InvalidSurfacePlugin;

impl crate::plugin::Plugin for InvalidSurfacePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("invalid-surface-plugin".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_surfaces(|_state| {
            vec![TestSurfaceBuilder::new(crate::surface::SurfaceId::BUFFER).build()]
        });
    }
}

pub(super) fn owner_delta(old: Option<&str>, new: Option<&str>) -> AppliedWinnerDelta {
    fn descriptor(id: &str, revision: &str) -> PluginDescriptor {
        PluginDescriptor {
            id: PluginId(id.to_string()),
            source: PluginSource::Host {
                provider: "test".to_string(),
            },
            revision: PluginRevision(revision.to_string()),
            rank: PluginRank::HOST,
        }
    }

    AppliedWinnerDelta {
        id: PluginId("surface-plugin".to_string()),
        old: old.map(|rev| descriptor("surface-plugin", rev)),
        new: new.map(|rev| descriptor("surface-plugin", rev)),
    }
}
