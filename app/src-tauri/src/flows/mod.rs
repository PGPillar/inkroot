//! Flow engine. Exactly one flow runs at a time, as a blocking state machine on its own
//! thread. It talks to the UI only through `flow://state` / `flow://progress` / `flow://log`
//! events and receives prompt answers through `answer`.

pub mod backup;
pub mod common;
pub mod import;
pub mod reinstall;
pub mod rescue;
pub mod root;
pub mod unroot;

use crate::backup::BackupLibrary;
use crate::clock::Clock;
use crate::events::Emitter;
use crate::host::keep_awake::KeepAwake;
use crate::host::setup::HostSetup;
use crate::ipc::{
    Emphasis, ErrorCode, FlowError, FlowKind, FlowOptions, FlowResult, FlowState, FlowStatus,
    Progress, Prompt, PromptKind, PromptOption, StepId,
};
use crate::logs::LogStore;
use crate::poller::Poller;
use crate::profile::DeviceProfile;
use crate::settings::{SettingsStore, risk_accepted};
use crate::tools::adb::Adb;
use crate::tools::edl::Edl;
use crate::tools::process::CancelToken;
use crate::tools::usb::UsbProbe;
use crate::tools::{Timeouts, ToolPaths};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

pub const BUSY_MESSAGE: &str = "Inkroot is already working on something. Let it finish first.";
pub const RISK_MESSAGE: &str = "Please read and accept the risk notice first.";
pub const WRITING_MESSAGE: &str =
    "Inkroot is writing to your tablet right now. Cancelling could damage it.";

/// Everything a flow may touch. Real implementations in the app, fakes in tests and demo.
pub struct Deps {
    pub adb: Arc<dyn Adb>,
    pub edl: Arc<dyn Edl>,
    pub usb: Arc<dyn UsbProbe>,
    pub host: Arc<dyn HostSetup>,
    pub awake: Arc<dyn KeepAwake>,
    pub clock: Arc<dyn Clock>,
    pub emitter: Arc<dyn Emitter>,
    pub log: Arc<LogStore>,
    pub library: BackupLibrary,
    pub settings: Arc<SettingsStore>,
    pub profiles: Vec<DeviceProfile>,
    pub tools: ToolPaths,
    pub timeouts: Timeouts,
    pub poller: Option<Arc<Poller>>,
    /// Expected SHA-256 of the bundled Magisk APK (checked before every extraction).
    pub magisk_apk_sha256: String,
}

/// How a flow stopped short of `done`.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowEnd {
    Failed(FlowError),
    Cancelled,
}

impl From<FlowError> for FlowEnd {
    fn from(e: FlowError) -> Self {
        FlowEnd::Failed(e)
    }
}

/// Why a blocking wait inside a flow was interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    Cancelled,
}

impl From<Interrupt> for FlowEnd {
    fn from(_: Interrupt) -> Self {
        FlowEnd::Cancelled
    }
}

/// `Box<dyn FnOnce(&FlowCtx) -> Result<FlowResult, FlowEnd> + Send>`.
pub type FlowFn = Box<dyn FnOnce(&FlowCtx) -> Result<FlowResult, FlowEnd> + Send>;

fn opt(id: &str, label: &str, emphasis: Emphasis) -> PromptOption {
    PromptOption {
        id: id.into(),
        label: label.into(),
        emphasis,
    }
}

/// Options for each prompt kind. Ids are fixed by the contract; labels are defaults.
pub fn prompt_options(kind: PromptKind) -> Vec<PromptOption> {
    use Emphasis::*;
    match kind {
        PromptKind::CableCheck => vec![
            opt("have", "Yes, I have one", Primary),
            opt("whereToGet", "Show me where to get one", Secondary),
            opt("continueWithout", "Continue without one", Danger),
        ],
        PromptKind::CableCheckConfirm => vec![
            opt("confirmContinue", "I understand, continue", Danger),
            opt("back", "Go back", Primary),
        ],
        PromptKind::ChooseSlot => {
            vec![opt("a", "Slot A", Secondary), opt("b", "Slot B", Secondary)]
        }
        PromptKind::KeepWaitingOrRescue => vec![
            opt("keepWaiting", "Keep waiting", Primary),
            opt("goToRescue", "Go to Rescue", Secondary),
        ],
        PromptKind::InstallDriver => vec![
            opt("install", "Install driver", Primary),
            opt("cancel", "Cancel", Secondary),
        ],
        PromptKind::InstallUdev => vec![
            opt("install", "Allow access", Primary),
            opt("cancel", "Cancel", Secondary),
        ],
        PromptKind::StillInEdl => vec![
            opt("retry", "Try restarting again", Primary),
            opt("heldPower", "I've restarted it", Secondary),
        ],
        PromptKind::ConnectionLostAfterWrite => vec![
            opt("restartedOk", "It started normally", Primary),
            opt("runAgain", "Run Rescue again", Secondary),
            opt("cancel", "Cancel", Secondary),
        ],
        PromptKind::TabletStarting => vec![opt(
            "homeScreen",
            "Done — I can see my home screen",
            Primary,
        )],
        PromptKind::EdlNotFound | PromptKind::RetryWrite => vec![
            opt("retry", "Try again", Primary),
            opt("cancel", "Cancel", Secondary),
        ],
    }
}

/// The contract string literal of a unit enum (for log lines).
pub(crate) fn lit<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn internal(message: &str, detail: String) -> FlowError {
    FlowError {
        code: ErrorCode::Internal,
        message: message.into(),
        detail,
        suggested_action: None,
    }
}

/// The snapshot a flow shows before its first `step` call.
fn initial_step(kind: FlowKind) -> (StepId, u8) {
    match kind {
        FlowKind::Backup => (StepId::CheckDevice, 4),
        FlowKind::ImportBackup => (StepId::PickImage, 3),
        FlowKind::Root => (StepId::CheckDevice, 4),
        FlowKind::Rescue => (StepId::PickImage, 4),
        FlowKind::Unroot => (StepId::CheckDevice, 3),
        FlowKind::ReinstallBootService => (StepId::CheckDevice, 2),
    }
}

/// The real flow for each kind. Later tasks register their flows here.
fn dispatch(kind: FlowKind) -> Option<FlowFn> {
    match kind {
        FlowKind::Backup => Some(Box::new(backup::run_backup)),
        FlowKind::ImportBackup => Some(Box::new(import::run_import)),
        FlowKind::Root => Some(Box::new(root::run_root)),
        FlowKind::Rescue => Some(Box::new(rescue::run_rescue)),
        FlowKind::Unroot => Some(Box::new(unroot::run_unroot)),
        FlowKind::ReinstallBootService => Some(Box::new(reinstall::run_reinstall)),
    }
}

enum Msg {
    Answer {
        prompt_id: String,
        option_id: String,
    },
    Cancel,
}

/// State shared by the engine (UI side) and the flow thread.
struct Shared {
    id: String,
    cancel: CancelToken,
    tx: Sender<Msg>,
    state: Mutex<FlowState>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, FlowState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct FlowEngine {
    deps: Deps,
    me: Weak<FlowEngine>,
    /// Lock order: `active` before any `Shared::state`.
    active: Mutex<Option<Arc<Shared>>>,
    last: Mutex<Option<FlowState>>,
}

impl FlowEngine {
    pub fn new(deps: Deps) -> Arc<FlowEngine> {
        Arc::new_cyclic(|me| FlowEngine {
            deps,
            me: me.clone(),
            active: Mutex::new(None),
            last: Mutex::new(None),
        })
    }

    pub fn deps(&self) -> &Deps {
        &self.deps
    }

    fn active(&self) -> MutexGuard<'_, Option<Arc<Shared>>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts the real flow for `kind`.
    pub fn start(&self, kind: FlowKind, opts: FlowOptions) -> Result<String, FlowError> {
        self.start_impl(kind, opts, dispatch(kind))
    }

    /// Starts `flow` under every gate and all the bookkeeping of `start`. For tests and for
    /// composing flows; the app always goes through `start`.
    #[doc(hidden)]
    pub fn start_with(
        &self,
        kind: FlowKind,
        opts: FlowOptions,
        flow: FlowFn,
    ) -> Result<String, FlowError> {
        self.start_impl(kind, opts, Some(flow))
    }

    fn start_impl(
        &self,
        kind: FlowKind,
        opts: FlowOptions,
        flow: Option<FlowFn>,
    ) -> Result<String, FlowError> {
        let d = &self.deps;
        let mut active = self.active();
        if active.is_some() {
            return Err(FlowError {
                code: ErrorCode::Busy,
                message: BUSY_MESSAGE.into(),
                detail: "another flow is active".into(),
                suggested_action: None,
            });
        }
        if matches!(kind, FlowKind::Root | FlowKind::Unroot | FlowKind::Rescue)
            && !risk_accepted(&d.settings.get())
        {
            return Err(internal(
                RISK_MESSAGE,
                format!("acceptedRiskVersion is not current for {}", lit(&kind)),
            ));
        }
        let profile = d.profiles.first().ok_or_else(|| {
            internal(
                "Something went wrong inside Inkroot. Please reinstall it and try again.",
                "no device profiles loaded".into(),
            )
        })?;
        d.tools.require_for(kind, profile)?;
        let Some(flow) = flow else {
            return Err(internal(
                "This isn't available in this version of Inkroot yet.",
                format!("flow {} is not available yet", lit(&kind)),
            ));
        };
        let engine = self.me.upgrade().ok_or_else(|| {
            internal(
                "Something went wrong inside Inkroot. Please try again.",
                "engine dropped".into(),
            )
        })?;

        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = channel();
        let (step, step_count) = initial_step(kind);
        let shared = Arc::new(Shared {
            id: id.clone(),
            cancel: CancelToken::default(),
            tx,
            state: Mutex::new(FlowState {
                flow_id: id.clone(),
                kind,
                step,
                step_index: 1,
                step_count,
                status: FlowStatus::Running,
                prompt: None,
                error: None,
                result: None,
                writing: false,
            }),
        });

        // Claim the slot, then release the engine lock before anything that may block on
        // poller I/O, so writing()/current()/answer()/cancel() never wait on a probe.
        *active = Some(shared.clone());
        drop(active);

        if let Some(p) = &d.poller {
            p.pause();
        }
        d.log.set_flow(Some(id.clone()));
        d.log.info(&format!("Starting {} (flow {id})", lit(&kind)));

        // The initial snapshot is emitted by the flow thread itself (before the flow runs),
        // so nothing is shown unless the thread really started, and it precedes every other
        // snapshot of this flow.
        let thread_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("inkroot-flow".into())
            .spawn(move || run_flow(engine, thread_shared, rx, opts, flow));
        if let Err(e) = spawned {
            d.log.set_flow(None);
            if let Some(p) = &d.poller {
                p.resume();
            }
            let mut active = self.active();
            if active.as_ref().is_some_and(|a| a.id == shared.id) {
                *active = None;
            }
            return Err(internal(
                "Something went wrong inside Inkroot. Please try again.",
                format!("could not start flow thread: {e}"),
            ));
        }
        Ok(id)
    }

    /// Delivers a prompt answer. Unknown flows, stale prompt ids and options that weren't
    /// offered are ignored.
    pub fn answer(&self, flow_id: &str, prompt_id: &str, option_id: &str) {
        let active = self.active();
        let Some(sh) = active.as_ref().filter(|s| s.id == flow_id) else {
            return;
        };
        let current = sh.state().prompt.as_ref().map(|p| p.id.clone());
        if current.as_deref() == Some(prompt_id) {
            let _ = sh.tx.send(Msg::Answer {
                prompt_id: prompt_id.into(),
                option_id: option_id.into(),
            });
        }
    }

    /// Cancels the active flow, unless it is writing to the tablet.
    pub fn cancel(&self, flow_id: &str) -> Result<(), FlowError> {
        let active = self.active();
        let Some(sh) = active.as_ref().filter(|s| s.id == flow_id) else {
            return Ok(());
        };
        // Checked under the state lock that `set_writing` takes, so a write can't start
        // between this check and the token being set without seeing the token.
        let st = sh.state();
        if st.writing {
            return Err(internal(
                WRITING_MESSAGE,
                "cancel rejected: partition write in progress".into(),
            ));
        }
        sh.cancel.cancel();
        let _ = sh.tx.send(Msg::Cancel);
        drop(st);
        self.deps.log.info("Cancel requested");
        Ok(())
    }

    pub fn writing(&self) -> bool {
        self.active().as_ref().is_some_and(|s| s.state().writing)
    }

    /// The active flow's latest snapshot, else the last finished flow's final snapshot.
    pub fn current(&self) -> Option<FlowState> {
        let active = self.active();
        if let Some(sh) = active.as_ref() {
            return Some(sh.state().clone());
        }
        drop(active);
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn finish(&self, sh: &Shared, outcome: Result<FlowResult, FlowEnd>) {
        let d = &self.deps;
        match &outcome {
            Ok(_) => d.log.info("Finished"),
            Err(FlowEnd::Cancelled) => d.log.info("Cancelled"),
            Err(FlowEnd::Failed(e)) => {
                d.log.info(&format!("Failed: {} ({})", e.message, e.detail));
            }
        }
        d.log.set_flow(None);
        if let Some(p) = &d.poller {
            p.resume();
        }
        // Hold `active` across the final emit and the clear: a `start` that runs after the
        // terminal snapshot was observed always finds the engine idle, and no new flow's
        // snapshot can be emitted before this one.
        let mut active = self.active();
        let final_state = {
            let mut st = sh.state();
            st.prompt = None;
            st.writing = false;
            match outcome {
                Ok(r) => {
                    st.status = FlowStatus::Done;
                    st.result = Some(r);
                    st.error = None;
                }
                Err(FlowEnd::Failed(e)) => {
                    st.status = FlowStatus::Failed;
                    st.error = Some(e);
                    st.result = None;
                }
                Err(FlowEnd::Cancelled) => {
                    st.status = FlowStatus::Cancelled;
                    st.error = None;
                    st.result = None;
                }
            }
            st.clone()
        };
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(final_state.clone());
        d.emitter.flow_state(&final_state);
        if active.as_ref().is_some_and(|a| a.id == sh.id) {
            *active = None;
        }
    }
}

fn run_flow(
    engine: Arc<FlowEngine>,
    shared: Arc<Shared>,
    rx: Receiver<Msg>,
    opts: FlowOptions,
    flow: FlowFn,
) {
    engine.deps.emitter.flow_state(&shared.state());
    let ctx = FlowCtx {
        engine: engine.clone(),
        shared: shared.clone(),
        rx: Mutex::new(rx),
        opts,
        edl_entered: AtomicBool::new(false),
        write_attempted: AtomicBool::new(false),
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| flow(&ctx)))
        .unwrap_or_else(|p| {
            let detail = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "flow panicked".into());
            Err(FlowEnd::Failed(internal(
                "Something went wrong inside Inkroot. Please try again.",
                format!("flow panicked: {detail}"),
            )))
        });
    if outcome.is_err() {
        ctx.release_edl_if_untouched();
    }
    drop(ctx);
    engine.finish(&shared, outcome);
}

/// Handed to flow functions; every state change goes through it.
pub struct FlowCtx {
    engine: Arc<FlowEngine>,
    shared: Arc<Shared>,
    rx: Mutex<Receiver<Msg>>,
    opts: FlowOptions,
    /// This flow sent the tablet into EDL itself (or found it there in a flow that started
    /// over adb), and hasn't restarted it since.
    edl_entered: AtomicBool,
    /// A partition write was started at some point in this flow.
    write_attempted: AtomicBool,
}

impl FlowCtx {
    pub fn id(&self) -> &str {
        &self.shared.id
    }

    pub fn deps(&self) -> &Deps {
        &self.engine.deps
    }

    pub fn opts(&self) -> &FlowOptions {
        &self.opts
    }

    pub fn kind(&self) -> FlowKind {
        self.shared.state().kind
    }

    /// Changes the flow's kind in place (reinstall → root); the flow id stays the same.
    pub fn set_kind(&self, kind: FlowKind) {
        self.info(&format!("Continuing as {}", lit(&kind)));
        self.update(|s| s.kind = kind);
    }

    pub fn info(&self, line: &str) {
        self.deps().log.info(line);
    }

    /// Applies `f` to the snapshot and emits it (under the state lock, so snapshots stay
    /// ordered).
    fn update(&self, f: impl FnOnce(&mut FlowState)) {
        let mut st = self.shared.state();
        f(&mut st);
        self.deps().emitter.flow_state(&st);
    }

    /// Enters a step: status `running`, no prompt.
    pub fn step(&self, step: StepId, index: u8, count: u8) {
        self.info(&format!("Step {index} of {count}: {}", lit(&step)));
        self.update(|s| {
            s.step = step;
            s.step_index = index;
            s.step_count = count;
            s.status = FlowStatus::Running;
            s.prompt = None;
        });
    }

    /// Changes only the step id, keeping the visible step number (sub-steps of a group).
    pub fn sub_step(&self, step: StepId) {
        self.info(&format!("Now: {}", lit(&step)));
        self.update(|s| {
            s.step = step;
            s.status = FlowStatus::Running;
            s.prompt = None;
        });
    }

    pub fn progress(&self, done: u64, total: u64, label: &str) {
        self.deps().emitter.progress(&Progress {
            flow_id: self.shared.id.clone(),
            done,
            total,
            label: label.into(),
        });
    }

    /// Shows a prompt and blocks until it is answered (returns the option id) or the flow is
    /// cancelled.
    pub fn ask(&self, kind: PromptKind) -> Result<String, Interrupt> {
        self.check_cancel()?;
        let prompt = Prompt {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            options: prompt_options(kind),
        };
        self.info(&format!("Waiting for your answer: {}", lit(&kind)));
        let p = prompt.clone();
        self.update(|s| {
            s.status = FlowStatus::WaitingForUser;
            s.prompt = Some(p);
        });
        let rx = self.rx.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match rx.recv() {
                Ok(Msg::Answer {
                    prompt_id,
                    option_id,
                }) if prompt_id == prompt.id
                    && prompt.options.iter().any(|o| o.id == option_id) =>
                {
                    self.info(&format!("You chose: {option_id}"));
                    self.update(|s| {
                        s.status = FlowStatus::Running;
                        s.prompt = None;
                    });
                    return Ok(option_id);
                }
                Ok(Msg::Answer { .. }) => continue,
                Ok(Msg::Cancel) | Err(_) => {
                    self.update(|s| {
                        s.status = FlowStatus::Running;
                        s.prompt = None;
                    });
                    return Err(Interrupt::Cancelled);
                }
            }
        }
    }

    /// Shows a prompt without blocking: the flow keeps working and checks `poll_answer`.
    pub fn offer(&self, kind: PromptKind) -> Prompt {
        let prompt = Prompt {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            options: prompt_options(kind),
        };
        self.info(&format!("Offering: {}", lit(&kind)));
        let p = prompt.clone();
        self.update(|s| {
            s.status = FlowStatus::WaitingForUser;
            s.prompt = Some(p);
        });
        prompt
    }

    /// The answer to an `offer`ed prompt, if one has arrived (never blocks). A cancel request
    /// is `Err`. Either way the prompt stays up; the caller moves on with `step`/`sub_step`.
    pub fn poll_answer(&self, prompt: &Prompt) -> Result<Option<String>, Interrupt> {
        self.check_cancel()?;
        let rx = self.rx.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match rx.try_recv() {
                Ok(Msg::Answer {
                    prompt_id,
                    option_id,
                }) if prompt_id == prompt.id
                    && prompt.options.iter().any(|o| o.id == option_id) =>
                {
                    self.info(&format!("You chose: {option_id}"));
                    return Ok(Some(option_id));
                }
                Ok(Msg::Answer { .. }) => continue,
                Ok(Msg::Cancel) => return Err(Interrupt::Cancelled),
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(None),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(Interrupt::Cancelled);
                }
            }
        }
    }

    /// Marks a partition write in progress (the UI blocks close/cancel). Emits on change.
    pub fn set_writing(&self, writing: bool) {
        let mut st = self.shared.state();
        if st.writing != writing {
            st.writing = writing;
            self.deps().emitter.flow_state(&st);
        }
    }

    pub fn check_cancel(&self) -> Result<(), Interrupt> {
        if self.shared.cancel.is_cancelled() {
            Err(Interrupt::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn cancel_token(&self) -> &CancelToken {
        &self.shared.cancel
    }

    /// Records that this flow put the tablet into EDL (an adb-started flow), so a failure or
    /// cancel before any write restarts it instead of leaving it stranded there.
    pub fn note_edl_entered(&self) {
        self.edl_entered.store(true, Ordering::SeqCst);
    }

    /// Records that the flow has restarted the tablet out of EDL itself.
    pub fn note_edl_reset(&self) {
        self.edl_entered.store(false, Ordering::SeqCst);
    }

    /// Records that a partition write is about to start. From then on the flow never restarts
    /// the tablet unless a read-back matched.
    pub fn note_write_attempted(&self) {
        self.write_attempted.store(true, Ordering::SeqCst);
    }

    pub fn write_attempted(&self) -> bool {
        self.write_attempted.load(Ordering::SeqCst)
    }

    /// After a failed or cancelled flow: if this flow sent the tablet into EDL, nothing was
    /// ever written, and the tablet is still there, restart it (best effort). Never after a
    /// write was attempted; never for a tablet the flow didn't send into EDL (Rescue).
    fn release_edl_if_untouched(&self) {
        if !self.edl_entered.load(Ordering::SeqCst) || self.write_attempted() {
            return;
        }
        let d = self.deps();
        if d.usb.edl_device().is_none() {
            return;
        }
        self.info("Restarting the tablet out of rescue mode (nothing was written to it)");
        common::best_effort_restart(self);
    }
}
