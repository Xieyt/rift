//! Actor that owns the scrolling-strip hint bar window.
//!
//! The reactor pushes a [`Snapshot`] of the active scrolling workspace on every
//! layout apply; this actor decides — per the configured visibility mode —
//! whether the bar is shown, drives the `auto` flash timer, and routes segment
//! clicks (delivered from the event tap as [`Event::MouseDown`]) to a
//! `FocusWindow` reactor command. It runs on the main thread because the CGS
//! window and its layer tree must be created and mutated there.
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_foundation::{CGPoint, CGRect};
use tracing::{instrument, warn};

use crate::actor::reactor::{Command, ReactorCommand};
use crate::actor::{self, reactor};
use crate::layout_engine::LayoutCommand;
use crate::common::config::{Config, HintsBarOverflow, HintsBarVisibility};
use crate::sys::screen::SpaceId;
use crate::sys::timer::Timer;
use crate::ui::hints_bar::{
    ColumnCell, HintsBar as HintsBarWindow, HintsBarData, HintsBarStyle, WorkspaceCell,
};
use crate::ui::stack_line::point_hits_indicator_frame;

/// Bar frames published for the event tap to hit-test clicks against.
pub type SharedHitRects = Arc<ArcSwap<Vec<CGRect>>>;

pub fn new_shared_hit_rects() -> SharedHitRects { Arc::new(ArcSwap::from_pointee(Vec::new())) }

/// Full strip state for one workspace, computed by the reactor.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub space_id: SpaceId,
    pub bar_frame: CGRect,
    /// Full display rect the bar is on (for right-edge pill placement).
    pub screen_frame: CGRect,
    pub cells: Vec<ColumnCell>,
    /// Occupied workspaces for the overview rail.
    pub workspaces: Vec<WorkspaceCell>,
    /// Overflow policy resolved from config.
    pub overflow: HintsBarOverflow,
    /// Column chips that fit on the top strip (== `cells.len()` if no overflow).
    pub top_count: usize,
    /// Frame for the second (overflow) bar in `bar` mode; `None` otherwise.
    pub overflow_frame: Option<CGRect>,
}

#[derive(Debug)]
pub enum Event {
    /// New strip state for the active display space (empty cells = nothing to show).
    Snapshot(Snapshot),
    ConfigUpdated(Config),
    /// A click that the event tap resolved onto the bar (global CG coordinates).
    MouseDown(CGPoint),
    /// `toggle_hints_bar` command.
    Toggle,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

type CellSig = (
    Vec<(String, bool, bool, bool, usize, usize, String)>,
    Vec<(String, bool, Vec<crate::sys::app::pid_t>)>,
    usize,
);

pub struct HintsBar {
    config: Config,
    rx: Receiver,
    #[allow(dead_code)]
    mtm: MainThreadMarker,
    reactor_tx: reactor::Sender,
    shared_hit_rects: SharedHitRects,
    bar: Option<HintsBarWindow>,
    /// Second strip for `overflow = "bar"` (opposite edge); `None` otherwise.
    overflow_bar: Option<HintsBarWindow>,
    last_snapshot: Option<Snapshot>,
    last_sig: Option<CellSig>,
    /// Whether the user toggled the bar on (only meaningful for `on_demand`).
    user_shown: bool,
    /// True while the `auto`-mode flash is showing (guards the hide timer).
    auto_pending: bool,
    /// Delay to (re)arm the hide timer with on the next run-loop turn.
    pending_arm: Option<Duration>,
    /// Newest snapshot awaiting the debounce window (coalesces bursts).
    pending: Option<Snapshot>,
    /// Whether the debounce timer is currently armed.
    debounce_armed: bool,
}

impl HintsBar {
    pub fn new(
        config: Config,
        rx: Receiver,
        mtm: MainThreadMarker,
        reactor_tx: reactor::Sender,
        shared_hit_rects: SharedHitRects,
    ) -> Self {
        Self {
            config,
            rx,
            mtm,
            reactor_tx,
            shared_hit_rects,
            bar: None,
            overflow_bar: None,
            last_snapshot: None,
            last_sig: None,
            user_shown: false,
            auto_pending: false,
            pending_arm: None,
            pending: None,
            debounce_armed: false,
        }
    }

    pub async fn run(mut self) {
        // Auto-flash hide timer + a short debounce that coalesces bursts of
        // snapshots (e.g. during a scroll gesture) into a single render.
        const DEBOUNCE_MS: u64 = 12;
        let mut hide_timer = Timer::manual();
        let mut debounce_timer = Timer::manual();
        loop {
            tokio::select! {
                msg = self.rx.recv() => match msg {
                    Some((span, event)) => {
                        let _guard = span.enter();
                        match event {
                            Event::Snapshot(s) => {
                                // Keep only the newest; render after the window.
                                self.pending = Some(s);
                                if !self.debounce_armed {
                                    debounce_timer
                                        .set_next_fire(Duration::from_millis(DEBOUNCE_MS));
                                    self.debounce_armed = true;
                                }
                            }
                            other => {
                                self.flush_pending();
                                self.handle_event(other);
                                self.arm_hide(&mut hide_timer);
                            }
                        }
                    }
                    None => break,
                },
                _ = hide_timer.next(), if self.auto_pending => {
                    self.auto_pending = false;
                    self.hide();
                }
                _ = debounce_timer.next(), if self.debounce_armed => {
                    self.debounce_armed = false;
                    self.flush_pending();
                    self.arm_hide(&mut hide_timer);
                }
            }
        }
    }

    fn flush_pending(&mut self) {
        if let Some(snapshot) = self.pending.take() {
            self.on_snapshot(snapshot);
        }
    }

    fn arm_hide(&mut self, hide_timer: &mut Timer) {
        if let Some(delay) = self.pending_arm.take() {
            hide_timer.set_next_fire(delay);
            self.auto_pending = true;
        }
    }

    fn bar_visible(&self) -> bool { self.bar.as_ref().is_some_and(|b| b.is_visible()) }

    fn is_enabled(&self) -> bool { self.config.settings.ui.hints_bar.enabled }

    #[instrument(name = "hints_bar::handle_event", skip(self))]
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => self.on_snapshot(snapshot),
            Event::ConfigUpdated(config) => self.on_config(config),
            Event::MouseDown(point) => self.on_mouse_down(point),
            Event::Toggle => self.on_toggle(),
        }
    }

    fn signature(cells: &[ColumnCell], workspaces: &[WorkspaceCell], top_count: usize) -> CellSig {
        let entries = cells
            .iter()
            .map(|c| {
                let active_label = c
                    .members
                    .get(c.active)
                    .map(|m| format!("{}\u{1}{}", m.label, m.title))
                    .unwrap_or_default();
                (
                    c.hint.clone(),
                    c.focused,
                    c.visible,
                    c.tabbed,
                    c.active,
                    c.members.len(),
                    active_label,
                )
            })
            .collect();
        let ws = workspaces
            .iter()
            .map(|w| (w.label.clone(), w.active, w.pids.clone()))
            .collect();
        (entries, ws, top_count)
    }

    fn on_snapshot(&mut self, snapshot: Snapshot) {
        if !self.is_enabled() {
            self.teardown();
            return;
        }
        let sig = Self::signature(&snapshot.cells, &snapshot.workspaces, snapshot.top_count);
        let changed = self.last_sig.as_ref() != Some(&sig);
        self.last_sig = Some(sig);
        self.last_snapshot = Some(snapshot);
        self.reevaluate(changed);
    }

    fn on_config(&mut self, config: Config) {
        self.config = config;
        if !self.is_enabled() {
            self.teardown();
            return;
        }
        // Restyle / re-place with the latest snapshot.
        self.reevaluate(true);
    }

    fn on_toggle(&mut self) {
        if !self.is_enabled() {
            return;
        }
        match self.config.settings.ui.hints_bar.visibility {
            HintsBarVisibility::OnDemand => {
                self.user_shown = !self.user_shown;
                self.reevaluate(false);
            }
            HintsBarVisibility::Auto => {
                self.arm_flash();
                self.reevaluate(false);
            }
            HintsBarVisibility::Always => {}
        }
    }

    /// Decide visibility for the current mode and render or hide accordingly.
    fn reevaluate(&mut self, changed: bool) {
        let Some(snapshot) = self.last_snapshot.clone() else {
            return;
        };
        if !self.is_enabled() || snapshot.cells.is_empty() {
            self.hide();
            return;
        }

        let show = match self.config.settings.ui.hints_bar.visibility {
            HintsBarVisibility::Always => true,
            HintsBarVisibility::OnDemand => self.user_shown,
            HintsBarVisibility::Auto => {
                if changed {
                    self.arm_flash();
                }
                changed || self.auto_pending || self.pending_arm.is_some()
            }
        };

        if show {
            if changed || !self.bar_visible() {
                self.render(snapshot);
            }
        } else {
            self.hide();
        }
    }

    fn arm_flash(&mut self) {
        let ms = self.config.settings.ui.hints_bar.auto_hide_ms.max(1);
        self.pending_arm = Some(Duration::from_millis(ms));
    }

    /// Display backing scale so the bar rasterizes crisply on Retina.
    fn backing_scale(&self) -> f64 {
        NSScreen::mainScreen(self.mtm).map(|s| s.backingScaleFactor()).unwrap_or(2.0)
    }

    fn render(&mut self, snap: Snapshot) {
        tracing::trace!(target: "hbbench", "hb_render");
        let style = HintsBarStyle::from(&self.config.settings.ui.hints_bar);
        let scale = self.backing_scale();
        let total = snap.cells.len();
        let has_overflow = snap.top_count < total;

        // Partition columns per overflow mode.
        let (main_cells, main_split, overflow_cells): (
            Vec<ColumnCell>,
            Option<usize>,
            Vec<ColumnCell>,
        ) = match snap.overflow {
            HintsBarOverflow::Row if has_overflow => {
                (snap.cells.clone(), Some(snap.top_count), Vec::new())
            }
            HintsBarOverflow::Bar if has_overflow => (
                snap.cells[..snap.top_count].to_vec(),
                None,
                snap.cells[snap.top_count..].to_vec(),
            ),
            _ => (snap.cells.clone(), None, Vec::new()),
        };

        // Main bar: create or reframe, then update.
        let main = match &self.bar {
            Some(bar) => {
                if bar.frame() != snap.bar_frame {
                    if let Err(err) = bar.set_frame(snap.bar_frame) {
                        warn!(?err, "hints_bar: set_frame failed");
                    }
                }
                bar
            }
            None => match HintsBarWindow::new(snap.bar_frame, style, scale) {
                Ok(bar) => {
                    self.bar = Some(bar);
                    self.bar.as_ref().unwrap()
                }
                Err(err) => {
                    warn!(?err, "hints_bar: failed to create bar window");
                    return;
                }
            },
        };
        if let Err(err) = main.update(
            style,
            HintsBarData {
                cells: main_cells,
                screen: snap.screen_frame,
                workspaces: snap.workspaces.clone(),
                overflow_split: main_split,
            },
        ) {
            warn!(?err, "hints_bar: update failed");
        }

        // Overflow bar (only `bar` mode with real overflow).
        if let Some(of_frame) = snap.overflow_frame.filter(|_| !overflow_cells.is_empty()) {
            let ob = match &self.overflow_bar {
                Some(ob) => {
                    if ob.frame() != of_frame {
                        if let Err(err) = ob.set_frame(of_frame) {
                            warn!(?err, "hints_bar: overflow set_frame failed");
                        }
                    }
                    ob
                }
                None => match HintsBarWindow::new(of_frame, style, scale) {
                    Ok(ob) => {
                        self.overflow_bar = Some(ob);
                        self.overflow_bar.as_ref().unwrap()
                    }
                    Err(err) => {
                        warn!(?err, "hints_bar: failed to create overflow bar");
                        self.sync_hit_rects();
                        return;
                    }
                },
            };
            if let Err(err) = ob.update(
                style,
                HintsBarData {
                    cells: overflow_cells,
                    screen: snap.screen_frame,
                    workspaces: Vec::new(),
                    overflow_split: None,
                },
            ) {
                warn!(?err, "hints_bar: overflow update failed");
            }
        } else if let Some(ob) = &self.overflow_bar {
            if ob.is_visible() {
                let _ = ob.hide();
            }
        }

        self.sync_hit_rects();
    }

    fn hide(&mut self) {
        if let Some(bar) = &self.bar {
            if bar.is_visible() {
                if let Err(err) = bar.hide() {
                    warn!(?err, "hints_bar: hide failed");
                }
            }
        }
        if let Some(ob) = &self.overflow_bar {
            if ob.is_visible() {
                let _ = ob.hide();
            }
        }
        self.sync_hit_rects();
    }

    fn teardown(&mut self) {
        self.hide();
        self.bar = None;
        self.overflow_bar = None;
        self.auto_pending = false;
        self.pending_arm = None;
        self.shared_hit_rects.store(Arc::new(Vec::new()));
    }

    fn sync_hit_rects(&self) {
        let mut rects = Vec::new();
        if let Some(bar) = &self.bar {
            if bar.is_visible() {
                rects.push(bar.frame());
            }
        }
        if let Some(ob) = &self.overflow_bar {
            if ob.is_visible() {
                rects.push(ob.frame());
            }
        }
        self.shared_hit_rects.store(Arc::new(rects));
    }

    fn on_mouse_down(&mut self, point: CGPoint) {
        // Main bar: workspace pill first, then a column chip.
        if let Some(bar) = &self.bar {
            if bar.is_visible() && point_hits_indicator_frame(point, bar.frame()) {
                let frame = bar.frame();
                let local = CGPoint::new(point.x - frame.origin.x, point.y - frame.origin.y);
                if let Some(ws_index) = bar.workspace_at_point(local) {
                    tracing::debug!(ws_index, "hints_bar: switch workspace via click");
                    let _ = self.reactor_tx.send(reactor::Event::Command(Command::Layout(
                        LayoutCommand::SwitchToWorkspace(ws_index),
                    )));
                    return;
                }
                if let Some(index) = bar.column_at_point(local) {
                    if let Some(window_id) = bar.window_id_at(index) {
                        let _ = self.reactor_tx.send(reactor::Event::Command(Command::Reactor(
                            ReactorCommand::FocusWindow {
                                window_id: window_id.into(),
                                window_server_id: None,
                            },
                        )));
                    }
                }
                return;
            }
        }
        // Overflow bar: column chips only (its cells are already the tail slice).
        if let Some(ob) = &self.overflow_bar {
            if ob.is_visible() && point_hits_indicator_frame(point, ob.frame()) {
                let frame = ob.frame();
                let local = CGPoint::new(point.x - frame.origin.x, point.y - frame.origin.y);
                if let Some(index) = ob.column_at_point(local) {
                    if let Some(window_id) = ob.window_id_at(index) {
                        let _ = self.reactor_tx.send(reactor::Event::Command(Command::Reactor(
                            ReactorCommand::FocusWindow {
                                window_id: window_id.into(),
                                window_server_id: None,
                            },
                        )));
                    }
                }
            }
        }
    }
}
