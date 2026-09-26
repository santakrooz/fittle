//! Share card: the exported image above a caption strip (target, rig,
//! integration, date, and site unless private), in Fittle's dark tokens.
//! Text is drawn with the design-system fonts (OFL, embedded) through
//! `swash`, which is pure Rust.

use fittle_core::Info;
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::shape::ShapeContext;
use swash::zeno::{Format, Vector};

use crate::decode::Image;

static DISPLAY: &[u8] = include_bytes!("../fonts/BricolageGrotesque.ttf");
static UI: &[u8] = include_bytes!("../fonts/Inter.ttf");
static MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono.ttf");

// Design tokens (dark), docs/design-system/tokens.json.
const BG: [u8; 3] = [0x0A, 0x0E, 0x16];
const LINE: [u8; 3] = [0x1F, 0x2A, 0x3D];
const TEXT: [u8; 3] = [0xE8, 0xEC, 0xF3];
const MUTED: [u8; 3] = [0x83, 0x91, 0xA7];
const ACCENT: [u8; 3] = [0xF5, 0xB8, 0x3D];

/// Cards are laid out on at least this width, so text stays legible.
const MIN_WIDTH: usize = 640;

/// Layout unit: 1/1000 of the card width.
fn unit(width: usize) -> f32 {
    width.max(MIN_WIDTH) as f32 / 1000.0
}

/// Size of the card for an image `w` × `h`: (width, height, strip height).
pub fn size(w: usize, h: usize) -> (usize, usize, usize) {
    let width = w.max(MIN_WIDTH);
    let strip = (196.0 * unit(width)).round() as usize;
    (width, h + strip, strip)
}

/// The caption's lines, from `fittle info` facts.
#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub title: String,
    pub subtitle: Option<String>,
    pub rig: String,
    pub details: String,
    pub integration: Option<String>,
}

fn duration(s: f64) -> String {
    let t = s.round() as i64;
    match (t / 3600, (t % 3600) / 60, t % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m}m {s}s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn caption(info: &Info, stretch: &str, private: bool) -> Caption {
    let f = &info.fields;
    let title = f
        .object
        .as_ref()
        .map(|o| o.value.trim().to_string())
        .or_else(|| info.target.as_ref().map(|t| t.id.clone()))
        .unwrap_or_else(|| "Untitled".into());
    let subtitle = info
        .target
        .as_ref()
        .and_then(|t| t.common_name.clone().or_else(|| Some(t.kind.clone())))
        .filter(|s| !s.is_empty() && !title.eq_ignore_ascii_case(s));

    let mut rig: Vec<String> = Vec::new();
    match (&info.origin.scope, &f.telescope, &f.camera) {
        (Some(s), _, _) => rig.push(s.name.clone()),
        (None, t, c) => {
            let serial = |k: &str| f.serials.iter().any(|s| s.key == k);
            if let Some(t) = t.as_ref().filter(|_| !serial("TELESCOP")) {
                rig.push(t.value.trim().to_string());
            }
            if let Some(c) = c {
                rig.push(c.value.trim().to_string());
            }
        }
    }
    if let Some(x) = &f.filter {
        rig.push(format!("{} filter", x.value.trim()));
    }
    match (&f.stack_count, &f.exposure_s) {
        (Some(n), Some(e)) if n.value > 1 => rig.push(format!("{} × {} s", n.value, num(e.value))),
        (_, Some(e)) => rig.push(format!("{} s", num(e.value))),
        _ => {}
    }

    let mut details: Vec<String> = Vec::new();
    if let Some(d) = info
        .derived
        .session_night
        .as_ref()
        .map(|n| n.value.clone())
        .or_else(|| {
            f.date_obs
                .as_ref()
                .and_then(|d| d.value.get(..10).map(str::to_string))
        })
    {
        details.push(d);
    }
    if !private {
        if let (Some(lat), Some(lon)) = (&f.site_lat, &f.site_lon) {
            let ns = if lat.value >= 0.0 { 'N' } else { 'S' };
            let ew = if lon.value >= 0.0 { 'E' } else { 'W' };
            details.push(format!(
                "{:.1}°{ns} {:.1}°{ew}",
                lat.value.abs(),
                lon.value.abs()
            ));
        }
    }
    details.push(format!("display stretch: {stretch}"));
    details.push("made with Fittle".into());

    let counted = match (&f.stack_count, &f.exposure_s) {
        (Some(n), Some(e)) => Some(n.value as f64 * e.value),
        (None, Some(e)) => Some(e.value),
        _ => None,
    };
    let total = f.total_integration_s.as_ref().map(|t| t.value).or(counted);
    Caption {
        title,
        subtitle,
        rig: rig.join(" · "),
        details: details.join("  ·  "),
        integration: total.map(duration),
    }
}

struct Canvas {
    width: usize,
    height: usize,
    /// Interleaved RGB, 0–1.
    rgb: Vec<f32>,
}

impl Canvas {
    fn blend(&mut self, x: i32, y: i32, color: [u8; 3], alpha: f32) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height || alpha <= 0.0 {
            return;
        }
        let at = (y as usize * self.width + x as usize) * 3;
        for (px, &v) in self.rgb[at..at + 3].iter_mut().zip(&color) {
            *px = *px * (1.0 - alpha) + v as f32 / 255.0 * alpha;
        }
    }

    fn fill(&mut self, y0: usize, y1: usize, color: [u8; 3]) {
        for y in y0..y1.min(self.height) {
            for x in 0..self.width {
                self.blend(x as i32, y as i32, color, 1.0);
            }
        }
    }
}

struct Style {
    font: &'static [u8],
    size: f32,
    weight: f32,
    color: [u8; 3],
}

struct Pen {
    shape: ShapeContext,
    scale: ScaleContext,
}

impl Pen {
    fn glyphs(&mut self, s: &Style, text: &str) -> (Vec<(u16, f32)>, f32) {
        let font = FontRef::from_index(s.font, 0).expect("embedded font");
        let vars = [("wght", s.weight), ("opsz", s.size.clamp(12.0, 32.0))];
        let mut shaper = self
            .shape
            .builder(font)
            .size(s.size)
            .variations(&vars[..])
            .build();
        shaper.add_str(text);
        let mut out = Vec::new();
        let mut x = 0.0;
        shaper.shape_with(|cluster| {
            for g in cluster.glyphs {
                out.push((g.id, x + g.x));
                x += g.advance;
            }
        });
        (out, x)
    }

    fn width(&mut self, s: &Style, text: &str) -> f32 {
        self.glyphs(s, text).1
    }

    /// Longest prefix of `text` (plus an ellipsis) that fits `max` pixels.
    fn fit(&mut self, s: &Style, text: &str, max: f32) -> String {
        if self.width(s, text) <= max {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let (mut lo, mut hi) = (0, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let t: String = chars[..mid].iter().collect::<String>() + "…";
            if self.width(s, &t) <= max {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
            + "…"
    }

    fn draw(&mut self, c: &mut Canvas, s: &Style, text: &str, x: f32, baseline: f32) {
        let font = FontRef::from_index(s.font, 0).expect("embedded font");
        let (glyphs, _) = self.glyphs(s, text);
        let vars = [("wght", s.weight), ("opsz", s.size.clamp(12.0, 32.0))];
        let mut scaler = self
            .scale
            .builder(font)
            .size(s.size)
            .hint(false)
            .variations(&vars[..])
            .build();
        for (id, gx) in glyphs {
            let px = x + gx;
            let Some(img) = Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .offset(Vector::new(px.fract(), 0.0))
                .render(&mut scaler, id)
            else {
                continue;
            };
            let p = img.placement;
            for yy in 0..p.height as i32 {
                for xx in 0..p.width as i32 {
                    let a = img.data[(yy * p.width as i32 + xx) as usize] as f32 / 255.0;
                    c.blend(
                        px.floor() as i32 + p.left + xx,
                        baseline.round() as i32 - p.top + yy,
                        s.color,
                        a,
                    );
                }
            }
        }
    }
}

/// Compose the card from a stretched image (planes 0–1). Mono images are
/// shown in grey. Returns an RGB image.
pub fn compose(img: &Image, cap: &Caption) -> Image {
    let (width, height, strip) = size(img.width, img.height);
    let mut c = Canvas {
        width,
        height,
        rgb: vec![0.0; width * height * 3],
    };
    c.fill(0, height, BG);
    // Image, centred when the card is wider than it.
    let x0 = (width - img.width) / 2;
    let plane = img.width * img.height;
    for y in 0..img.height {
        for x in 0..img.width {
            let at = ((y * width) + x0 + x) * 3;
            for ch in 0..3 {
                c.rgb[at + ch] =
                    img.data[ch.min(img.planes - 1) * plane + y * img.width + x].clamp(0.0, 1.0);
            }
        }
    }
    let u = unit(width);
    let top = img.height as f32;
    c.fill(img.height, img.height + (u.round() as usize).max(1), LINE);

    let pad = 40.0 * u;
    let title = Style {
        font: DISPLAY,
        size: 46.0 * u,
        weight: 700.0,
        color: TEXT,
    };
    let sub = Style {
        font: UI,
        size: 22.0 * u,
        weight: 400.0,
        color: MUTED,
    };
    let rig = Style {
        font: UI,
        size: 22.0 * u,
        weight: 500.0,
        color: TEXT,
    };
    let mono = Style {
        font: MONO,
        size: 15.0 * u,
        weight: 400.0,
        color: MUTED,
    };
    let big = Style {
        font: DISPLAY,
        size: 46.0 * u,
        weight: 700.0,
        color: ACCENT,
    };
    let label = Style {
        font: UI,
        size: 12.5 * u,
        weight: 600.0,
        color: MUTED,
    };

    let mut pen = Pen {
        shape: ShapeContext::new(),
        scale: ScaleContext::new(),
    };
    let right = width as f32 - pad;
    // Right block: integration.
    let mut text_right = right;
    if let Some(total) = &cap.integration {
        let w = pen.width(&big, total).max(pen.width(&label, "INTEGRATION"));
        let bx = right - w;
        let tw = pen.width(&big, total);
        pen.draw(&mut c, &big, total, right - tw, top + pad + 40.0 * u);
        let lw = pen.width(&label, "INTEGRATION");
        pen.draw(
            &mut c,
            &label,
            "INTEGRATION",
            right - lw,
            top + pad + 72.0 * u,
        );
        text_right = bx - 32.0 * u;
    }
    let avail = (text_right - pad).max(40.0 * u);
    let base1 = top + pad + 40.0 * u;
    let t = pen.fit(&title, &cap.title, avail);
    pen.draw(&mut c, &title, &t, pad, base1);
    if let Some(s) = &cap.subtitle {
        let tx = pad + pen.width(&title, &t) + 16.0 * u;
        let room = text_right - tx;
        if room > 60.0 * u {
            let s = pen.fit(&sub, s, room);
            pen.draw(&mut c, &sub, &s, tx, base1);
        }
    }
    let base2 = base1 + 44.0 * u;
    let r = pen.fit(&rig, &cap.rig, avail);
    pen.draw(&mut c, &rig, &r, pad, base2);
    let base3 = (top + strip as f32 - pad).max(base2 + 30.0 * u);
    let d = pen.fit(&mono, &cap.details, right - pad);
    pen.draw(&mut c, &mono, &d, pad, base3);

    // Back to planar.
    let n = width * height;
    let mut data = vec![0f32; n * 3];
    for i in 0..n {
        for ch in 0..3 {
            data[ch * n + i] = c.rgb[i * 3 + ch];
        }
    }
    Image {
        width,
        height,
        planes: 3,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration(20.0), "20 s");
        assert_eq!(duration(1800.0), "30 min");
        assert_eq!(duration(4200.0), "1h 10m");
        assert_eq!(duration(68220.0), "18h 57m");
        assert_eq!(duration(90.0), "1m 30s");
    }

    #[test]
    fn card_size_and_text_ink() {
        let img = Image {
            width: 800,
            height: 400,
            planes: 1,
            data: vec![0.5; 800 * 400],
        };
        let cap = Caption {
            title: "NGC 6995".into(),
            subtitle: Some("Eastern Veil".into()),
            rig: "ZWO Seestar S50 · LP filter · 90 × 20 s".into(),
            details: "2026-09-24  ·  display stretch: auto STF  ·  made with Fittle".into(),
            integration: Some("30 min".into()),
        };
        let card = compose(&img, &cap);
        let (w, h, strip) = size(800, 400);
        assert_eq!((card.width, card.height, card.planes), (w, h, 3));
        // The strip has text: some pixels well above the background.
        let n = w * h;
        let lit = (400 * w..n).filter(|&i| card.data[i] > 0.5).count();
        assert!(lit > 500 && lit < strip * w / 4, "{lit} text pixels");
        // Amber integration digits: red high, blue low, on the right.
        let amber = (400 * w..n)
            .filter(|&i| i % w > w / 2 && card.data[i] > 0.8 && card.data[2 * n + i] < 0.4)
            .count();
        assert!(amber > 50, "{amber} amber pixels");
    }
}
