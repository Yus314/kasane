use kasane_core::input::InputEvent;
use kasane_core::plugin::{Command, PluginId, PluginRuntime};
use kasane_core::protocol::KasaneRequest;
use kasane_core::scroll::{
    ScrollPolicyResult, ScrollRuntime, consume_info_scroll, default_scroll_candidate,
    fallback_scroll_policy, is_scroll_event, resolve_default_scroll_policy,
    selection_scroll_edge_line,
};
use kasane_core::state::{AppState, DirtyFlags, DragState, Msg, UpdateResult, update_in_place};

pub struct StepOutcome {
    pub dirty: DirtyFlags,
    pub commands: Vec<Command>,
    pub owner: Option<PluginId>,
}

impl StepOutcome {
    pub fn requests(&self) -> Vec<KasaneRequest> {
        self.commands
            .iter()
            .filter_map(|command| match command {
                Command::SendToKakoune(request) => Some(request.clone()),
                _ => None,
            })
            .collect()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum TraceStep {
    Input(InputEvent),
    Tick,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct Emitted {
    pub requests: Vec<KasaneRequest>,
    pub dirty: DirtyFlags,
    pub owner: Option<PluginId>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TraceOutcome {
    pub emitted: Vec<Emitted>,
    pub final_state: AppState,
}

#[allow(dead_code)]
pub struct LegacyHarness {
    pub state: Box<AppState>,
    pub registry: PluginRuntime,
    pub scroll_amount: i32,
    pub runtime: ScrollRuntime,
}

#[allow(dead_code)]
impl LegacyHarness {
    pub fn new(state: AppState, registry: PluginRuntime) -> Self {
        Self {
            state: Box::new(state),
            registry,
            scroll_amount: 3,
            runtime: ScrollRuntime {
                initial_resize_complete: true,
                ..ScrollRuntime::default()
            },
        }
    }

    pub fn dispatch_input(&mut self, input: InputEvent) -> StepOutcome {
        let UpdateResult {
            flags: dirty,
            commands,
            scroll_plans,
            source_plugin: owner,
        } = update_in_place(
            &mut self.state,
            Msg::from(input),
            &mut self.registry,
            self.scroll_amount,
        );
        for plan in scroll_plans {
            self.runtime.enqueue(plan);
        }
        StepOutcome {
            dirty,
            commands,
            owner,
        }
    }

    pub fn tick_animation(&mut self) -> StepOutcome {
        let commands = self
            .runtime
            .tick()
            .map(|resolved| Command::SendToKakoune(resolved.to_kakoune_request()))
            .into_iter()
            .collect();
        StepOutcome {
            dirty: DirtyFlags::empty(),
            commands,
            owner: None,
        }
    }

    #[allow(dead_code)]
    pub fn run_trace(&mut self, trace: &[TraceStep]) -> TraceOutcome {
        let emitted = trace
            .iter()
            .map(|step| match step {
                TraceStep::Input(input) => self.dispatch_input(input.clone()),
                TraceStep::Tick => self.tick_animation(),
            })
            .map(|outcome| Emitted {
                requests: outcome.requests(),
                dirty: outcome.dirty,
                owner: outcome.owner,
            })
            .collect();

        TraceOutcome {
            emitted,
            final_state: (*self.state).clone(),
        }
    }
}

#[allow(dead_code)]
pub struct NewHarness {
    pub state: Box<AppState>,
    pub registry: PluginRuntime,
    pub scroll_amount: i32,
    pub forced_scroll_policy: Option<ScrollPolicyResult>,
    pub runtime: ScrollRuntime,
}

#[allow(dead_code)]
impl NewHarness {
    pub fn new(state: AppState, registry: PluginRuntime) -> Self {
        Self {
            state: Box::new(state),
            registry,
            scroll_amount: 3,
            forced_scroll_policy: None,
            runtime: ScrollRuntime::default(),
        }
    }

    pub fn dispatch_input(&mut self, input: InputEvent) -> StepOutcome {
        if let Some(outcome) = self.try_dispatch_drag_scroll(&input) {
            return outcome;
        }
        if let Some(outcome) = self.try_dispatch_info_scroll(&input) {
            return outcome;
        }
        if let Some(outcome) = self.try_dispatch_default_scroll(&input) {
            return outcome;
        }

        let UpdateResult {
            flags: dirty,
            commands,
            scroll_plans,
            source_plugin: owner,
        } = update_in_place(
            &mut self.state,
            Msg::from(input),
            &mut self.registry,
            self.scroll_amount,
        );
        for plan in scroll_plans {
            self.runtime.enqueue(plan);
        }
        StepOutcome {
            dirty,
            commands,
            owner,
        }
    }

    pub fn tick_runtime(&mut self) -> StepOutcome {
        let commands = self
            .runtime
            .tick()
            .map(|resolved| Command::SendToKakoune(resolved.to_kakoune_request()))
            .into_iter()
            .collect();
        StepOutcome {
            dirty: DirtyFlags::empty(),
            commands,
            owner: None,
        }
    }

    fn try_dispatch_default_scroll(&mut self, input: &InputEvent) -> Option<StepOutcome> {
        let InputEvent::Mouse(mouse) = input else {
            return None;
        };

        if !matches!(&self.state.runtime.drag, DragState::None) {
            return None;
        }
        if !self.state.observed.infos.is_empty() || !is_scroll_event(mouse) {
            return None;
        }
        if self
            .state
            .runtime
            .hit_map
            .test(mouse.column as u16, mouse.line as u16)
            .is_some()
        {
            return None;
        }

        let candidate = default_scroll_candidate(mouse, self.scroll_amount)?;
        let result = self.forced_scroll_policy.unwrap_or_else(|| {
            resolve_default_scroll_policy(&mut self.registry, &self.state, candidate)
        });
        Some(StepOutcome {
            dirty: DirtyFlags::empty(),
            commands: self.apply_policy_result(candidate, result),
            owner: None,
        })
    }

    fn try_dispatch_drag_scroll(&mut self, input: &InputEvent) -> Option<StepOutcome> {
        let InputEvent::Mouse(mouse) = input else {
            return None;
        };

        if !matches!(
            self.state.runtime.drag,
            DragState::Active {
                button: kasane_core::input::MouseButton::Left,
                ..
            }
        ) || !is_scroll_event(mouse)
        {
            return None;
        }

        let hit_map = std::mem::take(&mut self.state.runtime.hit_map);
        let consumed = consume_info_scroll(&mut self.state, mouse, &hit_map);
        self.state.runtime.hit_map = hit_map;
        if consumed {
            return Some(StepOutcome {
                dirty: DirtyFlags::INFO,
                commands: Vec::new(),
                owner: None,
            });
        }

        let candidate = default_scroll_candidate(mouse, self.scroll_amount)?;
        let mut commands = self.apply_policy_result(candidate, fallback_scroll_policy(candidate));
        let edge_line = selection_scroll_edge_line(
            self.state.runtime.rows,
            mouse,
            self.state.config.scroll_edge_margin,
        )?;
        commands.push(Command::SendToKakoune(KasaneRequest::MouseMove {
            line: edge_line,
            column: mouse.column,
        }));
        Some(StepOutcome {
            dirty: DirtyFlags::empty(),
            commands,
            owner: None,
        })
    }

    fn apply_policy_result(
        &mut self,
        candidate: kasane_core::scroll::DefaultScrollCandidate,
        result: ScrollPolicyResult,
    ) -> Vec<Command> {
        match result {
            ScrollPolicyResult::Pass => {
                commands_from_policy_result(fallback_scroll_policy(candidate))
            }
            ScrollPolicyResult::Suppress => Vec::new(),
            ScrollPolicyResult::Immediate(resolved) => {
                commands_from_policy_result(ScrollPolicyResult::Immediate(resolved))
            }
            ScrollPolicyResult::Plan(plan) => {
                self.runtime.enqueue(plan);
                Vec::new()
            }
        }
    }

    fn try_dispatch_info_scroll(&mut self, input: &InputEvent) -> Option<StepOutcome> {
        let InputEvent::Mouse(mouse) = input else {
            return None;
        };

        if !is_scroll_event(mouse) {
            return None;
        }

        let hit_map = std::mem::take(&mut self.state.runtime.hit_map);
        let consumed = consume_info_scroll(&mut self.state, mouse, &hit_map);
        self.state.runtime.hit_map = hit_map;
        if !consumed {
            return None;
        }

        Some(StepOutcome {
            dirty: DirtyFlags::INFO,
            commands: Vec::new(),
            owner: None,
        })
    }

    #[allow(dead_code)]
    pub fn run_trace(&mut self, trace: &[TraceStep]) -> TraceOutcome {
        let emitted = trace
            .iter()
            .map(|step| match step {
                TraceStep::Input(input) => self.dispatch_input(input.clone()),
                TraceStep::Tick => self.tick_runtime(),
            })
            .map(|outcome| Emitted {
                requests: outcome.requests(),
                dirty: outcome.dirty,
                owner: outcome.owner,
            })
            .collect();

        TraceOutcome {
            emitted,
            final_state: (*self.state).clone(),
        }
    }
}

fn commands_from_policy_result(result: ScrollPolicyResult) -> Vec<Command> {
    match result {
        ScrollPolicyResult::Pass | ScrollPolicyResult::Suppress | ScrollPolicyResult::Plan(_) => {
            Vec::new()
        }
        ScrollPolicyResult::Immediate(resolved) => {
            vec![Command::SendToKakoune(resolved.to_kakoune_request())]
        }
    }
}

#[allow(dead_code)]
pub fn flatten_requests(outcome: &TraceOutcome) -> Vec<KasaneRequest> {
    outcome
        .emitted
        .iter()
        .flat_map(|emitted| emitted.requests.iter().cloned())
        .collect()
}

#[allow(dead_code)]
pub fn assert_same_requests(left: &TraceOutcome, right: &TraceOutcome) {
    assert_eq!(flatten_requests(left), flatten_requests(right));
}

#[allow(dead_code)]
pub fn assert_same_flags(left: &TraceOutcome, right: &TraceOutcome) {
    let left_flags: Vec<_> = left.emitted.iter().map(|emitted| emitted.dirty).collect();
    let right_flags: Vec<_> = right.emitted.iter().map(|emitted| emitted.dirty).collect();
    assert_eq!(left_flags, right_flags);
}

#[allow(dead_code)]
pub fn assert_same_visible_state(left: &TraceOutcome, right: &TraceOutcome) {
    assert_eq!(
        left.final_state.runtime.drag,
        right.final_state.runtime.drag
    );
    assert_eq!(
        left.final_state.observed.infos.len(),
        right.final_state.observed.infos.len()
    );
    let left_offsets: Vec<_> = left
        .final_state
        .observed
        .infos
        .iter()
        .map(|info| info.scroll_offset)
        .collect();
    let right_offsets: Vec<_> = right
        .final_state
        .observed
        .infos
        .iter()
        .map(|info| info.scroll_offset)
        .collect();
    assert_eq!(left_offsets, right_offsets);
}
