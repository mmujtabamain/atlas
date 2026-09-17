//! The workspace's look for the dock engine.
//!
//! Panes are cards: a rounded, bordered box with a gap between neighbours,
//! the way a code editor lays out its groups. The engine's hooks give a skin
//! the tab group's outer frame, its tab bar and its content frame, not one
//! element around all three; so the card is two halves drawn as one — the
//! tab bar takes the top corners and the top border, the content frame the
//! bottom ones — and the gap is the frame's padding, with the area's
//! background showing through. The split divider is drawn as nothing: the
//! resize hit area stays in the middle of the gap, invisible.
//!
//! While a tab is held the gap widens and the cards pull in from the
//! window's edges, both on a spring, so there is room to drop the tab
//! *between* two panes or along the window's edge, and the cards dim; on
//! drop, everything springs back. The cards shrink through the layout alone
//! — their contents keep their size.
//!
//! Tabs carry a close button — on show for the displayed tab, on hover for
//! the others — and a single pane's title bar carries one too. The tab bar's
//! menu is the pane's own commands, then zoom, then close.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::base::spring;
use gpui_kit::component::dock::{
    AnyDrag, BasePanelView, ClosePanel, DockArea, DockAreaRenderer, DockContext, DockSkin, DragPanel, DropIndicator, NodeId, PanelHandle, PanelState, TabGroupContext, TabGroupRenderer, TilesRenderer, ToggleZoom,
};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::base::ResizeHandleContext;
use gpui_kit::component::{
    ActiveTheme as _, AxisExt as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The gap between two cards at rest.
pub const GAP: Pixels = px(8.);
/// The gap while a tab is held: wide enough to drop the tab into.
pub const HELD_GAP: Pixels = px(28.);
/// How far the cards pull in from the area's edges while a tab is held —
/// the room the window-edge drop strips take.
pub const HELD_INSET: Pixels = px(26.);
/// The card's corner radius.
const RADIUS: Pixels = px(8.);
/// How opaque a card is drawn while a tab is held.
pub const HELD_OPACITY: f32 = 0.75;
/// The size of the ghost that follows the pointer while a tab is dragged.
const DRAG_PREVIEW_SIZE: Size<Pixels> = size(px(96.), px(30.));

/// What every group and frame of one area reads: whether a tab is held.
pub struct SkinState {
    area: WeakEntity<DockArea>,
    held: Cell<bool>,
    /// The inset and the gap as last laid out — mid-spring while a tab is
    /// picked up or let go — so the drop zones are laid where the cards are.
    laid_out: Cell<(Pixels, Pixels)>,
    /// The pointer is over one of the workspace's own drop zones (a gap, an
    /// edge strip), which takes the drop: the engine's pane indicator would
    /// promise a second place for it.
    zone_hovered: Cell<bool>,
}

impl SkinState {
    /// Whether a tab is held over this area right now.
    pub fn is_held(&self) -> bool {
        self.held.get()
    }

    /// The gap the cards are heading for.
    pub fn target_gap(&self) -> Pixels {
        if self.held.get() { HELD_GAP } else { GAP }
    }

    /// The inset the area is heading for.
    pub fn target_inset(&self) -> Pixels {
        if self.held.get() { HELD_INSET } else { GAP / 2. }
    }

    /// The opacity the cards are heading for.
    pub fn target_opacity(&self) -> f32 {
        if self.held.get() { HELD_OPACITY } else { 1. }
    }
}

/// The skin: gpui-component's, with the workspace's own frames and tab bar.
pub struct WorkspaceSkin {
    base: Rc<DockSkin>,
    state: Rc<SkinState>,
}

impl WorkspaceSkin {
    /// Builds a [`DockArea`] wearing this skin, and the skin to steer it.
    pub fn dock_area(id: impl Into<SharedString>, version: Option<usize>, window: &mut Window, cx: &mut App) -> (Entity<DockArea>, Rc<Self>) {
        let mut skin = None;
        let area = cx.new(|cx| {
            let this = Rc::new(WorkspaceSkin { base: DockSkin::new(cx), state: Rc::new(SkinState { area: cx.weak_entity(), held: Cell::new(false), laid_out: Cell::new((GAP / 2., GAP)), zone_hovered: Cell::new(false) }) });
            skin = Some(this.clone());
            DockArea::new(id, version, window, cx).with_renderer(this)
        });
        let skin: Rc<Self> = skin.expect("the skin is made inside the area's constructor");
        // There are no side docks to collapse; the affordance would be noise.
        // (Set once the area exists: the setter redraws it.)
        skin.base.set_toggle_button_visible(false, cx);
        (area, skin)
    }

    /// Tells the skin a tab is held (or released): the gap and the inset
    /// head for their held sizes, and the area redraws.
    pub fn set_held(&self, held: bool, cx: &mut App) {
        if self.state.held.replace(held) != held {
            log::debug!("workspace skin: tab {}", if held { "held: gaps widen" } else { "released: gaps close" });
            let _ = self.state.area.update(cx, |_, cx| cx.notify());
        }
    }

    /// Whether a tab is held over this area.
    pub fn is_held(&self) -> bool {
        self.state.is_held()
    }

    /// The area's inset as last drawn.
    pub fn inset(&self) -> Pixels {
        self.state.laid_out.get().0
    }

    /// The gap between cards as last drawn.
    pub fn gap(&self) -> Pixels {
        self.state.laid_out.get().1
    }

    /// Tells the skin whether one of the workspace's drop zones is hovered,
    /// so the engine's own drop indicator stays out of the way meanwhile.
    pub fn set_zone_hovered(&self, hovered: bool) {
        self.state.zone_hovered.set(hovered);
    }

    /// The inset the area is heading for.
    pub fn target_inset(&self) -> Pixels {
        self.state.target_inset()
    }

    /// The gap the cards are heading for.
    pub fn target_gap(&self) -> Pixels {
        self.state.target_gap()
    }
}

impl DockAreaRenderer for WorkspaceSkin {
    /// The area's outer frame: the well the cards sit in. It carries no
    /// padding of its own — the engine measures the area through a child
    /// of this frame, and a child sits inside the padding, so padding here
    /// would put the recorded bounds (which the drop zones are laid on) off
    /// by the inset. The inset goes on the centre frame instead.
    fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let motion = cx.theme().motion_tokens().spring_move;
        let inset = spring(("workspace-inset", "inset"), self.state.target_inset(), motion, window, cx);
        let (_, gap) = self.state.laid_out.get();
        self.state.laid_out.set((inset, gap));
        div().id("dock-area").bg(gap_colour(cx))
    }

    /// The cards pull in from the area's edges while a tab is held, on the
    /// spring the frame advanced this frame.
    fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let (inset, _) = self.state.laid_out.get();
        self.base.center_frame(window, cx).p(inset)
    }

    fn split_frame(&self, node: NodeId, _: Axis, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        div().id(("dock-split-frame", node.as_u64())).bg(gap_colour(cx))
    }

    /// The divider is the gap itself, and shows only when it is used: a
    /// fainter line while the pointer rests on it, the accent colour along
    /// its whole length while it is dragged. A divider drag is known by the
    /// resize cursor it carries; the dragged divider is the one the pointer
    /// is at (the layout keeps it under the pointer as it moves).
    fn render_split_handle(&self, handle: &ResizeHandleContext, _: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let accent = cx.theme().primary;
        let axis = handle.axis();
        let resizing = matches!(cx.active_drag_cursor_style(), Some(CursorStyle::ResizeColumn | CursorStyle::ResizeRow | CursorStyle::ResizeLeftRight | CursorStyle::ResizeUpDown));
        let dragged_line = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if !resizing {
                    return;
                }
                let mouse = window.mouse_position();
                let centre = bounds.center();
                let near = if axis.is_horizontal() { (mouse.x - centre.x).abs() <= px(24.) } else { (mouse.y - centre.y).abs() <= px(24.) };
                if !near {
                    return;
                }
                let line = if axis.is_horizontal() { Bounds::new(point(centre.x - px(1.), bounds.top()), size(px(2.), bounds.size.height)) } else { Bounds::new(point(bounds.left(), centre.y - px(1.)), size(bounds.size.width, px(2.))) };
                window.paint_quad(fill(line, accent));
            },
        )
        .absolute()
        .when(axis.is_horizontal(), |this| this.top_0().left(px(-1.)).w(px(2.)).h_full())
        .when(axis.is_vertical(), |this| this.left_0().top(px(-1.)).h(px(2.)).w_full());
        let line = div()
            .flex_none()
            .relative()
            .rounded(px(1.))
            .when(axis.is_horizontal(), |this| this.h_full().w(px(2.)))
            .when(axis.is_vertical(), |this| this.w_full().h(px(2.)))
            .group_hover("handle", |this| this.bg(accent.opacity(0.55)))
            .child(dragged_line);
        Some(line.into_any_element())
    }

    fn render_dock(&self, dock: &DockContext, content: AnyElement, window: &mut Window, cx: &mut App) -> AnyElement {
        self.base.render_dock(dock, content, window, cx)
    }

    fn build_placeholder(&self, state: &PanelState, window: &mut Window, cx: &mut App) -> Option<Arc<dyn BasePanelView>> {
        self.base.build_placeholder(state, window, cx)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(WorkspaceTabs { base: self.base.tab_group_renderer(), state: self.state.clone(), scroll_handle: ScrollHandle::new(), last_active_ix: Cell::new(None) })
    }

    fn tiles_renderer(&self) -> Rc<dyn TilesRenderer> {
        self.base.tiles_renderer()
    }
}

/// The background between the cards — the well the cards sit in: darker
/// than the panes in both modes, so the cards read as lifted.
fn gap_colour(cx: &App) -> Hsla {
    let theme = cx.theme();
    let depth = if theme.is_dark() { 0.6 } else { 0.06 };
    theme.background.blend(black().opacity(depth))
}

/// One tab group's look: the card halves, the tabs with their close buttons.
struct WorkspaceTabs {
    base: Rc<dyn TabGroupRenderer>,
    state: Rc<SkinState>,
    scroll_handle: ScrollHandle,
    /// The displayed tab the last frame drew, so a change scrolls the new
    /// tab into view.
    last_active_ix: Cell<Option<usize>>,
}

impl TabGroupRenderer for WorkspaceTabs {
    /// The group's slot, padded by half the gap so neighbours are a gap
    /// apart; the actions (zoom, close) are the base skin's.
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let motion = cx.theme().motion_tokens().spring_move;
        let node = group.node().as_u64();
        let half_gap = spring((("pane-gap", node), "gap"), self.state.target_gap() / 2., motion, window, cx);
        let opacity = spring((("pane-opacity", node), "opacity"), self.state.target_opacity(), motion, window, cx);
        let (inset, _) = self.state.laid_out.get();
        self.state.laid_out.set((inset, half_gap * 2.));
        self.base.frame(group, window, cx).bg(gap_colour(cx)).p(half_gap).opacity(opacity)
    }

    /// The card's lower half.
    fn content_frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let (background, border) = (cx.theme().background, cx.theme().border);
        self.base.content_frame(group, window, cx).bg(background).border_1().border_t_0().border_color(border).rounded_b(RADIUS).overflow_hidden()
    }

    fn render_tab_bar(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> AnyElement {
        let visible: Vec<usize> = group.panels().iter().enumerate().filter(|(_, panel)| panel.visible(cx)).map(|(ix, _)| ix).collect();
        match visible.as_slice() {
            [] => Empty.into_any_element(),
            [ix] => self.render_title(group, *ix, window, cx),
            _ => self.render_tabs(group, &visible, window, cx),
        }
    }

    fn render_active_panel(&self, panel: AnyView, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> AnyElement {
        self.base.render_active_panel(panel, group, window, cx)
    }

    fn render_drop_indicator(&self, indicator: DropIndicator, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        if self.state.zone_hovered.get() {
            return None;
        }
        self.base.render_drop_indicator(indicator, window, cx)
    }

    fn render_empty(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.base.render_empty(group, window, cx)
    }
}

impl WorkspaceTabs {
    /// The card's upper half around `bar`: the top corners and border.
    fn bar_shell(&self, cx: &App) -> Div {
        let theme = cx.theme();
        div().w_full().flex_shrink_0().bg(theme.tokens.tab_bar).border_1().border_b_0().border_color(theme.border).rounded_t(RADIUS).overflow_hidden()
    }

    /// One pane, no tabs: its title, draggable, with the close button and the menu.
    ///
    /// Draggable even when it is the window's last pane — the engine sees
    /// nowhere for such a pane to go, but the workspace does: another window,
    /// or a window of its own.
    fn render_title(&self, group: &TabGroupContext, ix: usize, window: &mut Window, cx: &mut App) -> AnyElement {
        let panel = &group.panels()[ix];
        let title = title_of(panel, window, cx);
        let drag = (!group.is_locked()).then(|| group.drag_panel(ix, cx)).flatten();
        let panel_for_ghost = panel.clone();
        let panel_id = panel.panel_id(cx);
        let closable = group.is_closable() && !group.is_collapsed();
        let group_for_close = group.clone();
        self.bar_shell(cx)
            .child(
                h_flex()
                    .h(px(30.))
                    .pl_3()
                    .pr_1()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .id("tab")
                            .flex_1()
                            .min_w_16()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(title)
                            .when_some(drag, |this, drag| {
                                this.on_drag(drag, move |drag, offset, _, cx| {
                                    cx.stop_propagation();
                                    drag.set_drag_offset(offset);
                                    drag.set_preview_size(DRAG_PREVIEW_SIZE);
                                    let panel = panel_for_ghost.clone();
                                    cx.new(|_| TabGhost { panel })
                                })
                            }),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .ml_1()
                            .gap_1()
                            .when(closable, |this| {
                                this.child(Button::new("tab-close").icon(IconName::X).xsmall().ghost().tab_stop(false).tooltip("Close pane").on_click(move |_, window, cx| {
                                    group_for_close.close(panel_id, window, cx);
                                }))
                            })
                            .child(self.render_toolbar(group, window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// The tab bar: one tab per pane, each with a close button (on show for
    /// the displayed tab, on hover for the others), the menu at the end.
    fn render_tabs(&self, group: &TabGroupContext, visible: &[usize], window: &mut Window, cx: &mut App) -> AnyElement {
        let droppable = group.is_droppable();
        let tabs_count = group.panels().len();
        let active_ix = group.active_ix();
        let displayed = group.active_panel().map(|panel| panel.panel_id(cx));
        let displayed_ix = displayed.and_then(|displayed| group.panels().iter().position(|panel| panel.panel_id(cx) == displayed));
        let closable = group.is_closable();
        // Bring a newly displayed tab into view.
        if self.last_active_ix.replace(Some(active_ix)) != Some(active_ix)
            && let Some(visible_ix) = visible.iter().position(|ix| *ix == active_ix)
        {
            self.scroll_handle.scroll_to_item(visible_ix);
        }
        let muted = cx.theme().muted_foreground;
        let tabs: Vec<Tab> = visible
            .iter()
            .map(|&ix| {
                let panel = &group.panels()[ix];
                let panel_id = panel.panel_id(cx);
                let selected = Some(ix) == displayed_ix;
                let drag = (!group.is_locked()).then(|| group.drag_panel(ix, cx)).flatten();
                let panel_for_ghost = panel.clone();
                let group_name = SharedString::from(format!("workspace-tab-{ix}"));
                let group_for_close = group.clone();
                let close = Button::new(SharedString::from(format!("tab-close-{ix}")))
                    .icon(IconName::X)
                    .xsmall()
                    .ghost()
                    .tab_stop(false)
                    .text_color(muted)
                    .tooltip("Close pane")
                    .when(!selected, |this| this.invisible().group_hover(group_name.clone(), |this| this.visible()))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        group_for_close.close(panel_id, window, cx);
                    });
                Tab::new()
                    .group(group_name)
                    .child(title_of(panel, window, cx))
                    .suffix(close)
                    .selected(selected)
                    .on_click({
                        let group = group.clone();
                        move |_, window, cx| group.select_tab(ix, window, cx)
                    })
                    .when_some(drag, |this, drag| {
                        this.on_drag(drag, move |drag, offset, _, cx| {
                            cx.stop_propagation();
                            drag.set_drag_offset(offset);
                            drag.set_preview_size(DRAG_PREVIEW_SIZE);
                            let panel = panel_for_ghost.clone();
                            cx.new(|_| TabGhost { panel })
                        })
                    })
                    .when(droppable, |this| {
                        this.drag_over::<DragPanel>(|this, _, _, cx| this.rounded_l_none().border_l_2().border_r_0().border_color(cx.theme().drag_border))
                            .on_drop({
                                let group = group.clone();
                                move |drag: &DragPanel, window, cx| group.drop_panel(drag.clone(), Some(ix), true, window, cx)
                            })
                            .drag_over::<AnyDrag>(|this, _, _, cx| this.rounded_l_none().border_l_2().border_r_0().border_color(cx.theme().drag_border))
                            .on_drop({
                                let group = group.clone();
                                move |item: &AnyDrag, window, cx| group.drop_item(item.clone(), None, window, cx)
                            })
                    })
            })
            .collect();
        let bar = TabBar::new("tab-bar")
            .track_scroll(&self.scroll_handle)
            .children(tabs)
            .last_empty_space(
                // Empty space so a tab can be moved past the last one.
                div()
                    .id("tab-bar-empty-space")
                    .h_full()
                    .flex_grow_1()
                    .min_w_16()
                    .when(droppable, |this| {
                        this.drag_over::<DragPanel>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
                            .on_drop({
                                let group = group.clone();
                                let node = group.node();
                                move |drag: &DragPanel, window, cx| {
                                    // A tab dropped past its own last tab lands
                                    // in the final slot; one from elsewhere is
                                    // appended in the background.
                                    let ix = (drag.source() == node).then(|| tabs_count - 1);
                                    group.drop_panel(drag.clone(), ix, false, window, cx);
                                }
                            })
                            .drag_over::<AnyDrag>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
                            .on_drop({
                                let group = group.clone();
                                move |item: &AnyDrag, window, cx| group.drop_item(item.clone(), None, window, cx)
                            })
                    }),
            )
            .suffix(h_flex().items_center().h_full().px_1().gap_1().bg(cx.theme().tokens.tab_bar).child(self.render_toolbar(group, window, cx)));
        let _ = closable;
        self.bar_shell(cx).child(bar).into_any_element()
    }

    /// The end of the bar: the way back from a zoom, and the menu with the
    /// pane's commands, zoom and close.
    fn render_toolbar(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if group.is_collapsed() {
            return h_flex();
        }
        let zoomed = group.is_zoomed();
        let handle = group.active_panel().and_then(PanelHandle::of);
        let control = group.active_panel().and_then(|panel| panel.zoomable(cx).then(|| PanelHandle::of(panel).and_then(|handle| handle.zoom_control(cx))).flatten());
        let toolbar_zoom = control.is_some_and(|control| control.toolbar_visible());
        let menu_zoom = control.is_some_and(|control| control.menu_visible());
        let closable = group.is_closable();
        let buttons = handle.and_then(|handle| handle.toolbar_buttons(window, cx));
        let panel = handle.map(|handle| handle.panel());
        h_flex()
            .gap_1()
            .occlude()
            .when_some(buttons, |this, buttons| this.children(buttons.into_iter().map(|button| button.xsmall().ghost().tab_stop(false))))
            .when_some(
                match (zoomed, toolbar_zoom) {
                    (true, _) => Some(("zoom-out", IconName::Minimize, "Zoom out")),
                    (false, true) => Some(("zoom-in", IconName::Maximize, "Zoom in")),
                    (false, false) => None,
                },
                |this, (id, icon, tooltip)| {
                    this.child(Button::new(id).icon(icon).xsmall().ghost().tab_stop(false).tooltip(tooltip).selected(zoomed).on_click({
                        let group = group.clone();
                        move |_, window, cx| group.toggle_zoom(window, cx)
                    }))
                },
            )
            .child(
                Button::new("menu").icon(IconName::Ellipsis).xsmall().ghost().tab_stop(false).dropdown_menu(move |menu, window, cx| {
                    menu.when_some(panel.clone(), |menu, panel| panel.dropdown_menu(menu, window, cx))
                        .separator()
                        .menu_with_disabled(if zoomed { "Zoom out" } else { "Zoom in" }, Box::new(ToggleZoom), !menu_zoom)
                        .when(closable, |menu| menu.separator().menu("Close", Box::new(ClosePanel)))
                }),
            )
    }
}

/// A panel's title, as the panel draws it.
fn title_of(panel: &Arc<dyn BasePanelView>, window: &mut Window, cx: &mut App) -> AnyElement {
    match PanelHandle::of(panel) {
        Some(handle) => handle.title(window, cx),
        None => SharedString::from(panel.panel_name(cx)).into_any_element(),
    }
}

/// The chip that follows the pointer while a tab is dragged: the tab's title.
struct TabGhost {
    panel: Arc<dyn BasePanelView>,
}

impl Render for TabGhost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .w(DRAG_PREVIEW_SIZE.width)
            .h(DRAG_PREVIEW_SIZE.height)
            .px_2()
            .items_center()
            .rounded(px(6.))
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .text_sm()
            .overflow_hidden()
            .child(title_of(&self.panel, window, cx))
    }
}
