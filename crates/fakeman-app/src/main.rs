#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod theme;

use gpui_kit::assets::AllAssets;
use gpui_kit::component::{
    Icon, IconName, TitleBar, WindowExt as _,
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    radio::Radio,
    resizable::{h_resizable, resizable_panel, v_resizable},
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::{ActiveTokens as _, Method, Tokens, corner, dim, font_size, spacing};

struct SavedRequest {
    method: Method,
    name: &'static str,
    folder: Option<&'static str>,
    url: &'static str,
}

const COLLECTION: &str = "fakeman samples";

const REQUESTS: [SavedRequest; 3] = [
    SavedRequest {
        method: Method::Post,
        name: "Create user",
        folder: Some("Users"),
        url: "https://api.example.com/users",
    },
    SavedRequest {
        method: Method::Post,
        name: "Signup",
        folder: Some("Users"),
        url: "https://api.example.com/signup",
    },
    SavedRequest {
        method: Method::Post,
        name: "Place order",
        folder: None,
        url: "https://api.example.com/orders",
    },
];

const FIELDS: [(&str, &str); 4] = [
    ("firstname", "person.firstName"),
    ("lastname", "person.lastName"),
    ("email", "internet.email"),
    ("age", "number.int"),
];

const HEADERS: [(&str, &str); 2] = [
    ("Content-Type", "application/json"),
    ("Authorization", "Bearer <token>"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestTab {
    Body,
    Headers,
    Run,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyFormat {
    FormUrlEncoded,
    RawJson,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Collections,
    History,
}

struct FakemanWindow {
    url: Entity<InputState>,
    selected: usize,
    open_tabs: Vec<usize>,
    request_tab: RequestTab,
    body_format: BodyFormat,
    sidebar_view: SidebarView,
    field_enabled: [bool; FIELDS.len()],
}

impl FakemanWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter URL or paste text")
                .default_value(REQUESTS[0].url)
        });
        Self {
            url,
            selected: 0,
            open_tabs: vec![0, 1],
            request_tab: RequestTab::Body,
            body_format: BodyFormat::RawJson,
            sidebar_view: SidebarView::Collections,
            field_enabled: [true, true, true, false],
        }
    }

    fn select(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = ix;
        if !self.open_tabs.contains(&ix) {
            self.open_tabs.push(ix);
        }
        self.url
            .update(cx, |url, cx| url.set_value(REQUESTS[ix].url, window, cx));
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value();
        let message = if url.is_empty() {
            "Send pressed with an empty URL.".to_string()
        } else {
            format!("POST {url}")
        };
        println!("{message}");
        window.push_notification(Notification::info(message).title("Send"), cx);
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
                                .child("My Workspace")
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
        let row = |request: &SavedRequest| {
            h_flex()
                .h(dim::TREE_ROW_HEIGHT)
                .px(spacing::LG)
                .gap(spacing::SM)
                .items_center()
                .text_size(font_size::BASE)
                .text_color(t.text_muted)
                .hover(|s| s.bg(t.bg_hover))
                .child(Self::method_tag(t, request.method))
                .child(div().truncate().child(request.url))
        };
        v_flex()
            .py(spacing::XS)
            .child(
                div()
                    .h(dim::TREE_ROW_HEIGHT)
                    .px(spacing::LG)
                    .flex()
                    .items_center()
                    .text_size(font_size::SM)
                    .text_color(t.text_subtle)
                    .child("Today"),
            )
            .children(REQUESTS.iter().map(row))
    }

    fn tree_row(
        &self,
        t: &Tokens,
        depth: usize,
        chevron: Option<bool>,
        lead: AnyElement,
        label: &'static str,
        active: bool,
    ) -> Div {
        h_flex()
            .h(dim::TREE_ROW_HEIGHT)
            .pl(spacing::MD + dim::TREE_INDENT * depth as f32)
            .pr(spacing::MD)
            .gap(spacing::SM)
            .items_center()
            .text_size(font_size::BASE)
            .text_color(if active { t.text } else { t.text_muted })
            .when(active, |this| this.bg(t.bg_selected))
            .when(!active, |this| this.hover(|s| s.bg(t.bg_hover)))
            .child(
                div()
                    .w(dim::ICON_SM)
                    .flex_none()
                    .when_some(chevron, |this, open| {
                        this.child(
                            Icon::new(if open {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .size(dim::ICON_SM)
                            .text_color(t.text_subtle),
                        )
                    }),
            )
            .child(lead)
            .child(div().truncate().child(label))
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

    fn sidebar(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let folder_icon = |open: bool| {
            Icon::new(if open {
                IconName::FolderOpen
            } else {
                IconName::Folder
            })
            .size(dim::ICON_SM)
            .text_color(t.text_subtle)
            .into_any_element()
        };
        let request_row = |this: &Self, ix: usize, depth: usize, cx: &mut Context<Self>| {
            let request = &REQUESTS[ix];
            this.tree_row(
                t,
                depth,
                None,
                Self::method_tag(t, request.method).into_any_element(),
                request.name,
                ix == this.selected,
            )
            .id(("request", ix))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| this.select(ix, window, cx)))
        };

        let mut tree = v_flex().py(spacing::XS).child(self.tree_row(
            t,
            0,
            Some(true),
            div().into_any_element(),
            COLLECTION,
            false,
        ));
        tree = tree.child(self.tree_row(t, 1, Some(true), folder_icon(true), "Users", false));
        for (ix, request) in REQUESTS.iter().enumerate() {
            if request.folder.is_some() {
                tree = tree.child(request_row(self, ix, 2, cx));
            }
        }
        for (ix, request) in REQUESTS.iter().enumerate() {
            if request.folder.is_none() {
                tree = tree.child(request_row(self, ix, 1, cx));
            }
        }
        tree = tree.child(self.tree_row(
            t,
            0,
            Some(false),
            div().into_any_element(),
            "Webhooks",
            false,
        ));

        let (search_label, content) = match self.sidebar_view {
            SidebarView::Collections => ("Search collections", tree.into_any_element()),
            SidebarView::History => ("Search history", self.history(t).into_any_element()),
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
                            .child("My Workspace"),
                    )
                    .child(
                        h_flex()
                            .gap(spacing::XS)
                            .child(Self::small_button(t, "New"))
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
                    .child(Icon::new(IconName::Plus).size(dim::ICON_MD))
                    .child(
                        h_flex()
                            .flex_1()
                            .h(dim::CONTROL_HEIGHT)
                            .px(spacing::MD)
                            .gap(spacing::SM)
                            .items_center()
                            .rounded(corner::SM)
                            .border_1()
                            .border_color(t.border)
                            .text_size(font_size::SM)
                            .text_color(t.text_subtle)
                            .child(Icon::new(IconName::Search).size(dim::ICON_SM))
                            .child(search_label),
                    )
                    .child(Icon::new(IconName::Ellipsis).size(dim::ICON_MD)),
            )
            .child(content)
    }

    fn small_button(t: &Tokens, label: &'static str) -> impl IntoElement {
        div()
            .px(spacing::MD)
            .py(spacing::XXS)
            .rounded(corner::SM)
            .border_1()
            .border_color(t.border_strong)
            .text_size(font_size::SM)
            .text_color(t.text)
            .hover(|s| s.bg(t.bg_hover))
            .child(label)
    }

    fn tab_strip(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.open_tabs.iter().map(|&ix| {
            let request = &REQUESTS[ix];
            let active = ix == self.selected;
            h_flex()
                .id(("tab", ix))
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
                .on_click(cx.listener(move |this, _, window, cx| this.select(ix, window, cx)))
                .child(
                    div()
                        .text_size(font_size::XS)
                        .font_weight(FontWeight::BOLD)
                        .text_color(t.method(request.method))
                        .child(request.method.label()),
                )
                .child(div().truncate().child(request.name))
                .child(
                    div()
                        .w(dim::ICON_SM)
                        .flex()
                        .justify_center()
                        .when(active, |this| {
                            this.child(
                                Icon::new(IconName::Close)
                                    .size(dim::ICON_SM)
                                    .text_color(t.text_subtle),
                            )
                        }),
                )
        });

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .bg(t.bg_header)
            .border_b_1()
            .border_color(t.border)
            .children(tabs)
            .child(
                div()
                    .px(spacing::LG)
                    .text_color(t.text_muted)
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

    fn request_header(&self, t: &Tokens) -> impl IntoElement {
        let request = &REQUESTS[self.selected];
        let mut crumbs = h_flex()
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
            .child(COLLECTION)
            .child("/");
        if let Some(folder) = request.folder {
            crumbs = crumbs.child(folder).child("/");
        }
        crumbs = crumbs.child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.text)
                .child(request.name),
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
                    .child(Self::small_button(t, "Save"))
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
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
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

    fn request_tabs(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
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
        let enabled_fields = self
            .field_enabled
            .iter()
            .filter(|enabled| **enabled)
            .count();

        h_flex()
            .h(dim::BAR_HEIGHT)
            .flex_none()
            .px(spacing::XL)
            .gap(spacing::XL)
            .border_b_1()
            .border_color(t.border)
            .child(tab(RequestTab::Body, "Body", Some(enabled_fields)))
            .child(tab(RequestTab::Headers, "Headers", Some(HEADERS.len())))
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

    fn body_tab(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let format_radio = |format: BodyFormat, id: &'static str, label: &'static str| {
            Radio::new(id)
                .label(label)
                .checked(self.body_format == format)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.body_format = format;
                    cx.notify();
                }))
        };

        let rows = FIELDS.iter().enumerate().map(|(ix, (name, generator))| {
            let enabled = self.field_enabled[ix];
            Self::table_row(
                t,
                Checkbox::new(("field-enabled", ix))
                    .checked(enabled)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        this.field_enabled[ix] = *checked;
                        cx.notify();
                    }))
                    .into_any_element(),
                div()
                    .text_color(if enabled { t.text } else { t.text_subtle })
                    .child(*name)
                    .into_any_element(),
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .text_color(if enabled { t.text } else { t.text_subtle })
                    .child(*generator)
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
                    .child(format_radio(BodyFormat::RawJson, "format-raw", "raw"))
                    .when(self.body_format == BodyFormat::RawJson, |this| {
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

    fn headers_tab(&self, t: &Tokens) -> impl IntoElement {
        let rows = HEADERS.iter().enumerate().map(|(ix, (key, value))| {
            Self::table_row(
                t,
                Checkbox::new(("header-enabled", ix))
                    .checked(true)
                    .into_any_element(),
                div().child(*key).into_any_element(),
                div().child(*value).into_any_element(),
            )
            .text_color(t.text)
        });
        Self::table(t)
            .child(Self::table_header(t, "Value"))
            .children(rows)
            .child(Self::placeholder_row(t, "Value"))
    }

    fn run_tab(&self, t: &Tokens) -> impl IntoElement {
        let setting = |label: &'static str, value: &'static str| {
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
            .child(setting("Send count", "1"))
            .child(setting("Delay between requests (ms)", "0"))
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

    fn workbench(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = v_flex()
            .size_full()
            .bg(t.bg_app)
            .child(self.request_header(t))
            .child(self.request_line(t, cx))
            .child(div().h(spacing::MD).flex_none())
            .child(self.request_tabs(t, cx))
            .child(
                div()
                    .id("request-editor")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(spacing::XL)
                    .map(|this| match self.request_tab {
                        RequestTab::Body => this.child(self.body_tab(t, cx).into_any_element()),
                        RequestTab::Headers => this.child(self.headers_tab(t).into_any_element()),
                        RequestTab::Run => this.child(self.run_tab(t).into_any_element()),
                    }),
            );

        v_flex()
            .size_full()
            .bg(t.bg_app)
            .child(self.tab_strip(t, cx))
            .child(
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

fn main() {
    gpui_kit::application().with_assets(AllAssets).run(|cx| {
        gpui_kit::init(cx);
        Tokens::postman_dark().install(cx);

        let mut options = TitleBar::window_options();
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some("fakeman".into());
        }
        options.window_bounds = Some(WindowBounds::centered(size(px(1280.), px(800.)), cx));

        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| FakemanWindow::new(window, cx))
        })
        .expect("failed to open the fakeman window");
    });
}
