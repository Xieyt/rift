use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use rift_protocol::StackInfo;
use tracing::trace;

use super::replay::Record;
use super::{AppState, Event, WorkspaceSwitchOrigin, WorkspaceSwitchState};
use crate::actor;
use crate::actor::app::{WindowId, pid_t};
use crate::actor::drag_swap::DragManager as DragSwapManager;
use crate::actor::reactor::Reactor;
use crate::actor::reactor::animation::AnimationManager;
use crate::actor::spaces::ForwardedSpaceState;
use crate::actor::{
    event_tap, gesture_tap, hints_bar, menu_bar, raise_manager, stack_line, window_notify,
    wm_controller,
};
use crate::common::collections::{HashMap, HashSet};
use crate::common::config::{LayoutMode, WindowSnappingSettings};
use crate::layout_engine::LayoutEngine;
use crate::model::broadcast::{BroadcastEvent, BroadcastSender, protocol_workspace_id};
use crate::sys::screen::SpaceId;

/// Manages application state and rules
pub struct AppManager {
    pub apps: HashMap<pid_t, AppState>,
}

impl AppManager {
    pub fn new() -> Self { AppManager { apps: HashMap::default() } }
}

/// Manages drag operations and window swapping
pub struct DragManager {
    pub drag_state: super::DragState,
    pub drag_swap_manager: DragSwapManager,
    pub skip_layout_for_window: Option<WindowId>,
}

impl DragManager {
    pub fn reset(&mut self) { self.drag_swap_manager.reset(); }

    pub fn last_target(&self) -> Option<WindowId> { self.drag_swap_manager.last_target() }

    pub fn dragged(&self) -> Option<WindowId> { self.drag_swap_manager.dragged() }

    pub fn origin_frame(&self) -> Option<CGRect> { self.drag_swap_manager.origin_frame() }

    pub fn update_config(&mut self, config: WindowSnappingSettings) {
        self.drag_swap_manager.update_config(config);
    }
}

/// Manages window notifications
pub struct NotificationManager {
    pub last_sls_notification_ids: Vec<u32>,
    pub last_layout_modes_by_space: HashMap<SpaceId, crate::common::config::LayoutMode>,
    pub _window_notify_tx: Option<window_notify::Sender>,
}

/// Manages menu state and interactions
pub struct MenuManager {
    pub menu_state: super::MenuState,
    pub menu_tx: Option<menu_bar::Sender>,
}

/// Manages Mission Control state
pub struct MissionControlManager {
    pub mission_control_state: super::MissionControlState,
    pub pending_mission_control_refresh: HashSet<pid_t>,
}

/// Manages workspace switching state
pub struct WorkspaceSwitchManager {
    pub workspace_switch_state: super::WorkspaceSwitchState,
    pub workspace_switch_generation: u64,
    pub active_workspace_switch: Option<u64>,
    pub pending_workspace_switch_origin: Option<WorkspaceSwitchOrigin>,
    pub pending_workspace_mouse_warp: Option<WindowId>,
}

impl WorkspaceSwitchManager {
    pub fn start_workspace_switch(&mut self, origin: WorkspaceSwitchOrigin) {
        self.workspace_switch_generation = self.workspace_switch_generation.wrapping_add(1);
        self.active_workspace_switch = Some(self.workspace_switch_generation);
        self.workspace_switch_state = WorkspaceSwitchState::Active;
        self.pending_workspace_switch_origin = Some(origin);
    }

    pub fn manual_switch_in_progress(&self) -> bool {
        self.workspace_switch_state == WorkspaceSwitchState::Active
            && self.pending_workspace_switch_origin == Some(WorkspaceSwitchOrigin::Manual)
    }

    pub fn mark_workspace_switch_inactive(&mut self) {
        self.workspace_switch_state = WorkspaceSwitchState::Inactive;
        self.pending_workspace_switch_origin = None;
    }
}

/// Manages refocus and cleanup state
pub struct RefocusManager {
    pub stale_cleanup_state: super::StaleCleanupState,
    pub refocus_state: super::RefocusState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshQuarantineState {
    Ready,
    Sleeping,
    SessionInactive,
    DisplayChurn,
}

pub struct RefreshQuarantineManager {
    pub sleeping: bool,
    pub session_inactive: bool,
    pub display_churn_active: bool,
    pub awaiting_post_wake_snapshot: bool,
    pub awaiting_post_session_snapshot: bool,
    pub pending_visible_refresh: bool,
    pub deferred_refresh_tracks_mission_control: bool,
}

impl RefreshQuarantineManager {
    pub fn state(&self) -> RefreshQuarantineState {
        if self.sleeping {
            RefreshQuarantineState::Sleeping
        } else if self.session_inactive {
            RefreshQuarantineState::SessionInactive
        } else if self.display_churn_active {
            RefreshQuarantineState::DisplayChurn
        } else {
            RefreshQuarantineState::Ready
        }
    }

    pub fn blocks_refreshes(&self) -> bool { self.state() != RefreshQuarantineState::Ready }
}

/// Manages communication channels to other actors
pub struct CommunicationManager {
    pub event_tap_tx: Option<event_tap::Sender>,
    pub gesture_tap_tx: Option<gesture_tap::Sender>,
    pub stack_line_tx: Option<stack_line::Sender>,
    pub hints_bar_tx: Option<hints_bar::Sender>,
    pub hints_bar_last_sig: HashMap<SpaceId, u64>,
    /// Extra tiling reserve `(top, bottom)` px per space while the hint bar
    /// overflows (dynamic; only under `reserve` placement).
    pub hints_bar_reserve: HashMap<SpaceId, (f64, f64)>,
    /// Set when `hints_bar_reserve` changed this pass; drives one convergence
    /// layout so tiled windows settle under the new strip geometry.
    pub hints_bar_reserve_dirty: bool,
    /// Guards the convergence re-layout against re-entry.
    pub hints_bar_reserve_converging: bool,
    pub raise_manager_tx: raise_manager::Sender,
    pub event_broadcaster: BroadcastSender,
    pub wm_sender: Option<wm_controller::Sender>,
    pub events_tx: Option<actor::Sender<Event>>,
}

/// Manages recording state
pub struct RecordingManager {
    pub record: Record,
}

/// Manages layout engine state
pub struct LayoutManager {
    pub layout_engine: LayoutEngine,
}

pub type LayoutResult = Vec<(SpaceId, Vec<(WindowId, CGRect)>)>;

fn bound_frame_to_screen(frame: CGRect, screen: CGRect) -> CGRect {
    const WINDOW_HIDDEN_THRESHOLD: f64 = 10.0;

    let screen_left = screen.origin.x;
    let screen_top = screen.origin.y;
    let screen_right = screen.max().x;
    let screen_bottom = screen.max().y;
    let max_y = (screen_bottom - frame.size.height).max(screen_top);
    let x = if frame.max().x <= screen_left {
        screen_left - frame.size.width + WINDOW_HIDDEN_THRESHOLD
    } else if frame.origin.x >= screen_right {
        screen_right - WINDOW_HIDDEN_THRESHOLD
    } else {
        frame.origin.x
    };

    CGRect::new(
        CGPoint::new(x, frame.origin.y.clamp(screen_top, max_y)),
        frame.size,
    )
}

fn bound_scrolling_tiled_frames_to_screen(
    reactor: &Reactor,
    layout: &mut Vec<(WindowId, CGRect)>,
    screen: CGRect,
    active_workspace_windows: &HashSet<WindowId>,
) {
    for (wid, frame) in layout.iter_mut() {
        if !active_workspace_windows.contains(wid)
            || reactor.layout_manager.layout_engine.is_window_floating(*wid)
        {
            continue;
        }
        *frame = bound_frame_to_screen(*frame, screen);
    }
}

/// Group a space's laid-out frames into the scrolling strip's columns, keyed by
/// rounded x-origin so a `BTreeMap` orders them left -> right.
///
/// Two windows are excluded, and the first one is the subtle one:
///
/// 1. **Windows outside the active workspace.** `layout` covers the whole *space*,
///    and the scrolling layout parks every inactive workspace's windows off-screen
///    at a shared x. Without this filter they all collapse into one phantom column
///    that earns its own hint letter and a member-count badge — observed as chip
///    "D" with badge "8" while the active workspace held just two windows.
///    `bound_scrolling_tiled_frames_to_screen` filters on the same set.
/// 2. **Floating windows**, which are not part of the strip at all.
///
/// Split out from `build_hints_bar_cells` so the rule is testable without standing
/// up a whole `Reactor`.
fn scrolling_strip_columns(
    layout: &[(WindowId, CGRect)],
    active_workspace_windows: &HashSet<WindowId>,
    is_floating: impl Fn(WindowId) -> bool,
) -> std::collections::BTreeMap<i64, Vec<(WindowId, CGRect)>> {
    let mut columns: std::collections::BTreeMap<i64, Vec<(WindowId, CGRect)>> =
        std::collections::BTreeMap::new();
    for (wid, frame) in layout {
        if !active_workspace_windows.contains(wid) || is_floating(*wid) {
            continue;
        }
        columns.entry(frame.origin.x.round() as i64).or_default().push((*wid, *frame));
    }
    columns
}

/// Build the scrolling strip cells: group the active workspace's tiled windows
/// into columns by x-origin (ordered left -> right), then decorate each with a
/// hint letter, its app, focus, and whether it lies inside the screen viewport.
fn build_hints_bar_cells(
    reactor: &Reactor,
    layout: &[(WindowId, CGRect)],
    screen: CGRect,
    hb: &crate::common::config::HintsBarSettings,
    groups: &[crate::layout_engine::engine::GroupContainerInfo],
    active_workspace_windows: &HashSet<WindowId>,
) -> Vec<crate::ui::hints_bar::ColumnCell> {
    let focused = reactor.layout_manager.layout_engine.focused_window();
    let resolve = |wid: WindowId| -> (String, String) {
        let label = reactor
            .app_manager
            .apps
            .get(&wid.pid)
            .and_then(|a| a.info.localized_name.clone())
            .unwrap_or_default();
        let title = reactor
            .state
            .windows
            .window(wid)
            .map(|w| w.info.title.clone())
            .unwrap_or_default();
        (label, title)
    };

    let columns = scrolling_strip_columns(layout, active_workspace_windows, |wid| {
        reactor.layout_manager.layout_engine.is_window_floating(wid)
    });

    let keys: Vec<char> = hb.keys.chars().collect();
    let screen_left = screen.origin.x;
    let screen_right = screen.origin.x + screen.size.width;

    let mut cells = Vec::with_capacity(columns.len());
    for (i, (_x, wins)) in columns.into_iter().enumerate() {
        let focused_here = focused.is_some_and(|f| wins.iter().any(|(w, _)| *w == f));

        // A tabbed column shows up in `groups` (from the engine's tab_groups),
        // carrying the full window list + active tab even when the hidden tabs
        // aren't in the visible frame list. Stacks aren't grouped, so fall back
        // to the windows sharing this column's x.
        let group = groups
            .iter()
            .find(|g| g.window_ids.iter().any(|w| wins.iter().any(|(x, _)| x == w)));
        let (member_ids, active, tabbed) = if let Some(g) = group {
            let count = g.window_ids.len().max(1);
            let active = if focused_here {
                g.window_ids
                    .iter()
                    .position(|w| Some(*w) == focused)
                    .unwrap_or(g.selected_index)
            } else {
                g.selected_index
            };
            (g.window_ids.clone(), active.min(count - 1), true)
        } else {
            let ids: Vec<WindowId> = wins.iter().map(|(w, _)| *w).collect();
            let active = if focused_here {
                ids.iter().position(|w| Some(*w) == focused).unwrap_or(0)
            } else {
                0
            };
            (ids, active, false)
        };

        let members: Vec<crate::ui::hints_bar::WindowMember> = member_ids
            .iter()
            .map(|w| {
                let (label, title) = resolve(*w);
                crate::ui::hints_bar::WindowMember { window_id: *w, label, title }
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let active = active.min(members.len() - 1);

        let xmin = wins.iter().map(|(_, f)| f.origin.x).fold(f64::INFINITY, f64::min);
        let xmax = wins
            .iter()
            .map(|(_, f)| f.origin.x + f.size.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let overlap = (xmax.min(screen_right) - xmin.max(screen_left)).max(0.0);
        // On screen if a usable strip is showing; > a parked sliver (~10px),
        // and partial columns (a half-revealed neighbour) still count.
        let visible = overlap > 24.0;

        cells.push(crate::ui::hints_bar::ColumnCell {
            hint: keys.get(i).map(|c| c.to_string()).unwrap_or_default(),
            members,
            active,
            tabbed,
            focused: focused_here,
            visible,
        });
    }
    cells
}

/// Cheap content signature of a hint-bar snapshot, so the reactor can skip
/// re-sending an identical bar (nothing downstream would change).
fn hints_bar_sig(
    cells: &[crate::ui::hints_bar::ColumnCell],
    workspaces: &[crate::ui::hints_bar::WorkspaceCell],
    frame: CGRect,
    top_count: usize,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    frame.origin.x.to_bits().hash(&mut h);
    frame.origin.y.to_bits().hash(&mut h);
    frame.size.width.to_bits().hash(&mut h);
    frame.size.height.to_bits().hash(&mut h);
    for c in cells {
        c.hint.hash(&mut h);
        c.active.hash(&mut h);
        c.tabbed.hash(&mut h);
        c.focused.hash(&mut h);
        c.visible.hash(&mut h);
        for m in &c.members {
            m.window_id.hash(&mut h);
            m.label.hash(&mut h);
            m.title.hash(&mut h);
        }
    }
    for w in workspaces {
        w.label.hash(&mut h);
        w.active.hash(&mut h);
        w.pids.hash(&mut h);
    }
    top_count.hash(&mut h);
    h.finish()
}

impl LayoutManager {
    pub fn update_layout(
        reactor: &mut Reactor,
        is_resize: bool,
        is_workspace_switch: bool,
        space_scope: Option<SpaceId>,
    ) -> Result<bool, crate::model::reactor::ReactorError> {
        reactor.communication_manager.hints_bar_reserve_dirty = false;
        let layout_result = Self::calculate_layout(reactor, space_scope);
        let result = Self::apply_layout(reactor, layout_result, is_resize, is_workspace_switch)?;
        if reactor.communication_manager.hints_bar_reserve_dirty
            && !reactor.communication_manager.hints_bar_reserve_converging
        {
            reactor.communication_manager.hints_bar_reserve_converging = true;
            // Thread the scope through unchanged: a scoped workspace switch must not
            // widen to a full re-layout just because the hint bar re-reserved.
            let converged =
                Self::update_layout(reactor, is_resize, is_workspace_switch, space_scope);
            reactor.communication_manager.hints_bar_reserve_converging = false;
            return converged;
        }
        Ok(result)
    }

    fn calculate_layout(reactor: &mut Reactor, space_scope: Option<SpaceId>) -> LayoutResult {
        if reactor.state.windows.tracked_window_count() == 0 {
            return LayoutResult::new();
        }
        let screens = reactor.space_state.screens.clone();
        let all_screen_frames: Vec<CGRect> = screens.iter().map(|s| s.frame).collect();
        let active_space_count = screens
            .iter()
            .filter_map(|screen| screen.space)
            .filter(|space| reactor.is_space_active(*space))
            .count();
        let mut layout_result = LayoutResult::new();

        for screen in screens {
            let Some(space) = screen.space else {
                continue;
            };
            if space_scope.is_some_and(|scope| scope != space) {
                continue;
            }
            if !reactor.is_space_active(space) {
                continue;
            }
            let display_uuid_opt = screen.display_uuid_owned();
            let gaps = reactor
                .config
                .settings
                .layout
                .gaps
                .effective_for_display(display_uuid_opt.as_deref());
            reactor
                .layout_manager
                .layout_engine
                .update_space_display(space, display_uuid_opt.clone());
            // Reserve space so tiled windows never sit under bars: `external_bar`
            // insets (all layouts, e.g. sketchybar) plus the hint bar's own strip
            // (scrolling + reserve placement only).
            let ext_top = reactor.config.settings.layout.external_bar_top.max(0.0);
            let ext_bottom = reactor.config.settings.layout.external_bar_bottom.max(0.0);
            let reserve = reactor.config.settings.ui.hints_bar.reserved_thickness();
            let scrolling = reactor.layout_manager.layout_engine.active_layout_mode_at(space)
                == LayoutMode::Scrolling;
            let mut f = screen.frame;
            f.origin.y += ext_top;
            f.size.height = (f.size.height - ext_top - ext_bottom).max(1.0);
            if reserve > 0.0 && scrolling {
                f.size.height = (f.size.height - reserve).max(1.0);
                if matches!(
                    reactor.config.settings.ui.hints_bar.position,
                    crate::common::config::HintsBarPosition::Top
                ) {
                    f.origin.y += reserve;
                }
            }
            // Dynamic overflow reserve: an extra strip per edge while the active
            // space's hint bar overflows (row = taller top; bar = opposite edge).
            let (extra_top, extra_bottom) = if reactor.workspace_command_space() == Some(space) {
                reactor
                    .communication_manager
                    .hints_bar_reserve
                    .get(&space)
                    .copied()
                    .unwrap_or((0.0, 0.0))
            } else {
                (0.0, 0.0)
            };
            if extra_top > 0.0 {
                f.origin.y += extra_top;
                f.size.height = (f.size.height - extra_top).max(1.0);
            }
            if extra_bottom > 0.0 {
                f.size.height = (f.size.height - extra_bottom).max(1.0);
            }
            let tiling_frame = f;
            let mut layout =
                reactor.layout_manager.layout_engine.calculate_layout_with_virtual_workspaces(
                    &reactor.state.windows,
                    space,
                    tiling_frame,
                    &gaps,
                    reactor.config.settings.ui.stack_line.thickness(),
                    reactor.config.settings.ui.stack_line.horiz_placement,
                    reactor.config.settings.ui.stack_line.vert_placement,
                    |wid| reactor.state.windows.window(wid).map(|w| w.frame_monotonic),
                    &all_screen_frames,
                );
            if active_space_count > 1
                && reactor.layout_manager.layout_engine.active_layout_mode_at(space)
                    == LayoutMode::Scrolling
            {
                let active_workspace_windows: HashSet<WindowId> = reactor
                    .layout_manager
                    .layout_engine
                    .windows_in_active_workspace(&reactor.state.windows, space)
                    .into_iter()
                    .collect();
                bound_scrolling_tiled_frames_to_screen(
                    reactor,
                    &mut layout,
                    screen.frame,
                    &active_workspace_windows,
                );
            }
            layout_result.push((space, layout));
        }

        layout_result
    }

    fn apply_layout(
        reactor: &mut Reactor,
        layout_result: LayoutResult,
        is_resize: bool,
        is_workspace_switch: bool,
    ) -> Result<bool, crate::model::reactor::ReactorError> {
        let main_window = reactor.main_window();
        trace!(?main_window);
        let skip_wid = reactor
            .drag_manager
            .skip_layout_for_window
            .take()
            .or(reactor.drag_manager.drag_swap_manager.dragged());
        let mut any_frame_changed = false;

        let active_space = reactor.workspace_command_space();
        for (space, layout) in layout_result {
            if let Some(screen) = reactor.space_state.screen_by_space(space) {
                let screen_frame = screen.frame;
                let display_uuid = screen.display_uuid_owned();
                let gaps = reactor
                    .config
                    .settings
                    .layout
                    .gaps
                    .effective_for_display(display_uuid.as_deref());
                let active_workspace_for_space_has_fullscreen = active_space == Some(space)
                    && reactor
                        .layout_manager
                        .layout_engine
                        .active_workspace_for_space_has_fullscreen(space);
                let group_infos = reactor.layout_manager.layout_engine.collect_group_containers(
                    space,
                    screen_frame,
                    &gaps,
                    reactor.config.settings.ui.stack_line.thickness(),
                    reactor.config.settings.ui.stack_line.horiz_placement,
                    reactor.config.settings.ui.stack_line.vert_placement,
                );

                // Keep internal stack-line UI actor fed from the same group snapshot.
                if reactor.config.settings.ui.stack_line.enabled
                    && let Some(tx) = &reactor.communication_manager.stack_line_tx
                {
                    let groups: Vec<crate::actor::stack_line::GroupInfo> = group_infos
                        .iter()
                        .map(|g| crate::actor::stack_line::GroupInfo {
                            node_id: g.node_id,
                            space_id: space,
                            container_kind: g.container_kind,
                            frame: g.frame,
                            total_count: g.total_count,
                            selected_index: g.selected_index,
                            window_ids: g.window_ids.clone(),
                            titles: g
                                .window_ids
                                .iter()
                                .map(|wid| {
                                    reactor
                                        .state
                                        .windows
                                        .window(*wid)
                                        .map(|w| w.info.title.clone())
                                        .unwrap_or_default()
                                })
                                .collect(),
                        })
                        .collect();
                    tracing::debug!(
                        "stack_line feed: {} group(s) for space {:?}",
                        groups.len(),
                        space
                    );
                    let active_space_ids: Vec<crate::sys::screen::SpaceId> =
                        reactor.iter_active_spaces().collect();

                    if let Err(e) = tx.try_send(crate::actor::stack_line::Event::GroupsUpdated {
                        active_space_ids,
                        space_id: space,
                        groups,
                        active_workspace_for_space_has_fullscreen,
                    }) {
                        tracing::warn!("Failed to send groups update to stack_line: {}", e);
                    }
                }

                // Feed the scrolling-strip hint bar for the active display space.
                if reactor.config.settings.ui.hints_bar.enabled && active_space == Some(space) {
                    if let Some(tx) = reactor.communication_manager.hints_bar_tx.clone() {
                        let is_scrolling =
                            reactor.layout_manager.layout_engine.active_layout_mode_at(space)
                                == LayoutMode::Scrolling;
                        let cells = if is_scrolling {
                            let hb = &reactor.config.settings.ui.hints_bar;
                            // Computed here rather than hoisted: only the scrolling
                            // strip needs it, and only when the bar is enabled.
                            let active_ws: HashSet<WindowId> = reactor
                                .layout_manager
                                .layout_engine
                                .windows_in_active_workspace(&reactor.state.windows, space)
                                .into_iter()
                                .collect();
                            build_hints_bar_cells(
                                reactor,
                                &layout,
                                screen_frame,
                                hb,
                                &group_infos,
                                &active_ws,
                            )
                        } else {
                            Vec::new()
                        };

                        use crate::common::config::HintsBarPosition as HbPos;
                        let hb = &reactor.config.settings.ui.hints_bar;
                        let h = hb.height.max(1.0);
                        let show_workspace = hb.show_workspace;
                        let p = hb.pad;
                        let sx = screen_frame.origin.x;
                        let sy = screen_frame.origin.y;
                        let sw = screen_frame.size.width;
                        let sh = screen_frame.size.height;
                        let mut bar_frame = match hb.position {
                            HbPos::Bottom => CGRect::new(
                                CGPoint::new(sx + p.left, sy + sh - h - p.bottom),
                                CGSize::new((sw - p.left - p.right).max(1.0), h),
                            ),
                            HbPos::Top => CGRect::new(
                                CGPoint::new(sx + p.left, sy + p.top),
                                CGSize::new((sw - p.left - p.right).max(1.0), h),
                            ),
                            HbPos::Right | HbPos::Left => {
                                let w = (sw * 0.35).clamp(160.0, 360.0);
                                let x = if matches!(hb.position, HbPos::Right) {
                                    sx + sw - w - p.right
                                } else {
                                    sx + p.left
                                };
                                CGRect::new(
                                    CGPoint::new(x, sy + p.top),
                                    CGSize::new(w, (sh - p.top - p.bottom).max(1.0)),
                                )
                            }
                        };
                        let workspaces: Vec<crate::ui::hints_bar::WorkspaceCell> = if show_workspace
                        {
                            reactor
                                .layout_manager
                                .layout_engine
                                .workspace_app_summaries(&reactor.state.windows, space)
                                .into_iter()
                                .filter(|(_, _, _, pids)| !pids.is_empty())
                                .map(|(idx, _name, active, pids)| {
                                    crate::ui::hints_bar::WorkspaceCell {
                                        label: (idx + 1).to_string(),
                                        index: idx,
                                        active,
                                        pids,
                                    }
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };

                        // Overflow plan: cap the top strip at `max_width_ratio`
                        // of the display; wrap the excess per `overflow` mode.
                        use crate::common::config::HintsBarOverflow as HbOverflow;
                        let style = crate::ui::hints_bar::HintsBarStyle::from(hb);
                        let overflow_mode = hb.overflow;
                        let mut top_count = cells.len();
                        let mut overflow_frame: Option<CGRect> = None;
                        if !cells.is_empty()
                            && hb.max_width_ratio < 1.0
                            && !matches!(overflow_mode, HbOverflow::Scale)
                            && !hb.position.is_vertical()
                        {
                            let fs = style
                                .font_size
                                .unwrap_or_else(|| (style.height * 0.5).clamp(11.0, 15.0));
                            let budget = (sw * hb.max_width_ratio).max(1.0);
                            let k = crate::ui::hints_bar::HintsBar::overflow_split(
                                &cells,
                                &workspaces,
                                &style,
                                fs,
                                budget,
                            );
                            if k < cells.len() {
                                top_count = k;
                                match overflow_mode {
                                    HbOverflow::Row => match hb.position {
                                        HbPos::Bottom => {
                                            bar_frame.origin.y -= h;
                                            bar_frame.size.height += h;
                                        }
                                        _ => {
                                            bar_frame.size.height += h;
                                        }
                                    },
                                    HbOverflow::Bar => {
                                        overflow_frame = Some(match hb.position {
                                            HbPos::Top => CGRect::new(
                                                CGPoint::new(sx + p.left, sy + sh - h - p.bottom),
                                                CGSize::new((sw - p.left - p.right).max(1.0), h),
                                            ),
                                            _ => CGRect::new(
                                                CGPoint::new(sx + p.left, sy + p.top),
                                                CGSize::new((sw - p.left - p.right).max(1.0), h),
                                            ),
                                        });
                                    }
                                    HbOverflow::Scale => {}
                                }
                            }
                        }

                        // Dynamic-reserve feedback: record the strip(s) this
                        // overflow needs so the next (forced) layout pass shrinks
                        // the tiling area to match. Only under `reserve` placement.
                        let reserving = hb.placement
                            == crate::common::config::HintsBarPlacement::Reserve
                            && !hb.position.is_vertical();
                        let (want_top, want_bottom) = if reserving && top_count < cells.len() {
                            match overflow_mode {
                                HbOverflow::Row => {
                                    if matches!(hb.position, HbPos::Top) {
                                        (h, 0.0)
                                    } else {
                                        (0.0, h)
                                    }
                                }
                                HbOverflow::Bar => {
                                    if matches!(hb.position, HbPos::Top) {
                                        (0.0, h)
                                    } else {
                                        (h, 0.0)
                                    }
                                }
                                HbOverflow::Scale => (0.0, 0.0),
                            }
                        } else {
                            (0.0, 0.0)
                        };
                        let prev = reactor
                            .communication_manager
                            .hints_bar_reserve
                            .get(&space)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        if (prev.0 - want_top).abs() > 0.5 || (prev.1 - want_bottom).abs() > 0.5 {
                            reactor
                                .communication_manager
                                .hints_bar_reserve
                                .insert(space, (want_top, want_bottom));
                            reactor.communication_manager.hints_bar_reserve_dirty = true;
                        }

                        // Skip redundant sends: nothing downstream changes when the
                        // signature matches the last one we pushed for this space.
                        let sig = hints_bar_sig(&cells, &workspaces, bar_frame, top_count);
                        let unchanged =
                            reactor.communication_manager.hints_bar_last_sig.get(&space)
                                == Some(&sig);
                        if !unchanged {
                            reactor.communication_manager.hints_bar_last_sig.insert(space, sig);
                            tracing::trace!(target: "hbbench", "hb_sent");
                            let _ = tx.try_send(hints_bar::Event::Snapshot(hints_bar::Snapshot {
                                space_id: space,
                                bar_frame,
                                screen_frame,
                                cells,
                                workspaces,
                                overflow: overflow_mode,
                                top_count,
                                overflow_frame,
                            }));
                        }
                    }
                }

                if let Some(workspace_id) =
                    reactor.layout_manager.layout_engine.active_workspace(space)
                {
                    let workspace_index =
                        reactor.layout_manager.layout_engine.active_workspace_idx(space);
                    let workspace_name = reactor
                        .layout_manager
                        .layout_engine
                        .workspace_name(space, workspace_id)
                        .unwrap_or_else(|| format!("Workspace {:?}", workspace_id));

                    let stacks: Vec<StackInfo> = group_infos
                        .iter()
                        .map(|g| StackInfo {
                            container_kind: match g.container_kind {
                                crate::layout_engine::LayoutKind::Horizontal => {
                                    rift_protocol::LayoutKind::Horizontal
                                }
                                crate::layout_engine::LayoutKind::Vertical => {
                                    rift_protocol::LayoutKind::Vertical
                                }
                                crate::layout_engine::LayoutKind::HorizontalStack => {
                                    rift_protocol::LayoutKind::HorizontalStack
                                }
                                crate::layout_engine::LayoutKind::VerticalStack => {
                                    rift_protocol::LayoutKind::VerticalStack
                                }
                            },
                            total_count: g.total_count,
                            selected_index: g.selected_index,
                            windows: g.window_ids.iter().map(WindowId::to_debug_string).collect(),
                        })
                        .collect();

                    if stacks.len() > 0 {
                        let event = BroadcastEvent::StacksChanged {
                            workspace_id: protocol_workspace_id(workspace_id),
                            workspace_index,
                            workspace_name,
                            stacks,
                            active_workspace_has_fullscreen:
                                active_workspace_for_space_has_fullscreen,
                            space_id: space.get(),
                            display_uuid,
                        };
                        let _ = reactor.communication_manager.event_broadcaster.send(event);
                    }
                }
            }

            if is_workspace_switch {
                any_frame_changed |=
                    AnimationManager::workspace_switch_layout(reactor, space, &layout, skip_wid);
            } else if reactor.workspace_switch_manager.active_workspace_switch.is_some() {
                any_frame_changed |=
                    AnimationManager::instant_layout(reactor, space, &layout, skip_wid);
            } else {
                any_frame_changed |=
                    AnimationManager::animate_layout(reactor, space, &layout, is_resize, skip_wid);
            }
        }

        Ok(any_frame_changed)
    }
}

/// Manages pending space changes
pub struct PendingSpaceChangeManager {
    pub pending_space_change: Option<ForwardedSpaceState>,
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    use super::{HashSet, bound_frame_to_screen, scrolling_strip_columns};
    use crate::actor::app::WindowId;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn bound_frame_to_screen_keeps_partial_overlap_for_strip_behavior() {
        let screen = rect(2000.0, 0.0, 1000.0, 800.0);
        let frame = rect(1500.0, 50.0, 700.0, 400.0);
        let bounded = bound_frame_to_screen(frame, screen);
        assert_eq!(bounded.origin.x, 1500.0);
        assert_eq!(bounded.size.width, 700.0);
    }

    #[test]
    fn bound_frame_to_screen_parks_fully_offscreen_windows_to_hidden_sliver() {
        let screen = rect(2000.0, 0.0, 1000.0, 800.0);
        let frame = rect(1200.0, 80.0, 600.0, 300.0);
        let bounded = bound_frame_to_screen(frame, screen);
        assert_eq!(bounded.origin.x, 1410.0);
        assert_eq!(bounded.size.width, 600.0);
    }

    #[test]
    fn bound_frame_to_screen_parks_right_offscreen_windows_to_hidden_sliver() {
        let screen = rect(2000.0, 0.0, 1000.0, 800.0);
        let frame = rect(3200.0, 80.0, 600.0, 300.0);
        let bounded = bound_frame_to_screen(frame, screen);
        assert_eq!(bounded.origin.x, 2990.0);
        assert_eq!(bounded.size.width, 600.0);
    }

    #[test]
    fn bound_frame_to_screen_does_not_park_partially_visible_right_windows() {
        let screen = rect(2000.0, 0.0, 1000.0, 800.0);
        let frame = rect(2998.0, 80.0, 600.0, 300.0);
        let bounded = bound_frame_to_screen(frame, screen);
        assert_eq!(bounded.origin.x, 2998.0);
        assert_eq!(bounded.size.width, 600.0);
    }

    /// The hint bar showed a third chip ("D", badge "8") while the active workspace
    /// held two windows: `layout` spans the whole space, and the eight windows parked
    /// off-screen for three *other* workspaces shared one x-origin, so they grouped
    /// into one phantom column with its own hint letter.
    #[test]
    fn strip_columns_exclude_windows_from_other_workspaces() {
        let active_a = WindowId::new(1, 1);
        let active_b = WindowId::new(1, 2);
        // Eight parked windows from other workspaces, all at the same x — this is
        // what the scrolling layout does with inactive workspaces.
        let parked: Vec<WindowId> = (10..18).map(|i| WindowId::new(2, i)).collect();

        let mut layout = vec![
            (active_a, rect(10.0, 38.0, 1183.0, 1064.0)),
            (active_b, rect(1203.0, 38.0, 1183.0, 1064.0)),
        ];
        layout.extend(parked.iter().map(|w| (*w, rect(-4255.0, 38.0, 1183.0, 1064.0))));

        let active: HashSet<WindowId> = [active_a, active_b].into_iter().collect();
        let columns = scrolling_strip_columns(&layout, &active, |_| false);

        assert_eq!(columns.len(), 2, "parked windows must not add a phantom column");
        assert_eq!(
            columns.keys().copied().collect::<Vec<_>>(),
            vec![10, 1203],
            "columns keyed by x, ordered left -> right"
        );
        assert!(
            columns.values().all(|c| c.len() == 1),
            "no column should collect the parked windows as members (the badge \"8\")"
        );
    }

    #[test]
    fn strip_columns_group_a_stack_and_drop_floating() {
        let stacked_a = WindowId::new(1, 1);
        let stacked_b = WindowId::new(1, 2);
        let floater = WindowId::new(1, 3);
        let layout = vec![
            (stacked_a, rect(10.0, 38.0, 1183.0, 527.0)),
            (stacked_b, rect(10.0, 575.0, 1183.0, 527.0)),
            (floater, rect(400.0, 300.0, 600.0, 400.0)),
        ];
        let active: HashSet<WindowId> = [stacked_a, stacked_b, floater].into_iter().collect();

        let columns = scrolling_strip_columns(&layout, &active, |w| w == floater);
        assert_eq!(columns.len(), 1, "a vertical stack is one column");
        assert_eq!(columns[&10].len(), 2, "both stacked windows are members");
        assert!(
            !columns.values().flatten().any(|(w, _)| *w == floater),
            "floating windows are not on the strip"
        );
    }

    /// x-origins are rounded before keying, so sub-pixel drift must not split a column.
    #[test]
    fn strip_columns_round_subpixel_x_into_one_column() {
        let a = WindowId::new(1, 1);
        let b = WindowId::new(1, 2);
        let layout = vec![
            (a, rect(10.2, 38.0, 1183.0, 527.0)),
            (b, rect(9.8, 575.0, 1183.0, 527.0)),
        ];
        let active: HashSet<WindowId> = [a, b].into_iter().collect();
        let columns = scrolling_strip_columns(&layout, &active, |_| false);
        assert_eq!(columns.len(), 1, "10.2 and 9.8 both round to 10");
    }
}
