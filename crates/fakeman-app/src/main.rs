#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mapping;
mod state;
mod theme;

use fakeman_core::{BodyFormat, Endpoint};
use fakeman_store::{EndpointId, Store};
use gpui_kit::assets::AllAssets;
use gpui_kit::component::{
    Icon, IconName, TitleBar, WindowExt as _,
    button::ButtonVariant,
    checkbox::Checkbox,
    h_flex,
    input::{Escape, Input, InputEvent, InputState},
    menu::{ContextMenuExt as _, PopupMenuItem},
    notification::Notification,
    radio::Radio,
    resizable::{h_resizable, resizable_panel, v_resizable},
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use state::{AppState, StateError};
use theme::{ActiveTokens as _, Method, Tokens, corner, dim, font_size, spacing};

const WORKSPACE: &str = "My Workspace";

actions!(fakeman, [NewEndpoint, DuplicateEndpoint]);

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestTab {
    Body,
    Headers,
    Run,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Collections,
    History,
}

struct Rename {
    id: EndpointId,
    input: Entity<InputState>,
    _subscription: Subscription,
}

struct FakemanWindow {
    state: Entity<AppState>,
    focus: FocusHandle,
    url: Entity<InputState>,
    search: Entity<InputState>,
    rename: Option<Rename>,
    shown: Option<EndpointId>,
    request_tab: RequestTab,
    sidebar_view: SidebarView,
    _subscriptions: Vec<Subscription>,
}

impl FakemanWindow {
    fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("Enter URL or paste text"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search collections"));
        let subscriptions = vec![
            cx.subscribe_in(&url, window, |this, url, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = url.read(cx).value();
                    this.state.update(cx, |state, cx| {
                        state.set_url(&value);
                        state.schedule_save(cx);
                    });
                }
            }),
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&state, window, |_, _, StateError(message), window, cx| {
                window.push_notification(Notification::error(message.clone()).title("fakeman"), cx);
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
        ];
        let flush_state = state.clone();
        window.on_window_should_close(cx, move |_, cx| {
            flush_state.update(cx, |state, cx| state.flush_or_report(cx));
            true
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);

        let mut this = Self {
            state,
            focus,
            url,
            search,
            rename: None,
            shown: None,
            request_tab: RequestTab::Body,
            sidebar_view: SidebarView::Collections,
            _subscriptions: subscriptions,
        };
        this.sync_url(window, cx);
        this
    }

    /// Runs a state change, reports a failure as a notification, and points
    /// the URL input at the endpoint that is selected afterwards.
    fn update_state<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut AppState) -> anyhow::Result<R>,
    ) -> Option<R> {
        let result = self.state.update(cx, |state, cx| {
            let result = change(state);
            cx.notify();
            result
        });
        self.sync_url(window, cx);
        result
            .map_err(|err| {
                window.push_notification(
                    Notification::error(format!("{err:#}")).title("fakeman"),
                    cx,
                );
            })
            .ok()
    }

    fn sync_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if state.selected() == self.shown {
            return;
        }
        self.shown = state.selected();
        let url = state.draft().map(|d| d.url.clone()).unwrap_or_default();
        self.url
            .update(cx, |input, cx| input.set_value(url, window, cx));
        cx.notify();
    }

    fn select(&mut self, id: EndpointId, window: &mut Window, cx: &mut Context<Self>) {
        self.update_state(window, cx, |state| state.select(id));
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.update_state(window, cx, AppState::create) {
            self.start_rename(id, window, cx);
        }
    }

    fn duplicate(&mut self, id: EndpointId, window: &mut Window, cx: &mut Context<Self>) {
        self.update_state(window, cx, |state| state.duplicate(id));
    }

    fn on_new_endpoint(&mut self, _: &NewEndpoint, window: &mut Window, cx: &mut Context<Self>) {
        self.create(window, cx);
    }

    fn on_duplicate_endpoint(
        &mut self,
        _: &DuplicateEndpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.state.read(cx).selected() {
            self.duplicate(id, window, cx);
        }
    }

    fn start_rename(&mut self, id: EndpointId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.state.read(cx).endpoint(id).map(|e| e.name.clone()) else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.finish_rename(true, window, cx);
                }
            });
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.rename = Some(Rename {
            id,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.rename.take() else {
            return;
        };
        if commit {
            let name = rename.input.read(cx).value();
            self.update_state(window, cx, |state| state.rename(rename.id, &name));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn confirm_delete(&mut self, id: EndpointId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.state.read(cx).endpoint(id).map(|e| e.name.clone()) else {
            return;
        };
        let this = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let this = this.clone();
            alert
                .confirm()
                .title("Delete endpoint?")
                .description(format!(
                    "\"{name}\" and its fields and headers will be deleted. This cannot be undone."
                ))
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.update_state(window, cx, |state| state.delete(id));
                    })
                    .ok();
                    true
                })
        });
    }

    fn header(&self, t: &Tokens) -> impl IntoElement {
        TitleBar::new().child(
            h_flex()
                .w_full()
                .h_full()
                .pr(spacing::MD)
                .gap(spacing::LG)
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap(spacing::MD)
                        .items_center()
                        .child(
                            div()
                                .size(dim::LOGO)
                                .rounded_full()
                                .bg(t.brand)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::default()
                                        .path("icons/send.svg")
                                        .size(dim::ICON_SM)
                                        .text_color(t.text_on_accent),
                                ),
                        )
                        .child(
                            div()
                                .text_size(font_size::MD)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.text)
                                .child("fakeman"),
                        )
                        .child(
                            h_flex()
                                .gap(spacing::XS)
                                .items_center()
                                .px(spacing::MD)
                                .text_size(font_size::BASE)
                                .text_color(t.text_muted)
                                .child(WORKSPACE)
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .size(dim::ICON_SM)
                                        .text_color(t.text_subtle),
                                ),
                        ),
                )
                .child(
                    h_flex()
                        .w(dim::SEARCH_WIDTH)
                        .h(dim::CONTROL_HEIGHT)
                        .px(spacing::MD)
                        .gap(spacing::SM)
                        .items_center()
                        .rounded(corner::SM)
                        .bg(t.bg_raised)
                        .border_1()
                        .border_color(t.border)
                        .text_size(font_size::BASE)
                        .text_color(t.text_subtle)
                        .child(Icon::new(IconName::Search).size(dim::ICON_SM))
                        .child("Search fakeman"),
                )
                .child(
                    h_flex()
                        .gap(spacing::LG)
                        .items_center()
                        .text_color(t.text_muted)
                        .child(Icon::new(IconName::Settings).size(dim::ICON_MD))
                        .child(Icon::new(IconName::Bell).size(dim::ICON_MD)),
                ),
        )
    }

    fn sidebar_switcher(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |view: SidebarView, icon: &'static str, label: &'static str| {
            let active = self.sidebar_view == view;
            div()
                .id(label)
                .size(dim::CONTROL_HEIGHT)
                .flex()
                .items_center()
                .justify_center()
                .rounded(corner::SM)
                .cursor_pointer()
                .text_color(if active { t.text } else { t.text_muted })
                .map(|this| {
                    if active {
                        this.bg(t.bg_selected)
                    } else {
                        this.hover(|s| s.bg(t.bg_hover))
                    }
                })
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sidebar_view = view;
                    cx.notify();
                }))
                .child(Icon::default().path(icon).size(dim::ICON_MD))
        };

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .px(spacing::MD)
            .gap(spacing::XS)
            .items_center()
            .border_b_1()
            .border_color(t.border)
            .child(item(
                SidebarView::Collections,
                "icons/box.svg",
                "Collections",
            ))
            .child(item(SidebarView::History, "icons/clock.svg", "History"))
    }

    fn history(&self, t: &Tokens) -> impl IntoElement {
        v_flex().py(spacing::XS).child(
            div()
                .h(dim::TREE_ROW_HEIGHT)
                .px(spacing::LG)
                .flex()
                .items_center()
                .text_size(font_size::SM)
                .text_color(t.text_subtle)
                .child("Sent requests will show up here."),
        )
    }

    fn method_tag(t: &Tokens, method: Method) -> impl IntoElement {
        div()
            .w(dim::METHOD_TAG_WIDTH)
            .flex_none()
            .text_size(font_size::XS)
            .font_weight(FontWeight::BOLD)
            .text_color(t.method(method))
            .child(method.label())
    }

    fn endpoint_list(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let query = self.search.read(cx).value();
        let selected = state.selected();
        let this = cx.entity().downgrade();
        let rows: Vec<_> = state
            .filtered(&query)
            .into_iter()
            .map(|endpoint| {
                let id = endpoint.id;
                let active = selected == Some(id);
                let label = match &self.rename {
                    Some(rename) if rename.id == id => div()
                        .flex_1()
                        .on_action(cx.listener(|this, _: &Escape, window, cx| {
                            this.finish_rename(false, window, cx)
                        }))
                        .child(Input::new(&rename.input).h(dim::CONTROL_HEIGHT))
                        .into_any_element(),
                    _ => div()
                        .truncate()
                        .child(endpoint.name.clone())
                        .into_any_element(),
                };
                let this = this.clone();
                h_flex()
                    .id(("endpoint", id as u64))
                    .h(dim::TREE_ROW_HEIGHT)
                    .px(spacing::MD)
                    .gap(spacing::SM)
                    .items_center()
                    .cursor_pointer()
                    .text_size(font_size::BASE)
                    .text_color(if active { t.text } else { t.text_muted })
                    .when(active, |this| this.bg(t.bg_selected))
                    .when(!active, |this| this.hover(|s| s.bg(t.bg_hover)))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        this.select(id, window, cx);
                        if event.click_count() >= 2 {
                            this.start_rename(id, window, cx);
                        }
                    }))
                    .child(Self::method_tag(t, Method::Post))
                    .child(label)
                    .context_menu(move |menu, _, _| {
                        let action = |label: &'static str,
                                      run: fn(
                            &mut FakemanWindow,
                            EndpointId,
                            &mut Window,
                            &mut Context<FakemanWindow>,
                        )| {
                            let this = this.clone();
                            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                this.update(cx, |this, cx| run(this, id, window, cx)).ok();
                            })
                        };
                        menu.item(action("Rename", Self::start_rename))
                            .item(action("Duplicate", Self::duplicate))
                            .separator()
                            .item(action("Delete", Self::confirm_delete))
                    })
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        let empty_hint = if state.endpoints().is_empty() {
            "No endpoints yet. Click New to create one."
        } else {
            "No endpoints match your search."
        };

        v_flex()
            .id("endpoint-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(spacing::XS)
            .children(rows)
            .when(empty, |this| {
                this.child(
                    div()
                        .px(spacing::LG)
                        .py(spacing::MD)
                        .text_size(font_size::SM)
                        .text_color(t.text_subtle)
                        .child(empty_hint),
                )
            })
    }

    fn sidebar(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.sidebar_view {
            SidebarView::Collections => self.endpoint_list(t, cx).into_any_element(),
            SidebarView::History => self.history(t).into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(t.bg_sidebar)
            .child(self.sidebar_switcher(t, cx))
            .child(
                h_flex()
                    .h(dim::BAR_HEIGHT)
                    .px(spacing::LG)
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(t.border)
                    .child(
                        div()
                            .text_size(font_size::BASE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text)
                            .child(WORKSPACE),
                    )
                    .child(
                        h_flex()
                            .gap(spacing::XS)
                            .child(Self::small_button(t, "New").on_click(
                                cx.listener(|this, _, window, cx| this.create(window, cx)),
                            ))
                            .child(Self::small_button(t, "Import")),
                    ),
            )
            .child(
                h_flex()
                    .h(dim::BAR_HEIGHT)
                    .px(spacing::MD)
                    .gap(spacing::SM)
                    .items_center()
                    .text_color(t.text_muted)
                    .child(
                        div()
                            .id("new-endpoint")
                            .cursor_pointer()
                            .hover(|s| s.text_color(t.text))
                            .tooltip(|window, cx| Tooltip::new("New endpoint").build(window, cx))
                            .on_click(cx.listener(|this, _, window, cx| this.create(window, cx)))
                            .child(Icon::new(IconName::Plus).size(dim::ICON_MD)),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .h(dim::CONTROL_HEIGHT)
                            .pl(spacing::MD)
                            .gap(spacing::XS)
                            .items_center()
                            .rounded(corner::SM)
                            .border_1()
                            .border_color(t.border)
                            .text_size(font_size::SM)
                            .text_color(t.text_subtle)
                            .child(Icon::new(IconName::Search).size(dim::ICON_SM))
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.search).appearance(false).cleanable(true),
                                ),
                            ),
                    )
                    .child(Icon::new(IconName::Ellipsis).size(dim::ICON_MD)),
            )
            .child(content)
    }

    fn small_button(t: &Tokens, label: &'static str) -> Stateful<Div> {
        div()
            .id(label)
            .px(spacing::MD)
            .py(spacing::XXS)
            .rounded(corner::SM)
            .border_1()
            .border_color(t.border_strong)
            .cursor_pointer()
            .text_size(font_size::SM)
            .text_color(t.text)
            .hover(|s| s.bg(t.bg_hover))
            .child(label)
    }

    fn tab_strip(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let selected = state.selected();
        let tabs: Vec<_> = state
            .open_tabs()
            .iter()
            .filter_map(|&id| state.endpoint(id))
            .map(|endpoint| {
                let id = endpoint.id;
                let active = selected == Some(id);
                h_flex()
                    .id(("tab", id as u64))
                    .h_full()
                    .max_w(dim::TAB_MAX_WIDTH)
                    .px(spacing::LG)
                    .gap(spacing::SM)
                    .items_center()
                    .relative()
                    .border_r_1()
                    .border_color(t.border)
                    .cursor_pointer()
                    .text_size(font_size::BASE)
                    .map(|this| {
                        if active {
                            this.bg(t.bg_app).text_color(t.text).child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .h(dim::ACCENT_LINE)
                                    .bg(t.brand),
                            )
                        } else {
                            this.text_color(t.text_muted).hover(|s| s.bg(t.bg_hover))
                        }
                    })
                    .on_click(cx.listener(move |this, _, window, cx| this.select(id, window, cx)))
                    .child(
                        div()
                            .text_size(font_size::XS)
                            .font_weight(FontWeight::BOLD)
                            .text_color(t.method(Method::Post))
                            .child(Method::Post.label()),
                    )
                    .child(div().truncate().child(endpoint.name.clone()))
                    .child(
                        div()
                            .id(("close-tab", id as u64))
                            .w(dim::ICON_SM)
                            .flex()
                            .justify_center()
                            .when(active, |this| {
                                this.on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.update_state(window, cx, |state| state.close_tab(id));
                                }))
                                .child(
                                    Icon::new(IconName::Close)
                                        .size(dim::ICON_SM)
                                        .text_color(t.text_subtle),
                                )
                            }),
                    )
            })
            .collect();

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .bg(t.bg_header)
            .border_b_1()
            .border_color(t.border)
            .children(tabs)
            .child(
                div()
                    .id("new-tab")
                    .px(spacing::LG)
                    .cursor_pointer()
                    .text_color(t.text_muted)
                    .hover(|s| s.text_color(t.text))
                    .on_click(cx.listener(|this, _, window, cx| this.create(window, cx)))
                    .child(Icon::new(IconName::Plus).size(dim::ICON_MD)),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .h_full()
                    .px(spacing::LG)
                    .gap(spacing::SM)
                    .items_center()
                    .border_l_1()
                    .border_color(t.border)
                    .text_size(font_size::BASE)
                    .text_color(t.text_muted)
                    .child("No environment")
                    .child(Icon::new(IconName::ChevronDown).size(dim::ICON_SM)),
            )
    }

    fn request_header(
        &self,
        t: &Tokens,
        draft: &Endpoint,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let crumbs = h_flex()
            .gap(spacing::SM)
            .items_center()
            .text_size(font_size::BASE)
            .text_color(t.text_muted)
            .child(
                div()
                    .px(spacing::XS)
                    .rounded(corner::SM)
                    .border_1()
                    .border_color(t.border_strong)
                    .text_size(font_size::XS)
                    .font_weight(FontWeight::BOLD)
                    .child("HTTP"),
            )
            .child(WORKSPACE)
            .child("/")
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text)
                    .child(draft.name.clone()),
            );

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .px(spacing::XL)
            .items_center()
            .justify_between()
            .child(crumbs)
            .child(
                h_flex()
                    .gap(spacing::SM)
                    .items_center()
                    .child(
                        Self::small_button(t, "Save").on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| state.flush_or_report(cx));
                        })),
                    )
                    .child(Self::small_button(t, "Share")),
            )
    }

    fn request_line(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .px(spacing::XL)
            .gap(spacing::MD)
            .items_center()
            .child(
                h_flex()
                    .flex_1()
                    .h(dim::REQUEST_LINE_HEIGHT)
                    .items_center()
                    .rounded(corner::MD)
                    .border_1()
                    .border_color(t.border_strong)
                    .child(
                        h_flex()
                            .w(dim::METHOD_SELECT_WIDTH)
                            .h_full()
                            .px(spacing::LG)
                            .items_center()
                            .justify_between()
                            .border_r_1()
                            .border_color(t.border_strong)
                            .child(
                                div()
                                    .text_size(font_size::BASE)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.method(Method::Post))
                                    .child(Method::Post.label()),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(dim::ICON_SM)
                                    .text_color(t.text_subtle),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .px(spacing::XS)
                            .child(Input::new(&self.url).appearance(false)),
                    ),
            )
            .child(
                h_flex()
                    .id("send")
                    .w(dim::SEND_WIDTH)
                    .h(dim::REQUEST_LINE_HEIGHT)
                    .rounded(corner::MD)
                    .bg(t.send)
                    .hover(|s| s.bg(t.send_hover))
                    .cursor_pointer()
                    .text_color(t.text_on_accent)
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.push_notification(
                            Notification::info("Sending arrives in a later phase.").title("Send"),
                            cx,
                        );
                    }))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .justify_center()
                            .text_size(font_size::MD)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Send"),
                    )
                    .child(
                        div()
                            .h_full()
                            .px(spacing::SM)
                            .flex()
                            .items_center()
                            .border_l_1()
                            .border_color(t.send_divider)
                            .child(Icon::new(IconName::ChevronDown).size(dim::ICON_SM)),
                    ),
            )
    }

    fn request_tabs(
        &self,
        t: &Tokens,
        draft: &Endpoint,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = |tab: RequestTab, label: &'static str, count: Option<usize>| {
            let active = self.request_tab == tab;
            h_flex()
                .id(label)
                .h_full()
                .gap(spacing::XS)
                .items_center()
                .border_b_2()
                .border_color(if active { t.brand } else { t.bg_app })
                .cursor_pointer()
                .text_size(font_size::BASE)
                .text_color(if active { t.text } else { t.text_muted })
                .when(!active, |this| this.hover(|s| s.text_color(t.text)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_tab = tab;
                    cx.notify();
                }))
                .child(label)
                .when_some(count, |this, count| {
                    this.child(
                        div()
                            .text_size(font_size::XS)
                            .text_color(t.success)
                            .child(count.to_string()),
                    )
                })
        };
        let enabled_fields = draft.fields.iter().filter(|f| f.enabled).count();

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .px(spacing::XL)
            .gap(spacing::XL)
            .border_b_1()
            .border_color(t.border)
            .child(tab(RequestTab::Body, "Body", Some(enabled_fields)))
            .child(tab(
                RequestTab::Headers,
                "Headers",
                Some(draft.headers.len()),
            ))
            .child(tab(RequestTab::Run, "Run", None))
    }

    fn table_row(t: &Tokens, check: AnyElement, key: AnyElement, value: AnyElement) -> Div {
        h_flex()
            .h(dim::ROW_HEIGHT)
            .border_b_1()
            .border_color(t.border)
            .text_size(font_size::BASE)
            .child(
                div()
                    .w(dim::CHECK_COLUMN)
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_r_1()
                    .border_color(t.border)
                    .child(check),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .px(spacing::MD)
                    .flex()
                    .items_center()
                    .border_r_1()
                    .border_color(t.border)
                    .child(key),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .px(spacing::MD)
                    .flex()
                    .items_center()
                    .border_r_1()
                    .border_color(t.border)
                    .child(value),
            )
            .child(div().w(dim::TRAILING_COLUMN).h_full())
    }

    fn table_header(t: &Tokens, value_label: &'static str) -> impl IntoElement {
        Self::table_row(
            t,
            div().into_any_element(),
            div().child("Key").into_any_element(),
            div().child(value_label).into_any_element(),
        )
        .text_color(t.text_muted)
        .font_weight(FontWeight::MEDIUM)
    }

    fn placeholder_row(t: &Tokens, value_label: &'static str) -> impl IntoElement {
        Self::table_row(
            t,
            div().into_any_element(),
            div().child("Key").into_any_element(),
            div().child(value_label).into_any_element(),
        )
        .text_color(t.text_subtle)
    }

    fn table(t: &Tokens) -> Div {
        v_flex()
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .border_color(t.border)
    }

    fn body_tab(t: &Tokens, draft: &Endpoint) -> impl IntoElement {
        let format_radio = |format: BodyFormat, id: &'static str, label: &'static str| {
            Radio::new(id)
                .label(label)
                .checked(draft.body_format == format)
        };

        let rows = draft.fields.iter().enumerate().map(|(ix, field)| {
            let text = if field.enabled { t.text } else { t.text_subtle };
            Self::table_row(
                t,
                Checkbox::new(("field-enabled", ix))
                    .checked(field.enabled)
                    .into_any_element(),
                div()
                    .text_color(text)
                    .child(field.name.clone())
                    .into_any_element(),
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .text_color(text)
                    .child(mapping::generator_display(&field.generator).into_owned())
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(dim::ICON_SM)
                            .text_color(t.text_subtle),
                    )
                    .into_any_element(),
            )
        });

        v_flex()
            .gap(spacing::LG)
            .child(
                h_flex()
                    .gap(spacing::XL)
                    .items_center()
                    .text_size(font_size::BASE)
                    .child(format_radio(
                        BodyFormat::FormUrlEncoded,
                        "format-form",
                        "x-www-form-urlencoded",
                    ))
                    .child(format_radio(BodyFormat::Json, "format-raw", "raw"))
                    .when(draft.body_format == BodyFormat::Json, |this| {
                        this.child(
                            h_flex()
                                .gap(spacing::XS)
                                .items_center()
                                .text_color(t.link)
                                .child("JSON")
                                .child(Icon::new(IconName::ChevronDown).size(dim::ICON_SM)),
                        )
                    }),
            )
            .child(
                Self::table(t)
                    .child(Self::table_header(t, "Fake data"))
                    .children(rows)
                    .child(Self::placeholder_row(t, "Fake data")),
            )
    }

    fn headers_tab(t: &Tokens, draft: &Endpoint) -> impl IntoElement {
        let rows = draft.headers.iter().enumerate().map(|(ix, header)| {
            Self::table_row(
                t,
                Checkbox::new(("header-enabled", ix))
                    .checked(header.enabled)
                    .into_any_element(),
                div().child(header.key.clone()).into_any_element(),
                div().child(header.value.clone()).into_any_element(),
            )
            .text_color(if header.enabled {
                t.text
            } else {
                t.text_subtle
            })
        });
        Self::table(t)
            .child(Self::table_header(t, "Value"))
            .children(rows)
            .child(Self::placeholder_row(t, "Value"))
    }

    fn run_tab(t: &Tokens, draft: &Endpoint) -> impl IntoElement {
        let setting = |label: &'static str, value: String| {
            v_flex()
                .gap(spacing::XS)
                .child(
                    div()
                        .text_size(font_size::SM)
                        .text_color(t.text_muted)
                        .child(label),
                )
                .child(
                    div()
                        .w(px(200.))
                        .h(dim::CONTROL_HEIGHT)
                        .px(spacing::MD)
                        .flex()
                        .items_center()
                        .rounded(corner::SM)
                        .border_1()
                        .border_color(t.border_strong)
                        .text_size(font_size::BASE)
                        .text_color(t.text)
                        .child(value),
                )
        };
        v_flex()
            .gap(spacing::LG)
            .child(setting("Send count", draft.run.count.to_string()))
            .child(setting(
                "Delay between requests (ms)",
                draft.run.delay_ms.to_string(),
            ))
    }

    fn response(&self, t: &Tokens) -> impl IntoElement {
        let stat = |label: &'static str| {
            h_flex()
                .gap(spacing::XS)
                .child(div().text_color(t.text_subtle).child(label))
                .child(div().text_color(t.text_muted).child("—"))
        };
        v_flex()
            .size_full()
            .bg(t.bg_app)
            .child(
                h_flex()
                    .h(dim::BAR_HEIGHT)
                    .flex_none()
                    .px(spacing::XL)
                    .items_center()
                    .justify_between()
                    .text_size(font_size::BASE)
                    .child(
                        h_flex()
                            .gap(spacing::XS)
                            .items_center()
                            .text_color(t.text)
                            .child("Response")
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(dim::ICON_SM)
                                    .text_color(t.text_subtle),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap(spacing::LG)
                            .items_center()
                            .child(stat("Status"))
                            .child(stat("Time"))
                            .child(stat("Size")),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap(spacing::MD)
                    .text_color(t.text_subtle)
                    .child(
                        Icon::default()
                            .path("icons/send.svg")
                            .size(dim::ICON_EMPTY_STATE),
                    )
                    .child(
                        div()
                            .text_size(font_size::BASE)
                            .child("Click Send to get a response"),
                    ),
            )
    }

    fn no_endpoint(t: &Tokens) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(spacing::MD)
            .text_color(t.text_subtle)
            .child(
                Icon::default()
                    .path("icons/box.svg")
                    .size(dim::ICON_EMPTY_STATE),
            )
            .child(
                div()
                    .text_size(font_size::BASE)
                    .child("Open an endpoint from the sidebar, or create one with New"),
            )
    }

    fn workbench(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.state.read(cx).draft().cloned();
        let tab_strip = self.tab_strip(t, cx).into_any_element();
        let Some(draft) = draft else {
            return v_flex()
                .size_full()
                .bg(t.bg_app)
                .child(tab_strip)
                .child(div().flex_1().min_h_0().child(Self::no_endpoint(t)));
        };
        let editor = v_flex()
            .size_full()
            .bg(t.bg_app)
            .child(self.request_header(t, &draft, cx))
            .child(self.request_line(t, cx))
            .child(div().h(spacing::MD).flex_none())
            .child(self.request_tabs(t, &draft, cx))
            .child(
                div()
                    .id("request-editor")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(spacing::XL)
                    .map(|this| match self.request_tab {
                        RequestTab::Body => {
                            this.child(Self::body_tab(t, &draft).into_any_element())
                        }
                        RequestTab::Headers => {
                            this.child(Self::headers_tab(t, &draft).into_any_element())
                        }
                        RequestTab::Run => this.child(Self::run_tab(t, &draft).into_any_element()),
                    }),
            );

        v_flex().size_full().bg(t.bg_app).child(tab_strip).child(
            div().flex_1().min_h_0().child(
                v_resizable("request-response")
                    .child(resizable_panel().child(editor))
                    .child(
                        resizable_panel()
                            .size(dim::RESPONSE_HEIGHT)
                            .size_range(dim::RESPONSE_MIN..px(2000.))
                            .child(self.response(t)),
                    ),
            ),
        )
    }

    fn status_bar(&self, t: &Tokens) -> impl IntoElement {
        h_flex()
            .h(dim::STATUS_BAR_HEIGHT)
            .flex_none()
            .px(spacing::LG)
            .items_center()
            .justify_between()
            .bg(t.bg_header)
            .border_t_1()
            .border_color(t.border)
            .text_size(font_size::XS)
            .text_color(t.text_muted)
            .child(
                h_flex()
                    .gap(spacing::LG)
                    .items_center()
                    .child(Icon::new(IconName::PanelLeft).size(dim::ICON_SM))
                    .child(
                        h_flex()
                            .gap(spacing::XS)
                            .items_center()
                            .child(div().size(spacing::SM).rounded_full().bg(t.success))
                            .child("Online"),
                    )
                    .child(
                        h_flex()
                            .gap(spacing::XS)
                            .items_center()
                            .child(Icon::new(IconName::SquareTerminal).size(dim::ICON_SM))
                            .child("Console"),
                    ),
            )
            .child(concat!("fakeman ", env!("CARGO_PKG_VERSION")))
    }
}

impl Render for FakemanWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        v_flex()
            .key_context("Fakeman")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_new_endpoint))
            .on_action(cx.listener(Self::on_duplicate_endpoint))
            .size_full()
            .bg(t.bg_app)
            .text_color(t.text)
            .child(self.header(&t))
            .child(
                h_flex().flex_1().min_h_0().child(
                    div().flex_1().min_w_0().h_full().child(
                        h_resizable("sidebar-workbench")
                            .child(
                                resizable_panel()
                                    .size(dim::SIDEBAR_WIDTH)
                                    .size_range(dim::SIDEBAR_MIN..dim::SIDEBAR_MAX)
                                    .fixed()
                                    .child(self.sidebar(&t, cx)),
                            )
                            .child(resizable_panel().child(self.workbench(&t, cx))),
                    ),
                ),
            )
            .child(self.status_bar(&t))
    }
}

/// Opens the database in the OS data directory. If that fails, the app runs
/// on an empty in-memory database and says so, rather than not starting.
fn open_store() -> (Store, Option<String>) {
    let opened = fakeman_store::default_db_path()
        .map_err(anyhow::Error::from)
        .and_then(|path| {
            Store::open_default().map_err(|err| {
                anyhow::Error::from(err).context(format!("opening {}", path.display()))
            })
        });
    match opened {
        Ok(store) => (store, None),
        Err(err) => (
            Store::open_in_memory().expect("an in-memory database always opens"),
            Some(format!("{err:#}. Changes will not be saved.")),
        ),
    }
}

fn main() {
    gpui_kit::application().with_assets(AllAssets).run(|cx| {
        gpui_kit::init(cx);
        Tokens::postman_dark().install(cx);
        let (new, duplicate) = if cfg!(target_os = "macos") {
            ("cmd-n", "cmd-d")
        } else {
            ("ctrl-n", "ctrl-d")
        };
        cx.bind_keys([
            KeyBinding::new(new, NewEndpoint, Some("Fakeman")),
            KeyBinding::new(duplicate, DuplicateEndpoint, Some("Fakeman")),
        ]);

        let (store, mut startup_error) = open_store();
        let state = match AppState::load(store) {
            Ok(state) => state,
            Err(err) => {
                startup_error = Some(format!("{err:#}"));
                AppState::load(Store::open_in_memory().expect("an in-memory database always opens"))
                    .expect("an empty database always loads")
            }
        };
        let state = cx.new(|_| state);

        let mut options = TitleBar::window_options();
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some("fakeman".into());
        }
        options.window_bounds = Some(WindowBounds::centered(size(px(1280.), px(800.)), cx));

        gpui_kit::open_window(options, cx, move |window, cx| {
            if let Some(message) = startup_error.take() {
                window.push_notification(Notification::error(message).title("Database"), cx);
            }
            cx.new(|cx| FakemanWindow::new(state.clone(), window, cx))
        })
        .expect("failed to open the fakeman window");
    });
}
