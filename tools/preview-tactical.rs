// Render illustrative LCD frames using the production renderer, without hardware.
#[path = "../daemon/src/display/bitmap_font.rs"]
mod bitmap_font;
#[path = "../daemon/src/display/tactical.rs"]
mod tactical;
use std::{io::Write, net::Ipv4Addr, time::Instant};
fn main() {
    let output = std::env::args().nth(1).expect("output directory required");
    let now = Instant::now();
    let mut stats = tactical::TacticalStats::default();
    stats.start(now);
    stats.receive(now);
    let telemetry = tactical::Telemetry {
        addresses: vec![
            ("USB".into(), Ipv4Addr::new(192, 168, 1, 1)),
            ("WIFI".into(), Ipv4Addr::new(192, 168, 0, 50)),
        ],
        battery: Some((75, true)),
        ..Default::default()
    };
    for state in ["recording", "alert", "paused", "failure"] {
        match state {
            "alert" => {
                stats.detect(0);
                stats.detect(0);
                stats.detect(1);
                stats.detect(2);
            }
            "paused" => stats.stop(now),
            "failure" => stats.capture_failed = true,
            _ => {}
        }
        let pixels = tactical::render(&stats, &telemetry, 8080, now, 0);
        let mut file = std::fs::File::create(format!("{output}/{state}.ppm")).unwrap();
        write!(file, "P6\n128 128\n255\n").unwrap();
        for (r, g, b) in pixels {
            file.write_all(&[r, g, b]).unwrap();
        }
    }
}
