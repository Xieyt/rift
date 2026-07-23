//! Scrolling-strip hint bar: a horizontal minimap of every column in the
//! active niri-style workspace, rendered as a centered capsule of rounded
//! per-column chips. Each chip carries a home-row hint letter and the column's
//! app; the focused column's chip is filled with the accent colour, the columns
//! currently on screen are grouped under a brighter "viewport" backing, and
//! off-screen columns are dimmed. Clicking a chip focuses that column.
//!
//! Rendered like `ui/stack_line`: a borderless CGS window whose content is a
//! flipped `CALayer` tree, re-flushed on every update.
use std::cell::RefCell;
use std::collections::HashMap;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSStatusWindowLevel};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2_quartz_core::{CALayer, CATextLayer};
use tracing::warn;

use crate::actor::app::{WindowId, pid_t};
use crate::common::config::{HintsBarDensity, HintsBarPosition, HintsBarSettings};
use crate::sys::app::NSRunningApplicationExt;
use crate::sys::cgs_window::{CgsWindow, CgsWindowError};
use crate::ui::common::{render_layer_to_cgs_window, with_disabled_actions};
use crate::ui::stack_line::{Color, point_hits_indicator_frame};

// Chip layout metrics (in points, before any fit-scaling).
const MARGIN: f64 = 8.0; // gap from the bar's left/right edge
const CHIP_GAP: f64 = 6.0; // gap between chips
const V_INSET: f64 = 3.0; // chip top/bottom inset within the bar
const PAD_X: f64 = 9.0; // chip inner horizontal padding
const BADGE_GAP: f64 = 5.0; // gap between hint letter and app label
const MAX_TEXT_W: f64 = 168.0; // cap on the app-label width
const MIN_CHIP_W: f64 = 30.0;
const DOT_CHIP_W: f64 = 26.0;

/// One window inside a scrolling column.
#[derive(Debug, Clone)]
pub struct WindowMember {
    pub window_id: WindowId,
    /// App (localized) name.
    pub label: String,
    /// Window title (shown when titles are enabled / in the hover pill).
    pub title: String,
}

/// One column of the scrolling strip as it should appear on the bar.
#[derive(Debug, Clone)]
pub struct ColumnCell {
    /// Home-row hint letter (may be empty when keys run out).
    pub hint: String,
    /// Windows in this column, in stack/tab order.
    pub members: Vec<WindowMember>,
    /// Index of the active window within `members`.
    pub active: usize,
    /// True for a niri-style tabbed column (overlapping tabs); false = vertical stack.
    pub tabbed: bool,
    /// The focused column of the workspace.
    pub focused: bool,
    /// Column currently on screen (inside the scrolling viewport).
    pub visible: bool,
}

impl ColumnCell {
    fn active_member(&self) -> Option<&WindowMember> { self.members.get(self.active) }

    pub fn window_id(&self) -> Option<WindowId> { self.active_member().map(|m| m.window_id) }

    fn label(&self) -> &str { self.active_member().map(|m| m.label.as_str()).unwrap_or("") }

    fn title(&self) -> &str { self.active_member().map(|m| m.title.as_str()).unwrap_or("") }
}

/// Full render payload for the bar: the ordered columns of one workspace.
#[derive(Debug, Clone, Default)]
pub struct HintsBarData {
    pub cells: Vec<ColumnCell>,
}

/// Resolved visual style, derived from `HintsBarSettings`.
#[derive(Debug, Clone, Copy)]
pub struct HintsBarStyle {
    pub density: HintsBarDensity,
    pub height: f64,
    pub font_size: Option<f64>,
    pub background: Color,
    pub focused_color: Color,
    pub viewport_color: Color,
    pub label_color: Color,
    pub hint_color: Color,
    pub position: HintsBarPosition,
    pub blur: u32,
}

impl Default for HintsBarStyle {
    fn default() -> Self {
        Self {
            density: HintsBarDensity::Compact,
            height: 26.0,
            font_size: None,
            background: Color::new(0.11, 0.11, 0.13, 0.94),
            focused_color: Color::new(0.24, 0.48, 0.98, 0.95),
            viewport_color: Color::new(1.0, 1.0, 1.0, 0.09),
            label_color: Color::new(0.95, 0.95, 0.97, 1.0),
            hint_color: Color::new(1.0, 0.82, 0.44, 1.0),
            position: HintsBarPosition::Bottom,
            blur: 0,
        }
    }
}

impl From<&HintsBarSettings> for HintsBarStyle {
    fn from(c: &HintsBarSettings) -> Self {
        let d = Self::default();
        let hex = |s: &Option<String>, fallback: Color| {
            s.as_deref().and_then(Color::from_hex).unwrap_or(fallback)
        };
        Self {
            density: c.density,
            height: c.height,
            font_size: c.font_size,
            background: hex(&c.background, d.background),
            focused_color: hex(&c.focused_color, d.focused_color),
            viewport_color: hex(&c.viewport_color, d.viewport_color),
            label_color: hex(&c.label_color, d.label_color),
            hint_color: hex(&c.hint_color, d.hint_color),
            position: c.position,
            blur: c.blur,
        }
    }
}

struct BarState {
    style: HintsBarStyle,
    data: HintsBarData,
    visible: bool,
    /// Chip frames from the last render, used for click hit-testing.
    chip_rects: Vec<CGRect>,
}

pub struct HintsBar {
    frame: RefCell<CGRect>,
    root_layer: Retained<CALayer>,
    cgs_window: CgsWindow,
    state: RefCell<BarState>,
    /// Display backing scale (2.0 on Retina) — drives crisp rasterization.
    scale: f64,
    /// Detail popover shown above the focused multi-window column.
    pill: RefCell<Option<PillWindow>>,
}

impl HintsBar {
    pub fn new(frame: CGRect, style: HintsBarStyle, scale: f64) -> Result<Self, CgsWindowError> {
        let root_layer = CALayer::layer();
        // CGS content renders bottom-left origin; flip so text draws upright and
        // y grows downward from the top of the bar (matches layout coordinates).
        root_layer.setGeometryFlipped(true);
        root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(frame.size.width, frame.size.height),
        ));
        root_layer.setContentsScale(scale);

        let cgs_window = CgsWindow::new(frame)?;
        let _ = cgs_window.set_resolution(scale);
        if let Err(err) = cgs_window.set_opacity(false) {
            warn!(error=?err, "hints_bar: set_opacity failed");
        }
        if let Err(err) = cgs_window.set_alpha(1.0) {
            warn!(error=?err, "hints_bar: set_alpha failed");
        }
        // Float above tiled app windows so an overlay bar stays visible.
        if let Err(err) = cgs_window.set_level(NSStatusWindowLevel as i32) {
            warn!(error=?err, "hints_bar: set_level failed");
        }
        // Disable the system drop shadow (1 << 3); the capsule draws its own.
        if let Err(err) = cgs_window.set_tags(1 << 3) {
            warn!(error=?err, "hints_bar: set_tags failed");
        }
        if style.blur > 0 {
            let _ = cgs_window.set_blur(style.blur as i32, None);
        }

        Ok(Self {
            frame: RefCell::new(frame),
            root_layer,
            cgs_window,
            state: RefCell::new(BarState {
                style,
                data: HintsBarData::default(),
                visible: false,
                chip_rects: Vec::new(),
            }),
            pill: RefCell::new(None),
            scale,
        })
    }

    pub fn is_visible(&self) -> bool { self.state.borrow().visible }

    pub fn frame(&self) -> CGRect { *self.frame.borrow() }

    pub fn set_frame(&self, frame: CGRect) -> Result<(), CgsWindowError> {
        self.cgs_window.set_shape(frame)?;
        self.root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(frame.size.width, frame.size.height),
        ));
        *self.frame.borrow_mut() = frame;
        Ok(())
    }

    pub fn cells(&self) -> Vec<ColumnCell> { self.state.borrow().data.cells.clone() }

    /// Show `data` with `style`, rebuilding the layer tree and ordering the
    /// window above its neighbours.
    pub fn update(&self, style: HintsBarStyle, data: HintsBarData) -> Result<(), CgsWindowError> {
        {
            let mut state = self.state.borrow_mut();
            state.style = style;
            state.data = data;
            state.visible = true;
        }
        // Apply blur every update so it tracks config hot-reloads (0 = off,
        // plain alpha transparency shows straight through).
        let _ = self.cgs_window.set_blur(style.blur as i32, None);
        self.rebuild_layers();
        self.present();
        let result = self.cgs_window.order_above(None);
        self.update_pill();
        result
    }

    /// Hide the bar without discarding its data.
    pub fn hide(&self) -> Result<(), CgsWindowError> {
        self.state.borrow_mut().visible = false;
        if let Some(pill) = self.pill.borrow().as_ref() {
            pill.hide();
        }
        self.cgs_window.order_out()
    }

    /// Map a window-local point to the column index whose chip contains it.
    pub fn column_at_point(&self, point: CGPoint) -> Option<usize> {
        self.state
            .borrow()
            .chip_rects
            .iter()
            .position(|r| point_hits_indicator_frame(point, *r))
    }

    pub fn window_id_at(&self, index: usize) -> Option<WindowId> {
        self.state.borrow().data.cells.get(index).and_then(|c| c.window_id())
    }

    fn bounds(&self) -> CGRect { CGRect::new(CGPoint::new(0.0, 0.0), self.frame.borrow().size) }

    /// Rough single-line text width for chip sizing (proportional UI font).
    fn approx_text_width(text: &str, font_size: f64) -> f64 {
        text.chars().count() as f64 * font_size * 0.56
    }

    /// Natural (unscaled) chip width for each cell at the given font size,
    /// reserving room for the keycap, the app icon, and the label.
    fn natural_widths(cells: &[ColumnCell], style: &HintsBarStyle, font_size: f64) -> Vec<f64> {
        if matches!(style.density, HintsBarDensity::Dots) {
            return vec![DOT_CHIP_W; cells.len()];
        }
        let keycap_w = font_size + 6.0;
        let icon_w = font_size + 4.0;
        cells
            .iter()
            .map(|c| {
                let hint_w = if c.hint.is_empty() {
                    0.0
                } else {
                    keycap_w + BADGE_GAP
                };
                let text_w = Self::approx_text_width(c.label(), font_size).min(MAX_TEXT_W);
                let badge = if c.members.len() > 1 {
                    font_size + 4.0 + BADGE_GAP
                } else {
                    0.0
                };
                (PAD_X + hint_w + icon_w + BADGE_GAP + text_w + badge + PAD_X).max(MIN_CHIP_W)
            })
            .collect()
    }

    /// Lay out chips left -> right, centered within `bounds`, scaling widths and
    /// gaps down uniformly when the natural row is wider than the bar so every
    /// column stays visible. Pure so it can be unit-tested.
    pub fn layout_chips(bounds: CGRect, widths: &[f64]) -> Vec<CGRect> {
        let n = widths.len();
        if n == 0 {
            return Vec::new();
        }
        let avail = (bounds.size.width - 2.0 * MARGIN).max(1.0);
        let natural: f64 = widths.iter().sum::<f64>() + CHIP_GAP * (n - 1) as f64;
        let scale = if natural > avail {
            avail / natural
        } else {
            1.0
        };
        let gap = CHIP_GAP * scale;
        let scaled: Vec<f64> = widths.iter().map(|w| w * scale).collect();
        let total: f64 = scaled.iter().sum::<f64>() + gap * (n - 1) as f64;
        let mut x = bounds.origin.x + MARGIN + (avail - total) / 2.0;
        let y = bounds.origin.y + V_INSET;
        let h = (bounds.size.height - 2.0 * V_INSET).max(1.0);
        let mut rects = Vec::with_capacity(n);
        for w in scaled {
            rects.push(CGRect::new(CGPoint::new(x, y), CGSize::new(w, h)));
            x += w + gap;
        }
        rects
    }

    /// Vertical variant: uniform-width chips stacked top -> bottom and centered
    /// along the edge (`align_right` hugs the right edge). Pure for testing.
    pub fn layout_chips_vertical(
        bounds: CGRect,
        widths: &[f64],
        row_h: f64,
        align_right: bool,
    ) -> Vec<CGRect> {
        let n = widths.len();
        if n == 0 {
            return Vec::new();
        }
        let max_w = (bounds.size.width - 2.0 * MARGIN).max(1.0);
        let chip_w = widths.iter().cloned().fold(MIN_CHIP_W, f64::max).min(max_w);
        let avail = (bounds.size.height - 2.0 * MARGIN).max(1.0);
        let natural = n as f64 * row_h + CHIP_GAP * (n - 1) as f64;
        let scale = if natural > avail { avail / natural } else { 1.0 };
        let rh = row_h * scale;
        let gap = CHIP_GAP * scale;
        let total = n as f64 * rh + gap * (n - 1) as f64;
        let x = if align_right {
            bounds.origin.x + bounds.size.width - MARGIN - chip_w
        } else {
            bounds.origin.x + MARGIN
        };
        let mut y = bounds.origin.y + MARGIN + (avail - total) / 2.0;
        let mut rects = Vec::with_capacity(n);
        for _ in 0..n {
            rects.push(CGRect::new(CGPoint::new(x, y), CGSize::new(chip_w, rh)));
            y += rh + gap;
        }
        rects
    }

    /// The rounded rect bracketing the contiguous run of visible chips.
    fn viewport_backing(rects: &[CGRect], visible: &[bool], vertical: bool) -> Option<CGRect> {
        let first = visible.iter().position(|v| *v)?;
        let last = visible.iter().rposition(|v| *v)?;
        let l = rects.get(first)?;
        let r = rects.get(last)?;
        let pad = 3.0;
        if vertical {
            Some(CGRect::new(
                CGPoint::new(l.origin.x - pad, l.origin.y - pad),
                CGSize::new(
                    l.size.width + 2.0 * pad,
                    (r.origin.y + r.size.height + pad) - (l.origin.y - pad),
                ),
            ))
        } else {
            Some(CGRect::new(
                CGPoint::new(l.origin.x - pad, l.origin.y - pad),
                CGSize::new(
                    (r.origin.x + r.size.width + pad) - (l.origin.x - pad),
                    l.size.height + 2.0 * pad,
                ),
            ))
        }
    }

    fn present(&self) {
        let frame = *self.frame.borrow();
        render_layer_to_cgs_window(self.cgs_window.id(), frame.size, &self.root_layer);
    }

    fn add_rounded(&self, frame: CGRect, color: Color, radius: f64) -> Retained<CALayer> {
        let layer = CALayer::layer();
        layer.setFrame(frame);
        layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
        layer.setCornerRadius(radius.min(frame.size.height / 2.0).min(frame.size.width / 2.0));
        self.root_layer.addSublayer(&layer);
        layer
    }

    fn rebuild_layers(&self) {
        let mut state = self.state.borrow_mut();
        let style = state.style;
        let bounds = self.bounds();
        let cells = state.data.cells.clone();

        with_disabled_actions(|| {
            unsafe { self.root_layer.setSublayers(None) };

            if cells.is_empty() {
                state.chip_rects.clear();
                return;
            }

            let fs = style.font_size.unwrap_or_else(|| (style.height * 0.5).clamp(11.0, 15.0));
            let widths = Self::natural_widths(&cells, &style, fs);
            let vertical = style.position.is_vertical();
            let rects = if vertical {
                let align_right = matches!(style.position, HintsBarPosition::Right);
                Self::layout_chips_vertical(bounds, &widths, style.height, align_right)
            } else {
                Self::layout_chips(bounds, &widths)
            };
            let dots = matches!(style.density, HintsBarDensity::Dots);

            // Capsule background hugging the chips (a pill regardless of count).
            if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
                let cap_pad = 6.0;
                let cap_rect = if vertical {
                    let x0 = (first.origin.x - cap_pad).max(0.0);
                    let y0 = (first.origin.y - cap_pad).max(0.0);
                    let y1 = (last.origin.y + last.size.height + cap_pad).min(bounds.size.height);
                    CGRect::new(
                        CGPoint::new(x0, y0),
                        CGSize::new(first.size.width + 2.0 * cap_pad, (y1 - y0).max(1.0)),
                    )
                } else {
                    let x0 = (first.origin.x - cap_pad).max(0.0);
                    let x1 = (last.origin.x + last.size.width + cap_pad).min(bounds.size.width);
                    CGRect::new(
                        CGPoint::new(x0, 0.0),
                        CGSize::new((x1 - x0).max(1.0), bounds.size.height),
                    )
                };
                let radius = (cap_rect.size.width.min(cap_rect.size.height) * 0.5).min(14.0);
                let cap = self.add_rounded(cap_rect, style.background, radius);
                cap.setShadowColor(Some(&objc2_app_kit::NSColor::blackColor().CGColor()));
                cap.setShadowOpacity(0.22);
                cap.setShadowRadius(4.0);
                cap.setShadowOffset(CGSize::new(0.0, 1.5));
            }

            // Viewport grouping behind the on-screen columns.
            let visible: Vec<bool> = cells.iter().map(|c| c.visible).collect();
            if let Some(vp) = Self::viewport_backing(&rects, &visible, vertical) {
                let r = (vp.size.width.min(vp.size.height) * 0.5).min(10.0);
                self.add_rounded(vp, style.viewport_color, r);
            }

            for (cell, rect) in cells.iter().zip(rects.iter()) {
                if dots {
                    self.render_dot(*rect, cell, &style);
                } else {
                    self.render_chip(*rect, cell, &style, fs);
                }
            }

            state.chip_rects = rects;
        });
    }

    /// `dots` density: a centered pill per column (wider + accent when focused).
    fn render_dot(&self, rect: CGRect, cell: &ColumnCell, style: &HintsBarStyle) {
        let d = (rect.size.height * 0.42).clamp(6.0, 13.0);
        let w = if cell.focused { d * 2.6 } else { d };
        let color = if cell.focused {
            style.focused_color
        } else if cell.visible {
            style.label_color
        } else {
            Color::new(
                style.label_color.r,
                style.label_color.g,
                style.label_color.b,
                0.35,
            )
        };
        self.add_rounded(
            CGRect::new(
                CGPoint::new(
                    rect.origin.x + (rect.size.width - w) / 2.0,
                    rect.origin.y + (rect.size.height - d) / 2.0,
                ),
                CGSize::new(w, d),
            ),
            color,
            d / 2.0,
        );
    }

    /// `compact` / `full` density: focus fill + hint keycap + app icon + label.
    fn render_chip(&self, rect: CGRect, cell: &ColumnCell, style: &HintsBarStyle, fs: f64) {
        if cell.focused {
            self.add_rounded(rect, style.focused_color, rect.size.height * 0.42);
        }

        let dim = !cell.visible && !cell.focused;
        let label_color = if cell.focused {
            Color::new(1.0, 1.0, 1.0, 1.0)
        } else if dim {
            Color::new(
                style.label_color.r,
                style.label_color.g,
                style.label_color.b,
                0.5,
            )
        } else {
            style.label_color
        };
        let hint_color = if cell.focused {
            Color::new(1.0, 1.0, 1.0, 1.0)
        } else {
            style.hint_color
        };

        let icon_sz = (fs + 4.0).min(rect.size.height - 4.0).max(1.0);
        let mut cursor = rect.origin.x + PAD_X;
        if !cell.hint.is_empty() {
            let w = self.render_keycap(
                cursor,
                rect,
                &cell.hint.to_uppercase(),
                fs,
                cell.focused,
                hint_color,
            );
            cursor += w + BADGE_GAP;
        }

        // Compact: active window's icon + name, plus a count badge for multi-window columns.
        let pid = cell.window_id().map(|w| w.pid).unwrap_or(0);
        self.render_icon(cursor, rect, pid, icon_sz, if dim { 0.55 } else { 1.0 });
        cursor += icon_sz + BADGE_GAP;

        let count = cell.members.len();
        let badge_w = if count > 1 { fs + 4.0 } else { 0.0 };
        let label_right = rect.origin.x + rect.size.width
            - PAD_X
            - if badge_w > 0.0 {
                badge_w + BADGE_GAP
            } else {
                0.0
            };
        let text_w = (label_right - cursor).max(0.0);
        let full = matches!(style.density, HintsBarDensity::Full) && !cell.title().is_empty();
        if full {
            let half = rect.size.height / 2.0;
            self.root_layer.addSublayer(&self.text_layer(
                cell.label(),
                CGRect::new(CGPoint::new(cursor, rect.origin.y), CGSize::new(text_w, half)),
                (fs - 1.0).max(9.0),
                label_color,
                false,
            ));
            let sub = Color::new(label_color.r, label_color.g, label_color.b, label_color.a * 0.62);
            self.root_layer.addSublayer(&self.text_layer(
                cell.title(),
                CGRect::new(
                    CGPoint::new(cursor, rect.origin.y + half),
                    CGSize::new(text_w, half),
                ),
                (fs - 3.0).max(8.0),
                sub,
                false,
            ));
        } else {
            self.root_layer.addSublayer(&self.text_layer(
                cell.label(),
                CGRect::new(
                    CGPoint::new(cursor, rect.origin.y),
                    CGSize::new(text_w, rect.size.height),
                ),
                fs,
                label_color,
                false,
            ));
        }

        if count > 1 {
            let bx = rect.origin.x + rect.size.width - PAD_X - badge_w;
            self.render_count_badge(bx, rect, count, cell.focused, style, fs, badge_w);
        }
    }

    /// Small pill showing the window count of a multi-window column.
    fn render_count_badge(
        &self,
        x: f64,
        rect: CGRect,
        count: usize,
        focused: bool,
        style: &HintsBarStyle,
        fs: f64,
        w: f64,
    ) {
        let h = (rect.size.height - 8.0).clamp(10.0, fs + 6.0);
        let cap = CGRect::new(
            CGPoint::new(x, rect.origin.y + (rect.size.height - h) / 2.0),
            CGSize::new(w, h),
        );
        let bg = if focused {
            Color::new(1.0, 1.0, 1.0, 0.30)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.16)
        };
        self.add_rounded(cap, bg, 3.0);
        let tc = if focused {
            Color::new(1.0, 1.0, 1.0, 1.0)
        } else {
            style.label_color
        };
        self.root_layer
            .addSublayer(&self.text_layer(&count.to_string(), cap, fs * 0.78, tc, true));
    }

    /// Draw a small keycap (rounded backing + centered letter); returns its width.
    fn render_keycap(
        &self,
        x: f64,
        rect: CGRect,
        letter: &str,
        fs: f64,
        focused: bool,
        text_color: Color,
    ) -> f64 {
        let w = fs + 6.0;
        let h = (rect.size.height - 6.0).clamp(10.0, fs + 9.0);
        let cap = CGRect::new(
            CGPoint::new(x, rect.origin.y + (rect.size.height - h) / 2.0),
            CGSize::new(w, h),
        );
        let bg = if focused {
            Color::new(1.0, 1.0, 1.0, 0.28)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.13)
        };
        self.add_rounded(cap, bg, 3.5);
        self.root_layer
            .addSublayer(&self.text_layer(letter, cap, fs * 0.86, text_color, true));
        w
    }

    /// Draw the app icon for `pid` as a square at `x`, vertically centered.
    fn render_icon(&self, x: f64, rect: CGRect, pid: pid_t, sz: f64, opacity: f32) {
        let frame = CGRect::new(
            CGPoint::new(x, rect.origin.y + (rect.size.height - sz) / 2.0),
            CGSize::new(sz, sz),
        );
        if let Some(layer) = make_icon_layer(pid, frame, opacity) {
            self.root_layer.addSublayer(&layer);
        }
    }

    fn text_layer(
        &self,
        text: &str,
        frame: CGRect,
        font_size: f64,
        color: Color,
        centered: bool,
    ) -> Retained<CATextLayer> {
        let layer = CATextLayer::layer();
        let line_h = font_size * 1.3;
        let y = frame.origin.y + (frame.size.height - line_h) / 2.0;
        layer.setFrame(CGRect::new(
            CGPoint::new(frame.origin.x, y.max(frame.origin.y)),
            CGSize::new(frame.size.width, line_h.min(frame.size.height)),
        ));
        let ns = NSString::from_str(text);
        unsafe { layer.setString(Some(&ns)) };
        layer.setFontSize(font_size);
        layer.setWrapped(false);
        layer.setAlignmentMode(unsafe {
            if centered {
                objc2_quartz_core::kCAAlignmentCenter
            } else {
                objc2_quartz_core::kCAAlignmentLeft
            }
        });
        layer.setTruncationMode(unsafe { objc2_quartz_core::kCATruncationEnd });
        layer.setContentsScale(2.0);
        layer.setForegroundColor(Some(&color.to_nscolor().CGColor()));
        layer.setZPosition(10.0);
        layer
    }

    /// Show/refresh (or hide) the detail pill above the focused multi-window
    /// column, listing each window's icon + name with the active one filled.
    fn update_pill(&self) {
        let picked = {
            let st = self.state.borrow();
            let style = st.style;
            st.data
                .cells
                .iter()
                .enumerate()
                .find(|(_, c)| c.focused && c.members.len() > 1)
                .and_then(|(i, c)| st.chip_rects.get(i).map(|r| (*r, c.clone(), style)))
        };
        let Some((chip, cell, style)) = picked else {
            if let Some(pill) = self.pill.borrow().as_ref() {
                pill.hide();
            }
            return;
        };

        let bar = *self.frame.borrow();
        let fs = style.font_size.unwrap_or_else(|| (style.height * 0.5).clamp(11.0, 15.0));
        let pad = 6.0;
        let row_h = fs + 12.0;
        let icon_sz = fs + 2.0;
        let name_w = cell
            .members
            .iter()
            .map(|m| Self::approx_text_width(&m.label, fs))
            .fold(0.0_f64, f64::max)
            .min(300.0);
        let pill_w = (pad + 4.0 + icon_sz + 6.0 + name_w + pad).clamp(150.0, 380.0);
        let pill_h = cell.members.len() as f64 * row_h + 2.0 * pad;
        let (px, py) = if style.position.is_vertical() {
            // Pill beside the chip: left of a right-edge bar, right of a left-edge bar.
            let chip_x = bar.origin.x + chip.origin.x;
            let cy = bar.origin.y + chip.origin.y + chip.size.height / 2.0;
            let py = (cy - pill_h / 2.0).max(0.0);
            let px = if matches!(style.position, HintsBarPosition::Right) {
                (chip_x - pill_w - 6.0).max(0.0)
            } else {
                chip_x + chip.size.width + 6.0
            };
            (px, py)
        } else {
            let cx = bar.origin.x + chip.origin.x + chip.size.width / 2.0;
            let max_x = (bar.origin.x + bar.size.width - pill_w - 4.0).max(bar.origin.x + 4.0);
            let px = (cx - pill_w / 2.0).clamp(bar.origin.x + 4.0, max_x);
            let py = (bar.origin.y - pill_h - 6.0).max(0.0);
            (px, py)
        };
        let frame = CGRect::new(CGPoint::new(px, py), CGSize::new(pill_w, pill_h));

        let mut pill = self.pill.borrow_mut();
        if pill.is_none() {
            match PillWindow::new(frame, self.scale, style.blur) {
                Ok(p) => *pill = Some(p),
                Err(err) => {
                    warn!(?err, "hints_bar: failed to create pill window");
                    return;
                }
            }
        }
        if let Some(p) = pill.as_ref() {
            p.show(frame, &cell, &style, fs);
        }
    }
}

fn make_rounded_layer(frame: CGRect, color: Color, radius: f64) -> Retained<CALayer> {
    let l = CALayer::layer();
    l.setFrame(frame);
    l.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
    l.setCornerRadius(radius.min(frame.size.height / 2.0).min(frame.size.width / 2.0));
    l
}

fn make_text_layer(
    text: &str,
    frame: CGRect,
    font_size: f64,
    color: Color,
    centered: bool,
) -> Retained<CATextLayer> {
    let layer = CATextLayer::layer();
    let line_h = font_size * 1.3;
    let y = frame.origin.y + (frame.size.height - line_h) / 2.0;
    layer.setFrame(CGRect::new(
        CGPoint::new(frame.origin.x, y.max(frame.origin.y)),
        CGSize::new(frame.size.width, line_h.min(frame.size.height)),
    ));
    let ns = NSString::from_str(text);
    unsafe { layer.setString(Some(&ns)) };
    layer.setFontSize(font_size);
    layer.setWrapped(false);
    layer.setAlignmentMode(unsafe {
        if centered {
            objc2_quartz_core::kCAAlignmentCenter
        } else {
            objc2_quartz_core::kCAAlignmentLeft
        }
    });
    layer.setTruncationMode(unsafe { objc2_quartz_core::kCATruncationEnd });
    layer.setContentsScale(2.0);
    layer.setForegroundColor(Some(&color.to_nscolor().CGColor()));
    layer
}

thread_local! {
    /// App-icon layer contents cached by pid. Resolving an icon
    /// (`NSRunningApplication` + `layerContentsForContentsScale`) is not free,
    /// and every focus move rebuilds the bar — without this cache that
    /// re-rasterized every icon on the main thread each move, making focus feel
    /// clunky. Icons rarely change, so cache them for the process lifetime.
    static ICON_CACHE: RefCell<HashMap<pid_t, Retained<objc2::runtime::AnyObject>>> =
        RefCell::new(HashMap::new());
}

fn make_icon_layer(pid: pid_t, frame: CGRect, opacity: f32) -> Option<Retained<CALayer>> {
    let contents = ICON_CACHE.with(|cache| {
        if let Some(c) = cache.borrow().get(&pid) {
            return Some(c.clone());
        }
        let app = NSRunningApplication::with_process_id(pid)?;
        let image = app.icon()?;
        let contents = image.layerContentsForContentsScale(2.0);
        cache.borrow_mut().insert(pid, contents.clone());
        Some(contents)
    })?;
    let layer = CALayer::layer();
    layer.setFrame(frame);
    layer.setContentsGravity(unsafe { objc2_quartz_core::kCAGravityResizeAspect });
    layer.setContentsScale(2.0);
    if opacity < 0.999 {
        layer.setOpacity(opacity);
    }
    let ptr = &*contents as *const _ as *mut objc2::runtime::AnyObject;
    unsafe {
        let _: () = msg_send![&*layer, setContents: ptr];
    }
    Some(layer)
}

/// A borderless popover above the bar listing a column's windows.
struct PillWindow {
    cgs: CgsWindow,
    root: Retained<CALayer>,
}

impl PillWindow {
    fn new(frame: CGRect, scale: f64, blur: u32) -> Result<Self, CgsWindowError> {
        let root = CALayer::layer();
        root.setGeometryFlipped(true);
        root.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), frame.size));
        root.setContentsScale(scale);
        let cgs = CgsWindow::new(frame)?;
        let _ = cgs.set_resolution(scale);
        let _ = cgs.set_opacity(false);
        let _ = cgs.set_alpha(1.0);
        let _ = cgs.set_level(NSStatusWindowLevel as i32);
        let _ = cgs.set_tags(1 << 3);
        if blur > 0 {
            let _ = cgs.set_blur(blur as i32, None);
        }
        Ok(Self { cgs, root })
    }

    fn hide(&self) { let _ = self.cgs.order_out(); }

    fn show(&self, frame: CGRect, cell: &ColumnCell, style: &HintsBarStyle, fs: f64) {
        let _ = self.cgs.set_shape(frame);
        self.root.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), frame.size));
        let bounds = CGRect::new(CGPoint::new(0.0, 0.0), frame.size);
        let pad = 6.0;
        let row_h = fs + 12.0;
        let icon_sz = fs + 2.0;
        with_disabled_actions(|| {
            unsafe { self.root.setSublayers(None) };
            let bg = make_rounded_layer(bounds, style.background, 10.0);
            bg.setShadowColor(Some(&objc2_app_kit::NSColor::blackColor().CGColor()));
            bg.setShadowOpacity(0.30);
            bg.setShadowRadius(6.0);
            bg.setShadowOffset(CGSize::new(0.0, 2.0));
            self.root.addSublayer(&bg);
            for (i, m) in cell.members.iter().enumerate() {
                let ry = pad + i as f64 * row_h;
                let active = i == cell.active;
                if active {
                    let fill = make_rounded_layer(
                        CGRect::new(
                            CGPoint::new(pad - 2.0, ry),
                            CGSize::new((bounds.size.width - 2.0 * (pad - 2.0)).max(1.0), row_h),
                        ),
                        style.focused_color,
                        6.0,
                    );
                    self.root.addSublayer(&fill);
                }
                let iy = ry + (row_h - icon_sz) / 2.0;
                if let Some(l) = make_icon_layer(
                    m.window_id.pid,
                    CGRect::new(CGPoint::new(pad + 4.0, iy), CGSize::new(icon_sz, icon_sz)),
                    1.0,
                ) {
                    self.root.addSublayer(&l);
                }
                let tx = pad + 4.0 + icon_sz + 6.0;
                let tw = (bounds.size.width - tx - pad).max(0.0);
                let tc = if active {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    style.label_color
                };
                self.root.addSublayer(&make_text_layer(
                    &m.label,
                    CGRect::new(CGPoint::new(tx, ry), CGSize::new(tw, row_h)),
                    fs,
                    tc,
                    false,
                ));
            }
        });
        render_layer_to_cgs_window(self.cgs.id(), frame.size, &self.root);
        let _ = self.cgs.order_above(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(w: f64) -> CGRect { CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w, 26.0)) }

    #[test]
    fn chips_are_ordered_non_overlapping_and_within_bounds() {
        let b = bounds(1710.0);
        let widths = vec![120.0, 90.0, 150.0, 110.0];
        let rects = HintsBar::layout_chips(b, &widths);
        assert_eq!(rects.len(), 4);
        let mut prev_right = 0.0_f64;
        for r in &rects {
            assert!(r.origin.x >= prev_right - 0.01, "chips overlap");
            assert!(r.origin.x >= 0.0 && r.origin.x + r.size.width <= b.size.width + 0.5);
            prev_right = r.origin.x + r.size.width;
        }
    }

    #[test]
    fn few_chips_are_centered_not_stretched() {
        let b = bounds(1710.0);
        let widths = vec![120.0, 120.0];
        let rects = HintsBar::layout_chips(b, &widths);
        // Chips keep their natural width (no full-width stretch).
        assert!((rects[0].size.width - 120.0).abs() < 0.5);
        // Row is centered: left margin ~= right margin.
        let left = rects[0].origin.x;
        let right = b.size.width - (rects[1].origin.x + rects[1].size.width);
        assert!((left - right).abs() < 1.0, "row not centered: {left} vs {right}");
    }

    #[test]
    fn many_chips_scale_down_to_fit() {
        let b = bounds(400.0);
        let widths = vec![160.0; 8]; // 1280 natural >> 400 avail
        let rects = HintsBar::layout_chips(b, &widths);
        let last = rects.last().unwrap();
        assert!(
            last.origin.x + last.size.width <= b.size.width + 0.5,
            "overflow not scaled"
        );
        assert!(rects[0].size.width < 160.0, "chips not scaled down");
    }

    #[test]
    fn click_maps_to_the_chip_under_the_point() {
        let b = bounds(1710.0);
        let widths = vec![120.0, 120.0, 120.0];
        let rects = HintsBar::layout_chips(b, &widths);
        let mid = |r: &CGRect| CGPoint::new(r.origin.x + r.size.width / 2.0, 13.0);
        for (i, r) in rects.iter().enumerate() {
            let hit = rects.iter().position(|c| point_hits_indicator_frame(mid(r), *c));
            assert_eq!(hit, Some(i));
        }
        // A point in the gap before the first chip hits nothing.
        assert!(rects.iter().all(|c| !point_hits_indicator_frame(CGPoint::new(1.0, 13.0), *c)));
    }

    #[test]
    fn viewport_backing_brackets_visible_run() {
        let b = bounds(1710.0);
        let widths = vec![120.0; 6];
        let rects = HintsBar::layout_chips(b, &widths);
        let visible = [false, false, true, true, true, false];
        let vp = HintsBar::viewport_backing(&rects, &visible, false).expect("viewport present");
        assert!(vp.origin.x < rects[2].origin.x);
        assert!(vp.origin.x + vp.size.width > rects[4].origin.x + rects[4].size.width);
        assert!(HintsBar::viewport_backing(&rects, &[false; 6], false).is_none());
    }
}
