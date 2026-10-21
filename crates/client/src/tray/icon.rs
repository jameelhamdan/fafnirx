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
mod tests {
    use super::*;

    fn pixel(img: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn drive_icon_is_drawn_in_the_status_colour() {
        for (l, want) in [
            (Light::Ok, [0x40_i32, 0xbf, 0x6b]),
            (Light::Error, [0xdb, 0x3a, 0x3a]),
            (Light::Paused, [0xe6, 0xaa, 0x32]),
            (Light::Stopped, [0x8c, 0x8c, 0x8c]),
        ] {
            let img = render(l, false).unwrap();
            assert_eq!(img.len(), (SIZE * SIZE * 4) as usize);
            assert_eq!(pixel(&img, 0, 0)[3], 0, "corners are transparent");
            let face = pixel(&img, 45, 20);
            assert_eq!(face[3], 255);
            assert!((0..3).all(|i| (face[i] as i32 - want[i]).abs() <= 1), "{l:?} face {face:?}");
        }
    }

    #[test]
    fn activity_light_changes_when_on() {
        let (off, on) = (render(Light::Ok, false).unwrap(), render(Light::Ok, true).unwrap());
        let (a, b) = (pixel(&off, 49, 46), pixel(&on, 49, 46));
        assert!(b[0] > 240 && b[1] > 230, "lit light is bright: {b:?}");
        assert!(a[0] < 80, "unlit light is dark: {a:?}");
        assert_eq!(pixel(&off, 45, 20), pixel(&on, 45, 20), "only the light differs");
    }
}
