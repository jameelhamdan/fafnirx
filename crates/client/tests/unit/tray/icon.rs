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
