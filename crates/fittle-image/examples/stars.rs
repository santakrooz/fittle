//! `cargo run --release -p fittle-image --example stars -- <files…>`: star metrics and timing.
fn main() {
    for p in std::env::args().skip(1) {
        let t = std::time::Instant::now();
        match fittle_image::stars::measure_file(std::path::Path::new(&p)) {
            Ok((_, s)) => println!(
                "{:>5.0} ms  stars {:>4}  hfr {:>5.2}  fwhm {:>5.2}  ecc {:>4.2}  bg {:.4}  noise {:.5}  trail {:?}  {}",
                t.elapsed().as_secs_f64() * 1e3,
                s.stars,
                s.hfr.unwrap_or(0.0),
                s.fwhm.unwrap_or(0.0),
                s.eccentricity.unwrap_or(0.0),
                s.background,
                s.noise,
                s.trail.map(|t| t.length_px as i32),
                p.rsplit('/').next().unwrap()
            ),
            Err(e) => println!("error {e}: {p}"),
        }
    }
}
