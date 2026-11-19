//! Tray status icons, drawn in code so they stay crisp and need no asset files: the
//! keyboard face, green with open eyes while the keyboard is on, orange and asleep while
//! it is paused. Same shapes as the face in the app (`ui/face.slint`), on a 32 px grid.
//! Dev builds tint both faces (violet on, pink paused) so they stand apart from the
//! installed app.

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const SIZE: u32 = 32;
const INK: [f32; 3] = [18.0, 18.0, 18.0];
const GREEN: [f32; 3] = [60.0, 207.0, 127.0];
const ORANGE: [f32; 3] = [255.0, 79.0, 18.0];
const DEV_ON: [f32; 3] = [139.0, 108.0, 255.0];
const DEV_PAUSED: [f32; 3] = [255.0, 70.0, 170.0];

fn body_on() -> [f32; 3] {
    if keyless_core::DEV_BUILD { DEV_ON } else { GREEN }
}

fn body_paused() -> [f32; 3] {
    if keyless_core::DEV_BUILD { DEV_PAUSED } else { ORANGE }
}
/// Half the outline width.
const STROKE: f32 = 1.1;

pub fn on() -> Image {
    draw(body_on(), |x, y| {
        let eye = |cx: f32| ((x - cx).powi(2) + (y - 16.5).powi(2)).sqrt() - 2.6;
        eye(11.5).min(eye(20.5))
    })
}

pub fn paused() -> Image {
    draw(body_paused(), |x, y| {
        let lid = |x0: f32| curve_distance(x, y, (x0, 16.3), (x0 + 3.2, 19.1), (x0 + 6.4, 16.3)) - STROKE;
        lid(8.3).min(lid(17.3))
    })
}

/// `glyph` returns the signed distance (in pixels) from a point to the eyes' edge.
fn draw(body: [f32; 3], glyph: impl Fn(f32, f32) -> f32) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(SIZE, SIZE);
    for (i, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let x = (i as u32 % SIZE) as f32 + 0.5;
        let y = (i as u32 / SIZE) as f32 + 0.5;
        // Outline centerline of the face: a 27 x 19 rounded rectangle.
        let d = rounded_rect_distance(x, y, (16.0, 16.5), (13.5, 9.5), 5.0);
        let alpha = coverage(d - STROKE);
        let ink = (1.0 - coverage(d + STROKE)).max(coverage(glyph(x, y)));
        let mix = |c: usize| (body[c] + (INK[c] - body[c]) * ink).round() as u8;
        *pixel = Rgba8Pixel { r: mix(0), g: mix(1), b: mix(2), a: (alpha * 255.0).round() as u8 };
    }
    Image::from_rgba8(buffer)
}

/// Anti-aliased coverage of a pixel from its signed distance to an edge.
fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn rounded_rect_distance(x: f32, y: f32, center: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let qx = (x - center.0).abs() - (half.0 - radius);
    let qy = (y - center.1).abs() - (half.1 - radius);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Distance to a quadratic curve, measured against a fine polyline.
fn curve_distance(x: f32, y: f32, a: (f32, f32), control: (f32, f32), b: (f32, f32)) -> f32 {
    const STEPS: usize = 12;
    let point = |t: f32| {
        let u = 1.0 - t;
        (
            u * u * a.0 + 2.0 * u * t * control.0 + t * t * b.0,
            u * u * a.1 + 2.0 * u * t * control.1 + t * t * b.1,
        )
    };
    (0..STEPS)
        .map(|i| segment_distance(x, y, point(i as f32 / STEPS as f32), point((i + 1) as f32 / STEPS as f32)))
        .fold(f32::MAX, f32::min)
}

fn segment_distance(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py, bx, by) = (x - a.0, y - a.1, b.0 - a.0, b.1 - a.1);
    let t = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    ((px - bx * t).powi(2) + (py - by * t).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: u32, y: u32) -> Rgba8Pixel {
        let buffer = image.to_rgba8().expect("icons are RGBA");
        buffer.as_slice()[(y * SIZE + x) as usize]
    }

    #[test]
    fn faces_have_transparent_corners_and_solid_bodies() {
        for icon in [on(), paused()] {
            assert_eq!(pixel(&icon, 0, 0).a, 0);
            assert_eq!(pixel(&icon, 31, 31).a, 0);
            assert_eq!(pixel(&icon, 16, 10).a, 255);
        }
        let rgb = |c: [f32; 3]| (c[0] as u8, c[1] as u8, c[2] as u8);
        let body = pixel(&on(), 16, 22);
        assert_eq!((body.r, body.g, body.b), rgb(body_on()));
        let body = pixel(&paused(), 16, 22);
        assert_eq!((body.r, body.g, body.b), rgb(body_paused()));
    }

    #[test]
    fn open_eyes_and_closed_lids_are_dark() {
        assert!(pixel(&on(), 11, 16).r < 40);
        assert!(pixel(&paused(), 11, 17).r < 120);
        // Where the open eye would be, the sleeping face shows its body color.
        assert_eq!(pixel(&paused(), 11, 14).r, body_paused()[0] as u8);
    }
}
