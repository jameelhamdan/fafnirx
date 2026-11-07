//! The tray icon: a hard drive (`icon.svg`) coloured by the state of the client. The activity light
//! is lit (bright yellow) while syncing.

use anyhow::{Context, Result};
use tray_icon::Icon;

pub const SIZE: u32 = 64;

const TEMPLATE: &str = include_str!("icon.svg");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Light {
    Ok,
    Error,
    Paused,
    Stopped,
}

fn body_colour(l: Light) -> &'static str {
    match l {
        Light::Ok => "#40bf6b",
        Light::Error => "#db3a3a",
        Light::Paused => "#e6aa32",
        Light::Stopped => "#8c8c8c",
    }
}

/// RGBA pixels (straight alpha) of the drive in the colour of `l`.
fn render(l: Light, led_on: bool) -> Result<Vec<u8>> {
    let (led, glow) = if led_on { ("#fff48c", "0.35") } else { ("#1a1a1a", "0") };
    let svg = TEMPLATE.replace("{body}", body_colour(l)).replace("{led}", led).replace("{glow}", glow);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).context("parse icon.svg")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).context("allocate icon")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

pub fn icon(l: Light, led_on: bool) -> Result<Icon> {
    Icon::from_rgba(render(l, led_on)?, SIZE, SIZE).context("build tray icon")
}

#[cfg(test)]
#[path = "../../tests/unit/tray/icon.rs"]
mod tests;
