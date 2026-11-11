#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui_kit::assets::Assets;
use gpui_kit::component::{
    ActiveTheme as _, StyledExt as _, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    resizable::{h_resizable, resizable_panel},
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use gpui_kit::*;

const PLACEHOLDER_ENDPOINTS: [&str; 3] = ["Create user", "Signup", "Orders"];

struct FakemanWindow {
    url: Entity<InputState>,
    selected: usize,
}

impl FakemanWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://api.example.com/users"));
        Self { url, selected: 0 }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value();
        let message = if url.is_empty() {
            "Send pressed with an empty URL.".to_string()
        } else {
            format!("Send pressed for {url}")
        };
        println!("{message}");
        window.push_notification(Notification::info(message).title("Send"), cx);
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = PLACEHOLDER_ENDPOINTS.iter().enumerate().map(|(ix, name)| {
            SidebarMenuItem::new(*name)
                .active(ix == self.selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = ix;
                    cx.notify();
                }))
        });

        Sidebar::new("endpoints")
            .w_full()
            .header(SidebarHeader::new().child(div().font_bold().child("fakeman")))
            .child(SidebarGroup::new("Endpoints").child(SidebarMenu::new().children(items)))
    }

    fn main_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().secondary)
                            .text_sm()
                            .font_bold()
                            .child("POST"),
                    )
                    .child(div().flex_1().child(Input::new(&self.url)))
                    .child(
                        Button::new("send")
                            .primary()
                            .label("Send")
                            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .p_4()
                    .text_color(cx.theme().muted_foreground)
                    .child(PLACEHOLDER_ENDPOINTS[self.selected]),
            )
    }
}

impl Render for FakemanWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child("fakeman"))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("main-split")
                        .child(
                            resizable_panel()
                                .size(px(240.))
                                .size_range(px(160.)..px(480.))
                                .fixed()
                                .child(self.sidebar(cx)),
                        )
                        .child(resizable_panel().child(self.main_area(cx))),
                ),
            )
    }
}

fn main() {
    gpui_kit::application().with_assets(Assets).run(|cx| {
        gpui_kit::init(cx);

        let mut options = TitleBar::window_options();
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some("fakeman".into());
        }
        options.window_bounds = Some(WindowBounds::centered(size(px(1100.), px(720.)), cx));

        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| FakemanWindow::new(window, cx))
        })
        .expect("failed to open the fakeman window");
    });
}
