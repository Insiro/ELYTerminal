//! The interface's family and size, through Ely's theme, the right column's scale, and every glyph's antialiasing.

use std::panic::Location;

use ely_gpui_component::theme::{ActiveTheme, Theme};
use gpui::{
    AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Styled, TextRenderingMode, Window, rems,
};

use crate::settings::{Antialias, SettingsStore};

/// Ely's base text, which `interface_size` scales.
const BASE: f32 = 13.0;

/// gpui's name for the system's interface font.
const SYSTEM: &str = ".SystemUIFont";

/// Sets Ely's family and scale and gpui's antialiasing from the settings.
pub fn init(cx: &mut App) {
    let mut worn = None;
    let mut scaled = None;
    let mut wear = move |cx: &mut App| {
        let settings = SettingsStore::get(cx);
        let size = settings.interface_size;
        let scale = settings.panel_scale;
        let family = match settings.interface_font.as_str() {
            "" => SYSTEM.to_string(),
            family => family.to_string(),
        };
        let mode = match settings.text_antialias {
            Antialias::Grayscale => TextRenderingMode::Grayscale,
            Antialias::Subpixel => TextRenderingMode::Subpixel,
        };
        if cx.text_rendering_mode() != mode {
            log::info!("app: glyphs antialias {mode:?}");
            cx.set_text_rendering_mode(mode);
            // A refresh draws cached views again.
            cx.refresh_windows();
        }
        if scaled.is_some_and(|was| was != scale) {
            log::info!("app: the right column is at {scale}%");
            // Cached views keep their layout until drawn again.
            cx.refresh_windows();
        }
        scaled = Some(scale);
        if worn.as_ref() != Some(&(size, family.clone())) {
            log::info!("app: the interface's text is {family} at {size} px");
            worn = Some((size, family.clone()));
            Theme::update(cx, |theme| {
                theme.font_scale = f32::from(size) / BASE;
                theme.font_family = family.into();
            });
        }
    };
    wear(cx);
    cx.observe_global::<SettingsStore>(wear).detach();
}

/// A window's root in Ely's family and scale.
pub fn dress(root: Div, cx: &App) -> Div {
    let theme = cx.theme();
    // gpui's 1 rem, scaled, for text of no size.
    root.text_size(rems(theme.font_scale))
        .font_family(theme.font_family.clone())
}

/// The right column's content, every rem in it scaled by `panel_scale`.
pub fn panel(child: impl IntoElement, cx: &App) -> Scaled {
    Scaled {
        scale: f32::from(SettingsStore::get(cx).panel_scale) / 100.0,
        child: child.into_any_element(),
    }
}

/// An element laid out and drawn at a scaled rem, so Ely's sizes and text shrink with it.
pub struct Scaled {
    scale: f32,
    child: AnyElement,
}

impl Scaled {
    fn rem(&self, window: &Window) -> Pixels {
        window.rem_size() * self.scale
    }
}

impl IntoElement for Scaled {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Scaled {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let rem = self.rem(window);
        let id = window.with_rem_size(Some(rem), |window| self.child.request_layout(window, cx));
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem = self.rem(window);
        window.with_rem_size(Some(rem), |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem = self.rem(window);
        window.with_rem_size(Some(rem), |window| self.child.paint(window, cx));
    }
}
