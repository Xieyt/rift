use objc2_core_foundation::{CGPoint, CGRect, CGSize};
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
use crate::model::broadcast::{BroadcastEvent, BroadcastSender, StackInfo};
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

/// Build the scrolling strip cells: group the active workspace's tiled windows
/// into columns by x-origin (ordered left -> right), then decorate each with a
/// hint letter, its app, focus, and whether it lies inside the screen viewport.
fn build_hints_bar_cells(
    reactor: &Reactor,
    layout: &[(WindowId, CGRect)],
    screen: CGRect,
    hb: &crate::common::config::HintsBarSettings,
    groups: &[crate::layout_engine::engine::GroupContainerInfo],
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

    // BTreeMap keeps columns ordered left -> right by rounded x-origin.
    let mut columns: std::collections::BTreeMap<i64, Vec<(WindowId, CGRect)>> =
        std::collections::BTreeMap::new();
    for (wid, frame) in layout {
        if reactor.layout_manager.layout_engine.is_window_floating(*wid) {
            continue;
        }
        columns.entry(frame.origin.x.round() as i64).or_default().push((*wid, *frame));
    }

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
    workspace: &str,
    frame: CGRect,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    workspace.hash(&mut h);
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
    h.finish()
}

impl LayoutManager {
    pub fn update_layout(
        reactor: &mut Reactor,
        is_resize: bool,
        is_workspace_switch: bool,
    ) -> Result<bool, crate::model::reactor::ReactorError> {
        let layout_result = Self::calculate_layout(reactor);
        Self::apply_layout(reactor, layout_result, is_resize, is_workspace_switch)
    }

    fn calculate_layout(reactor: &mut Reactor) -> LayoutResult {
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
            // Reserve strip space for the hint bar (scrolling only): shrink the
            // tiling frame so tiled windows never sit under the bar.
            let reserve = reactor.config.settings.ui.hints_bar.reserved_thickness();
            let tiling_frame = if reserve > 0.0
                && reactor.layout_manager.layout_engine.active_layout_mode_at(space)
                    == LayoutMode::Scrolling
            {
                let mut f = screen.frame;
                f.size.height = (f.size.height - reserve).max(1.0);
                if matches!(
                    reactor.config.settings.ui.hints_bar.position,
                    crate::common::config::HintsBarPosition::Top
                ) {
                    f.origin.y += reserve;
                }
                f
            } else {
                screen.frame
            };
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
                            build_hints_bar_cells(reactor, &layout, screen_frame, hb, &group_infos)
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
                        let bar_frame = match hb.position {
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
                        let workspace = if show_workspace {
                            reactor
                                .layout_manager
                                .layout_engine
                                .active_workspace_idx(space)
                                .map(|i| (i + 1).to_string())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };

                        // Skip redundant sends: nothing downstream changes when the
                        // signature matches the last one we pushed for this space.
                        let sig = hints_bar_sig(&cells, &workspace, bar_frame);
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
                                workspace,
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
                            container_kind: g.container_kind,
                            total_count: g.total_count,
                            selected_index: g.selected_index,
                            windows: g.window_ids.iter().map(WindowId::to_debug_string).collect(),
                        })
                        .collect();

                    if stacks.len() > 0 {
                        let event = BroadcastEvent::StacksChanged {
                            workspace_id,
                            workspace_index,
                            workspace_name,
                            stacks,
                            active_workspace_has_fullscreen:
                                active_workspace_for_space_has_fullscreen,
                            space_id: space,
                            display_uuid,
                        };
                        let _ = reactor.communication_manager.event_broadcaster.send(event);
                    }
                }
            }

            let suppress_animation = is_workspace_switch
                || reactor.workspace_switch_manager.active_workspace_switch.is_some();
            if suppress_animation {
                any_frame_changed |=
                    AnimationManager::instant_layout(reactor, space, &layout, skip_wid);
            } else {
                any_frame_changed |=
                    AnimationManager::animate_layout(reactor, space, &layout, is_resize, skip_wid);
            }
        }

        reactor.maybe_send_menu_update();
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

    use super::bound_frame_to_screen;

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
}
