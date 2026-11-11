//! Design tokens for fakeman's Postman-style dark theme.
//!
//! Every colour, font size, spacing value and fixed dimension the UI uses lives
//! here, so views never hard-code styles. The palette follows Postman's dark
//! theme (neutral greys, #FF6C37 brand orange, blue Send, method colours). The
//! token roles (surface levels, text and border tiers, an 11-16 px type scale,
//! 4/6 px radii) are adapted from Bruno's theme files
//! (https://github.com/usebruno/bruno, packages/bruno-app/src/themes, MIT).

use gpui_kit::component::{Theme as KitTheme, ThemeMode};
use gpui_kit::{App, Global, Hsla, rgb, rgba};

pub mod font_size {
    use gpui_kit::{Pixels, px};

    pub const XS: Pixels = px(11.);
    pub const SM: Pixels = px(12.);
    pub const BASE: Pixels = px(13.);
    pub const MD: Pixels = px(14.);
}

pub mod spacing {
    use gpui_kit::{Pixels, px};

    pub const XXS: Pixels = px(2.);
    pub const XS: Pixels = px(4.);
    pub const SM: Pixels = px(6.);
    pub const MD: Pixels = px(8.);
    pub const LG: Pixels = px(12.);
    pub const XL: Pixels = px(16.);
}

pub mod corner {
    use gpui_kit::{Pixels, px};

    pub const SM: Pixels = px(4.);
    pub const MD: Pixels = px(6.);
}

pub mod dim {
    use gpui_kit::{Pixels, px};

    pub const SIDEBAR_WIDTH: Pixels = px(280.);
    pub const SIDEBAR_MIN: Pixels = px(200.);
    pub const SIDEBAR_MAX: Pixels = px(480.);
    pub const RESPONSE_HEIGHT: Pixels = px(260.);
    pub const RESPONSE_MIN: Pixels = px(120.);
    pub const BAR_HEIGHT: Pixels = px(40.);
    pub const CONTROL_HEIGHT: Pixels = px(28.);
    pub const REQUEST_LINE_HEIGHT: Pixels = px(36.);
    pub const ROW_HEIGHT: Pixels = px(32.);
    pub const TREE_ROW_HEIGHT: Pixels = px(28.);
    pub const STATUS_BAR_HEIGHT: Pixels = px(26.);
    pub const TAB_MAX_WIDTH: Pixels = px(220.);
    pub const METHOD_SELECT_WIDTH: Pixels = px(108.);
    pub const SEND_WIDTH: Pixels = px(104.);
    pub const SEARCH_WIDTH: Pixels = px(360.);
    pub const CHECK_COLUMN: Pixels = px(36.);
    pub const TRAILING_COLUMN: Pixels = px(120.);
    pub const METHOD_TAG_WIDTH: Pixels = px(38.);
    pub const LOGO: Pixels = px(22.);
    pub const ACCENT_LINE: Pixels = px(2.);
    pub const ICON_SM: Pixels = px(14.);
    pub const ICON_MD: Pixels = px(16.);
    pub const ICON_EMPTY_STATE: Pixels = px(40.);
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[expect(
    dead_code,
    reason = "v1 sends POST only; the other method colours are kept with the palette"
)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DEL",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Tokens {
    pub bg_header: Hsla,
    pub bg_app: Hsla,
    pub bg_sidebar: Hsla,
    pub bg_raised: Hsla,
    pub bg_hover: Hsla,
    pub bg_selected: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_subtle: Hsla,
    pub text_on_accent: Hsla,
    pub brand: Hsla,
    pub link: Hsla,
    pub send: Hsla,
    pub send_hover: Hsla,
    pub send_divider: Hsla,
    pub success: Hsla,
    pub method_get: Hsla,
    pub method_post: Hsla,
    pub method_put: Hsla,
    pub method_patch: Hsla,
    pub method_delete: Hsla,
}

impl Global for Tokens {}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

impl Tokens {
    pub fn postman_dark() -> Self {
        Self {
            bg_header: hex(0x1c1c1c),
            bg_app: hex(0x212121),
            bg_sidebar: hex(0x212121),
            bg_raised: hex(0x2b2b2b),
            bg_hover: hex(0x2e2e2e),
            bg_selected: hex(0x333333),
            border: hex(0x303030),
            border_strong: hex(0x404040),
            text: hex(0xf0f0f0),
            text_muted: hex(0xa6a6a6),
            text_subtle: hex(0x6b6b6b),
            text_on_accent: hex(0xffffff),
            brand: hex(0xff6c37),
            link: hex(0x5ca9fb),
            send: hex(0x097bed),
            send_hover: hex(0x0265d2),
            send_divider: rgba(0xffffff40).into(),
            success: hex(0x6bdd9a),
            method_get: hex(0x6bdd9a),
            method_post: hex(0xffe47e),
            method_put: hex(0x74aef6),
            method_patch: hex(0xc0a8e1),
            method_delete: hex(0xf79a8e),
        }
    }

    pub fn method(&self, method: Method) -> Hsla {
        match method {
            Method::Get => self.method_get,
            Method::Post => self.method_post,
            Method::Put => self.method_put,
            Method::Patch => self.method_patch,
            Method::Delete => self.method_delete,
        }
    }

    /// Installs the tokens as a global and restyles gpui-kit's primitives
    /// (inputs, checkboxes, radios, resize handles, toasts) to match them.
    pub fn install(self, cx: &mut App) {
        cx.set_global(self);
        KitTheme::change(ThemeMode::Dark, None, cx);
        KitTheme::update(cx, |theme| {
            theme.font_size = font_size::BASE;
            theme.radius = corner::SM;
            let colors = &mut theme.colors;
            colors.background = self.bg_app;
            colors.foreground = self.text;
            colors.muted_foreground = self.text_muted;
            colors.border = self.border;
            colors.input = self.border_strong;
            colors.ring = self.border_strong;
            colors.caret = self.text;
            colors.primary = self.send;
            colors.primary_hover = self.send_hover;
            colors.primary_active = self.send_hover;
            colors.primary_foreground = self.text_on_accent;
            colors.title_bar = self.bg_header;
            colors.title_bar_border = self.border;
            colors.popover = self.bg_raised;
            colors.popover_foreground = self.text;
            colors.drag_border = self.brand;
        });
    }
}

pub trait ActiveTokens {
    fn tokens(&self) -> &Tokens;
}

impl ActiveTokens for App {
    fn tokens(&self) -> &Tokens {
        self.global::<Tokens>()
    }
}
