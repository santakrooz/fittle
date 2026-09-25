//! Debayer for one-shot-colour (CFA) frames: 2×2 superpixel for viewing
//! (fast, half size; a preview downsamples anyway) and full-size bilinear for
//! exports.

use rayon::prelude::*;

use crate::decode::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ch {
    R = 0,
    G = 1,
    B = 2,
}

/// A 2×2 colour filter pattern as it applies to the stored rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cfa {
    cells: [[Ch; 2]; 2],
}

impl Cfa {
    /// `pattern` (RGGB, GRBG, GBRG, BGGR) as written in BAYERPAT, which by
    /// convention describes the image read top-down. `x_off`/`y_off` are
    /// XBAYROFF/YBAYROFF. `bottom_up` is true when ROWORDER says the stored
    /// rows run bottom-up, so row 0 is the image's last row.
    pub fn new(
        pattern: &str,
        x_off: i64,
        y_off: i64,
        bottom_up: bool,
        height: usize,
    ) -> Option<Cfa> {
        let p: Vec<Ch> = pattern
            .trim()
            .to_uppercase()
            .chars()
            .map(|c| match c {
                'R' => Some(Ch::R),
                'G' => Some(Ch::G),
                'B' => Some(Ch::B),
                _ => None,
            })
            .collect::<Option<_>>()?;
        let [a, b, c, d] = p[..] else { return None };
        let top_down = [[a, b], [c, d]];
        // Row parity of stored row 0 in top-down terms.
        let row0 = if bottom_up { (height - 1) % 2 } else { 0 };
        let mut cells = [[Ch::G; 2]; 2];
        for (sy, row) in cells.iter_mut().enumerate() {
            for (sx, cell) in row.iter_mut().enumerate() {
                let ty = if bottom_up {
                    (row0 + 2 - sy % 2) % 2
                } else {
                    sy
                };
                *cell = top_down[(ty + y_off.rem_euclid(2) as usize) % 2]
                    [(sx + x_off.rem_euclid(2) as usize) % 2];
            }
        }
        Some(Cfa { cells })
    }
}

/// Full-resolution bilinear debayer: each missing colour is the mean of the
/// same-colour sensor pixels in the 3×3 neighbourhood (edges clamp).
pub fn bilinear(img: &Image, cfa: Cfa) -> Image {
    let (w, h) = (img.width, img.height);
    let plane = w * h;
    let mut data = vec![0f32; plane * 3];
    let (r, rest) = data.split_at_mut(plane);
    let (g, b) = rest.split_at_mut(plane);
    r.par_chunks_mut(w)
        .zip(g.par_chunks_mut(w))
        .zip(b.par_chunks_mut(w))
        .enumerate()
        .for_each(|(y, ((rr, gg), bb))| {
            for x in 0..w {
                let mut sum = [0f32; 3];
                let mut n = [0u32; 3];
                let own = cfa.cells[y % 2][x % 2] as usize;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (sx, sy) = (x as i64 + dx, y as i64 + dy);
                        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                            continue;
                        }
                        let c = cfa.cells[sy as usize % 2][sx as usize % 2] as usize;
                        sum[c] += img.data[sy as usize * w + sx as usize];
                        n[c] += 1;
                    }
                }
                let v = |c: usize| {
                    if c == own {
                        img.data[y * w + x]
                    } else if n[c] > 0 {
                        sum[c] / n[c] as f32
                    } else {
                        0.0
                    }
                };
                rr[x] = v(0);
                gg[x] = v(1);
                bb[x] = v(2);
            }
        });
    Image {
        width: w,
        height: h,
        planes: 3,
        data,
    }
}

/// Superpixel debayer of a single-plane CFA image into a half-size RGB image.
pub fn superpixel(img: &Image, cfa: Cfa) -> Image {
    let (w, h) = (img.width / 2, img.height / 2);
    let plane = w * h;
    let mut data = vec![0f32; plane * 3];
    let (r, rest) = data.split_at_mut(plane);
    let (g, b) = rest.split_at_mut(plane);
    r.par_chunks_mut(w)
        .zip(g.par_chunks_mut(w))
        .zip(b.par_chunks_mut(w))
        .enumerate()
        .for_each(|(y, ((rr, gg), bb))| {
            let top = &img.data[(2 * y) * img.width..(2 * y + 1) * img.width];
            let bot = &img.data[(2 * y + 1) * img.width..(2 * y + 2) * img.width];
            for x in 0..w {
                let px = [[top[2 * x], top[2 * x + 1]], [bot[2 * x], bot[2 * x + 1]]];
                let (mut sr, mut sg, mut sb, mut ng) = (0.0, 0.0, 0.0, 0.0);
                for (dy, row) in px.iter().enumerate() {
                    for (dx, &v) in row.iter().enumerate() {
                        match cfa.cells[dy][dx] {
                            Ch::R => sr += v,
                            Ch::G => {
                                sg += v;
                                ng += 1.0;
                            }
                            Ch::B => sb += v,
                        }
                    }
                }
                rr[x] = sr;
                gg[x] = if ng > 0.0 { sg / ng } else { 0.0 };
                bb[x] = sb;
            }
        });
    Image {
        width: w,
        height: h,
        planes: 3,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rggb_superpixel() {
        // 2×2 RGGB block: R=1, G=0.5/0.3, B=0.2
        let img = Image {
            width: 2,
            height: 2,
            planes: 1,
            data: vec![1.0, 0.5, 0.3, 0.2],
        };
        let out = superpixel(&img, Cfa::new("RGGB", 0, 0, false, 2).unwrap());
        assert_eq!(out.data, [1.0, 0.4, 0.2]);
    }

    #[test]
    fn bilinear_flat_and_exact() {
        // A flat grey CFA frame debayers to flat grey; sensor pixels keep their value.
        let flat = Image {
            width: 6,
            height: 4,
            planes: 1,
            data: vec![0.5; 24],
        };
        let out = bilinear(&flat, Cfa::new("RGGB", 0, 0, false, 4).unwrap());
        assert!(out.data.iter().all(|v| (v - 0.5).abs() < 1e-6));
        let ramp = Image {
            width: 4,
            height: 4,
            planes: 1,
            data: (0..16).map(|i| i as f32).collect(),
        };
        let o = bilinear(&ramp, Cfa::new("RGGB", 0, 0, false, 4).unwrap());
        assert_eq!(o.data[0], 0.0, "R at (0,0) keeps its value");
        assert_eq!(o.data[16 + 1], 1.0, "G at (1,0) keeps its value");
        assert_eq!(o.data[32 + 5], 5.0, "B at (1,1) keeps its value");
    }

    #[test]
    fn offsets_and_row_order() {
        let img = Image {
            width: 2,
            height: 2,
            planes: 1,
            data: vec![1.0, 0.5, 0.3, 0.2],
        };
        // GRBG shifted one column is RGGB.
        let a = superpixel(&img, Cfa::new("GRBG", 1, 0, false, 2).unwrap());
        assert_eq!(a.data, [1.0, 0.4, 0.2]);
        // Stored bottom-up: stored row 0 is the image's bottom row (GB of RGGB).
        let b = superpixel(&img, Cfa::new("RGGB", 0, 0, true, 2).unwrap());
        assert_eq!(b.data, [0.3, 0.6, 0.5]);
        assert!(Cfa::new("XYZW", 0, 0, false, 2).is_none());
    }
}
