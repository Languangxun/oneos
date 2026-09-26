use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::time::{Duration, Instant};

const SIGNATURE: &str = include_str!("signature.data");

const FPS: u64 = 30;
const START_DELAY: f32 = 0.15;
const LETTER_SECS: f32 = 0.5;
const LETTER_DELAY: f32 = 0.06;
const HOLD_SECS: f32 = 1.0;

const BOOT_HISTORY: &str = "/var/lib/oneos/boot-history";
const RECORD_STAMP: &str = "/run/oneos-boot-recorded";
const HISTORY_LIMIT: usize = 8;
const HISTORY_WINDOW: usize = 5;
const FIRST_BOOT_SECS: f32 = 2.0;
const MIN_SECS: f32 = 0.9;
const SPEED_MARGIN: f32 = 0.8;

const STROKE_WIDTH_RATIO: f32 = 0.13;
const SOFTNESS: f32 = 0.5;
const SUPERSAMPLE: usize = 4;

const FG: u8 = 245;
const BAR_TRACK: u8 = 30;
const BAR_FILL: u8 = 220;

struct Glyph {
    contours: Vec<Vec<(f32, f32)>>,
    length: f32,
}

struct Signature {
    glyphs: Vec<Glyph>,
    em: f32,
}

struct Canvas {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0; width * height],
        }
    }
}

fn parse_signature(text: &str) -> Signature {
    let mut glyphs = Vec::new();
    let mut em = 1.0f32;

    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("# em") {
            em = value.trim().parse().unwrap_or(1.0);
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("G") => glyphs.push(Glyph {
                contours: Vec::new(),
                length: 0.0,
            }),
            Some("C") => {
                let numbers: Vec<f32> = tokens.filter_map(|token| token.parse().ok()).collect();
                let points = numbers
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| (pair[0], pair[1]))
                    .collect();
                if let Some(glyph) = glyphs.last_mut() {
                    glyph.contours.push(points);
                }
            }
            _ => {}
        }
    }

    Signature { glyphs, em }
}

fn layout(signature: &mut Signature, width: usize, height: usize) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for glyph in &signature.glyphs {
        for contour in &glyph.contours {
            for &(x, y) in contour {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return;
    }

    let scale = (width as f32 * 0.56 / (max_x - min_x)).min(height as f32 * 0.28 / (max_y - min_y));
    let origin_x = width as f32 / 2.0 - (min_x + max_x) / 2.0 * scale;
    let origin_y = height as f32 * 0.42 - (min_y + max_y) / 2.0 * scale;

    for glyph in &mut signature.glyphs {
        for contour in &mut glyph.contours {
            for point in contour.iter_mut() {
                point.0 = point.0 * scale + origin_x;
                point.1 = point.1 * scale + origin_y;
            }
        }
        let mut length = 0.0;
        for contour in &glyph.contours {
            for (start, end) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                length += ((end.0 - start.0).powi(2) + (end.1 - start.1).powi(2)).sqrt();
            }
        }
        glyph.length = length;
    }

    signature.em *= scale;
}

fn rasterize_fill(glyphs: &[Glyph], width: usize, height: usize, samples: usize) -> Vec<u8> {
    let sample_width = width * samples;
    let sample_height = height * samples;

    let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
    let factor = samples as f32;
    for glyph in glyphs {
        for contour in &glyph.contours {
            for (start, end) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                if start.1 == end.1 {
                    continue;
                }
                let winding = if end.1 > start.1 { 1 } else { -1 };
                edges.push((
                    start.0 * factor,
                    start.1 * factor,
                    end.0 * factor,
                    end.1 * factor,
                    winding,
                ));
            }
        }
    }

    let mut counts = vec![0u8; sample_width * sample_height];
    let mut crossings: Vec<(f32, i32)> = Vec::with_capacity(edges.len());

    for (row_index, row) in counts.chunks_mut(sample_width).enumerate() {
        let y = row_index as f32 + 0.5;
        crossings.clear();
        for &(x0, y0, x1, y1, winding) in &edges {
            if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                let t = (y - y0) / (y1 - y0);
                crossings.push((x0 + (x1 - x0) * t, winding));
            }
        }
        crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let mut winding = 0;
        let mut previous_x = 0.0f32;
        for &(x, direction) in &crossings {
            if winding != 0 {
                let start = (previous_x.max(0.0).floor() as usize).min(sample_width);
                let end = (x.max(0.0).ceil() as usize).min(sample_width);
                for value in &mut row[start..end] {
                    *value = value.saturating_add(1);
                }
            }
            winding += direction;
            previous_x = x;
        }
    }

    let total = (samples * samples) as u32;
    let mut fill = vec![0u8; width * height];
    for (y, row) in fill.chunks_mut(width).enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let mut sum = 0u32;
            for dy in 0..samples {
                let base = (y * samples + dy) * sample_width + x * samples;
                for dx in 0..samples {
                    sum += counts[base + dx] as u32;
                }
            }
            *value = (sum.min(total) * 255 / total) as u8;
        }
    }
    fill
}

fn cubic_bezier(t: f32) -> f32 {
    fn sample(p0: f32, p1: f32, p2: f32, p3: f32, s: f32) -> f32 {
        let mt = 1.0 - s;
        mt * mt * mt * p0 + 3.0 * mt * mt * s * p1 + 3.0 * mt * s * s * p2 + s * s * s * p3
    }

    let x = |s: f32| sample(0.0, 0.4, 0.2, 1.0, s);
    let y = |s: f32| sample(0.0, 0.0, 1.0, 1.0, s);

    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for _ in 0..20 {
        let mid = (low + high) / 2.0;
        if x(mid) < t {
            low = mid;
        } else {
            high = mid;
        }
    }
    y((low + high) / 2.0)
}

fn glyph_progress(elapsed: f32, index: usize) -> f32 {
    let start = START_DELAY + index as f32 * (LETTER_SECS + LETTER_DELAY);
    ((elapsed - start) / LETTER_SECS).clamp(0.0, 1.0)
}

fn total_seconds(glyph_count: usize) -> f32 {
    if glyph_count == 0 {
        return START_DELAY;
    }
    START_DELAY + glyph_count as f32 * (LETTER_SECS + LETTER_DELAY) - LETTER_DELAY
}

fn uptime_secs() -> Option<f32> {
    let text = fs::read_to_string("/proc/uptime").ok()?;
    text.split_whitespace().next()?.parse().ok()
}

fn parse_history(text: &str) -> Vec<f32> {
    text.lines()
        .filter_map(|line| line.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.1 && *value < 86_400.0)
        .collect()
}

fn read_history(path: &str) -> Vec<f32> {
    fs::read_to_string(path)
        .map(|text| parse_history(&text))
        .unwrap_or_default()
}

fn push_history(history: &mut Vec<f32>, value: f32) {
    history.push(value);
    let overflow = history.len().saturating_sub(HISTORY_LIMIT);
    history.drain(..overflow);
}

fn target_duration(history: &[f32], now: f32, natural: f32) -> f32 {
    let recent = &history[history.len().saturating_sub(HISTORY_WINDOW)..];
    if recent.is_empty() {
        return natural.min(FIRST_BOOT_SECS);
    }
    let mean = recent.iter().sum::<f32>() / recent.len() as f32;
    ((mean - now) * SPEED_MARGIN).clamp(MIN_SECS, natural)
}

fn boot_history_path() -> String {
    std::env::var("ONEO_SPLASH_HISTORY").unwrap_or_else(|_| BOOT_HISTORY.to_string())
}

fn record_stamp_path() -> String {
    std::env::var("ONEO_SPLASH_STAMP").unwrap_or_else(|_| RECORD_STAMP.to_string())
}

fn record_boot() -> io::Result<()> {
    let history_path = boot_history_path();
    let stamp_path = record_stamp_path();
    if Path::new(&stamp_path).exists() {
        return Ok(());
    }

    let now = uptime_secs().ok_or_else(|| io::Error::other("cannot read /proc/uptime"))?;
    let mut history = read_history(&history_path);
    push_history(&mut history, now);

    if let Some(parent) = Path::new(&history_path).parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for value in &history {
        text.push_str(&format!("{value:.3}\n"));
    }
    fs::write(&history_path, text)?;
    if let Some(parent) = Path::new(&stamp_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&stamp_path, format!("{now:.3}\n"))?;
    eprintln!("oneos-splash: recorded boot time {now:.2}s");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw_segment(
    mask: &mut [u8],
    width: usize,
    _height: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    softness: f32,
) {
    let pad = radius + softness + 1.0;
    let min_x = (x0.min(x1) - pad).floor().max(0.0) as isize;
    let max_x = (x0.max(x1) + pad).ceil() as isize;
    let min_y = (y0.min(y1) - pad).floor().max(0.0) as isize;
    let max_y = (y0.max(y1) + pad).ceil() as isize;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;

    for y in min_y..=max_y {
        let row = y as usize * width;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let t = if len_sq > 0.0 {
                (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let nx = x0 + t * dx;
            let ny = y0 + t * dy;
            let distance = ((px - nx).powi(2) + (py - ny).powi(2)).sqrt();
            let alpha = ((radius + softness - distance) / softness).clamp(0.0, 1.0);
            if alpha > 0.0 {
                let value = (255.0 * alpha) as u8;
                let index = row + x as usize;
                if value > mask[index] {
                    mask[index] = value;
                }
            }
        }
    }
}

fn draw_glyph_ink(
    mask: &mut [u8],
    width: usize,
    height: usize,
    glyph: &Glyph,
    target: f32,
    radius: f32,
    softness: f32,
) {
    if target <= 0.0 {
        return;
    }
    let mut walked = 0.0;
    for contour in &glyph.contours {
        for (start, end) in contour
            .iter()
            .zip(contour.iter().cycle().skip(1))
            .take(contour.len())
        {
            if walked >= target {
                return;
            }
            let segment = ((end.0 - start.0).powi(2) + (end.1 - start.1).powi(2)).sqrt();
            let remaining = target - walked;
            if remaining >= segment {
                draw_segment(
                    mask, width, height, start.0, start.1, end.0, end.1, radius, softness,
                );
            } else {
                let t = if segment > 0.0 {
                    remaining / segment
                } else {
                    0.0
                };
                draw_segment(
                    mask,
                    width,
                    height,
                    start.0,
                    start.1,
                    start.0 + (end.0 - start.0) * t,
                    start.1 + (end.1 - start.1) * t,
                    radius,
                    softness,
                );
                return;
            }
            walked += segment;
        }
    }
}

fn render(canvas: &mut Canvas, signature: &Signature, fill: &[u8], ink: &mut [u8], elapsed: f32) {
    ink.fill(0);

    let radius = signature.em * STROKE_WIDTH_RATIO / 2.0;
    let softness = (radius * 2.0 * SOFTNESS).max(1.0);
    for (index, glyph) in signature.glyphs.iter().enumerate() {
        let progress = cubic_bezier(glyph_progress(elapsed, index));
        draw_glyph_ink(
            ink,
            canvas.width,
            canvas.height,
            glyph,
            glyph.length * progress,
            radius,
            softness,
        );
    }

    for (out, (fill_value, ink_value)) in canvas.data.iter_mut().zip(fill.iter().zip(ink.iter())) {
        let coverage = *fill_value as u32 * *ink_value as u32 / 255;
        *out = (coverage * FG as u32 / 255) as u8;
    }

    let bar_width = canvas.width as f32 * 0.28;
    let bar_height = (canvas.height as f32 / 240.0).max(3.0);
    let bar_y = canvas.height as f32 * 0.66;
    let bar_x = canvas.width as f32 / 2.0;
    let bar_radius = bar_height / 2.0;

    draw_bar(
        &mut canvas.data,
        canvas.width,
        bar_x - bar_width / 2.0,
        bar_x + bar_width / 2.0,
        bar_y,
        bar_radius,
        BAR_TRACK,
    );

    let fill_ratio = (elapsed / total_seconds(signature.glyphs.len())).clamp(0.0, 1.0);
    if fill_ratio > 0.0 {
        draw_bar(
            &mut canvas.data,
            canvas.width,
            bar_x - bar_width / 2.0,
            bar_x - bar_width / 2.0 + bar_width * fill_ratio,
            bar_y,
            bar_radius,
            BAR_FILL,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bar(mask: &mut [u8], width: usize, x0: f32, x1: f32, y: f32, radius: f32, value: u8) {
    let min_x = (x0 - radius - 1.0).floor().max(0.0) as isize;
    let max_x = (x1 + radius + 1.0).ceil() as isize;
    let min_y = (y - radius - 1.0).floor().max(0.0) as isize;
    let max_y = (y + radius + 1.0).ceil() as isize;

    for py in min_y..=max_y {
        let row = py as usize * width;
        for px in min_x..=max_x {
            let fx = (px as f32 + 0.5).clamp(x0, x1);
            let fy = y;
            let distance = ((px as f32 + 0.5 - fx).powi(2) + (py as f32 + 0.5 - fy).powi(2)).sqrt();
            let alpha = (radius + 1.0 - distance).clamp(0.0, 1.0);
            if alpha > 0.0 {
                let scaled = (value as f32 * alpha) as u8;
                let index = row + px as usize;
                if scaled > mask[index] {
                    mask[index] = scaled;
                }
            }
        }
    }
}

struct Framebuffer {
    file: File,
    width: usize,
    height: usize,
    stride: usize,
    bytes_per_pixel: usize,
    buffer: Vec<u8>,
}

impl Framebuffer {
    fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).open(path)?;
        let virtual_size = fs::read_to_string("/sys/class/graphics/fb0/virtual_size")?;
        let (width, height) = virtual_size
            .trim()
            .split_once(',')
            .ok_or_else(|| io::Error::other("bad virtual_size"))?;
        let width: usize = width
            .trim()
            .parse()
            .map_err(|_| io::Error::other("bad width"))?;
        let height: usize = height
            .trim()
            .parse()
            .map_err(|_| io::Error::other("bad height"))?;
        let bytes_per_pixel =
            sysfs_usize("/sys/class/graphics/fb0/bits_per_pixel").unwrap_or(32) / 8;
        let stride =
            sysfs_usize("/sys/class/graphics/fb0/stride").unwrap_or(width * bytes_per_pixel);

        Ok(Self {
            file,
            width,
            height,
            stride,
            bytes_per_pixel,
            buffer: vec![0u8; stride * height],
        })
    }

    fn present(&mut self, canvas: &Canvas) -> io::Result<()> {
        for y in 0..self.height {
            let row = y * self.stride;
            let canvas_row = y * canvas.width;
            for x in 0..self.width {
                let value = canvas.data[canvas_row + x];
                let offset = row + x * self.bytes_per_pixel;
                match self.bytes_per_pixel {
                    4 => {
                        self.buffer[offset] = value;
                        self.buffer[offset + 1] = value;
                        self.buffer[offset + 2] = value;
                        self.buffer[offset + 3] = 0;
                    }
                    2 => {
                        let value16 = ((value as u16 >> 3) << 11)
                            | ((value as u16 >> 2) << 5)
                            | (value as u16 >> 3);
                        self.buffer[offset..offset + 2].copy_from_slice(&value16.to_ne_bytes());
                    }
                    _ => {}
                }
            }
        }
        self.file.write_all_at(&self.buffer, 0)
    }
}

fn sysfs_usize(path: &str) -> Option<usize> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn preview(dir: &str) -> io::Result<()> {
    let (width, height) = (800usize, 450usize);
    fs::create_dir_all(dir)?;
    let mut signature = parse_signature(SIGNATURE);
    layout(&mut signature, width, height);
    let fill = rasterize_fill(&signature.glyphs, width, height, SUPERSAMPLE);
    let mut ink = vec![0u8; width * height];
    let mut canvas = Canvas::new(width, height);
    let duration = total_seconds(signature.glyphs.len()) + HOLD_SECS;

    for step in 0..=8 {
        let elapsed = duration * step as f32 / 8.0;
        render(&mut canvas, &signature, &fill, &mut ink, elapsed);
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for value in &canvas.data {
            ppm.extend_from_slice(&[*value, *value, *value]);
        }
        fs::write(format!("{dir}/frame-{step:02}.ppm"), ppm)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(index) = args.iter().position(|arg| arg == "--preview") {
        let dir = args
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or("/tmp/oneos-splash");
        return preview(dir);
    }
    if args.iter().any(|arg| arg == "--record") {
        return record_boot();
    }

    let mut framebuffer = Framebuffer::open("/dev/fb0")?;
    let mut signature = parse_signature(SIGNATURE);
    layout(&mut signature, framebuffer.width, framebuffer.height);
    let fill = rasterize_fill(
        &signature.glyphs,
        framebuffer.width,
        framebuffer.height,
        SUPERSAMPLE,
    );
    let mut ink = vec![0u8; framebuffer.width * framebuffer.height];
    let mut canvas = Canvas::new(framebuffer.width, framebuffer.height);

    let natural = total_seconds(signature.glyphs.len()) + HOLD_SECS;
    let now = uptime_secs().unwrap_or(0.0);
    let history = read_history(&boot_history_path());
    let duration = target_duration(&history, now, natural);
    let speed = natural / duration;
    eprintln!(
        "oneos-splash: {:.2}s natural -> {:.2}s playback (x{speed:.1}), {} boot sample(s)",
        natural,
        duration,
        history.len()
    );

    let frame_time = Duration::from_secs_f64(1.0 / FPS as f64);
    let start = Instant::now();

    loop {
        let wall = start.elapsed().as_secs_f32();
        let elapsed = (wall * speed).min(natural);
        render(&mut canvas, &signature, &fill, &mut ink, elapsed);
        framebuffer.present(&canvas)?;
        if wall >= duration {
            break;
        }
        std::thread::sleep(frame_time);
    }
    render(&mut canvas, &signature, &fill, &mut ink, natural);
    framebuffer.present(&canvas)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_parses_glyphs() {
        let signature = parse_signature(SIGNATURE);
        assert_eq!(signature.glyphs.len(), "OneOS".len());
        assert!(signature.em > 0.0);
        assert!(
            signature
                .glyphs
                .iter()
                .all(|glyph| !glyph.contours.is_empty())
        );
    }

    #[test]
    fn bezier_easing_spans_unit_interval() {
        assert!(cubic_bezier(0.0).abs() < 1e-3);
        assert!((cubic_bezier(1.0) - 1.0).abs() < 1e-3);
        assert!(cubic_bezier(0.5) > 0.3 && cubic_bezier(0.5) < 0.8);
    }

    #[test]
    fn fill_mask_covers_pixels() {
        let mut signature = parse_signature(SIGNATURE);
        layout(&mut signature, 800, 450);
        let fill = rasterize_fill(&signature.glyphs, 800, 450, SUPERSAMPLE);
        assert!(fill.iter().any(|value| *value > 200));
    }

    #[test]
    fn history_parsing_ignores_junk() {
        let history = parse_history("2.5\njunk\n-1\n0\n3.25\n999999\n");
        assert_eq!(history, vec![2.5, 3.25]);
    }

    #[test]
    fn history_keeps_only_recent_samples() {
        let mut history = Vec::new();
        for value in 1..=20 {
            push_history(&mut history, value as f32);
        }
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(history.first(), Some(&13.0));
        assert_eq!(history.last(), Some(&20.0));
    }

    #[test]
    fn first_boot_uses_fixed_default() {
        let natural = 3.9;
        assert!((target_duration(&[], 0.5, natural) - FIRST_BOOT_SECS).abs() < 1e-6);
    }

    #[test]
    fn target_duration_follows_boot_estimate() {
        let natural = 3.9;
        let history = [3.0, 3.2, 2.8];
        let now = 0.5;
        let expected = ((3.0 + 3.2 + 2.8) / 3.0 - now) * SPEED_MARGIN;
        assert!((target_duration(&history, now, natural) - expected).abs() < 1e-6);
    }

    #[test]
    fn target_duration_is_clamped() {
        assert_eq!(target_duration(&[0.4], 0.5, 3.9), MIN_SECS);
        assert_eq!(target_duration(&[30.0], 0.5, 3.9), 3.9);
    }

    #[test]
    fn ink_mask_and_render_draw_text() {
        let mut signature = parse_signature(SIGNATURE);
        layout(&mut signature, 800, 450);
        let fill = rasterize_fill(&signature.glyphs, 800, 450, SUPERSAMPLE);
        let mut ink = vec![0u8; 800 * 450];
        let mut canvas = Canvas::new(800, 450);
        let duration = total_seconds(signature.glyphs.len());
        render(&mut canvas, &signature, &fill, &mut ink, duration);
        let overlap = fill
            .iter()
            .zip(ink.iter())
            .filter(|(fill_value, ink_value)| **fill_value > 100 && **ink_value > 100)
            .count();
        assert!(overlap > 0, "fill and ink do not overlap");
        assert!(canvas.data.iter().filter(|value| **value > 150).count() > 100);
    }
}
