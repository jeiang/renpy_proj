//! Drawing primitives (the parts of SDL2_gfx that `renpy.pygame.draw` uses).

use crate::blit::alpha_blend;
use crate::buf::Img;
use crate::sdl::SdlRect;

pub struct Pen {
    pub img: Img,
    pub clip: SdlRect,
    pub color: [u8; 4],
}

impl Pen {
    /// Plots one pixel with the color's alpha scaled by `weight` (0..=255).
    pub fn plot(&self, x: i32, y: i32, weight: u32) {
        let c = self.clip;
        if x < c.x || y < c.y || x >= c.x + c.w || y >= c.y + c.h {
            return;
        }
        if x < 0 || y < 0 || x as usize >= self.img.w || y as usize >= self.img.h {
            return;
        }
        let a = (self.color[3] as u32 * weight / 255) as u8;
        let fmt = self.img.fmt;
        let (ux, uy) = (x as usize, y as usize);
        if a == 255 {
            self.img.put(ux, uy, fmt.pack(self.color));
        } else if a > 0 {
            let d = fmt.unpack(self.img.px(ux, uy));
            let s = [self.color[0], self.color[1], self.color[2], a];
            self.img.put(ux, uy, fmt.pack(alpha_blend(s, d)));
        }
    }

    pub fn hline(&self, x1: i32, x2: i32, y: i32) {
        let (a, b) = (x1.min(x2), x1.max(x2));
        for x in a..=b {
            self.plot(x, y, 255);
        }
    }

    pub fn vline(&self, x: i32, y1: i32, y2: i32) {
        let (a, b) = (y1.min(y2), y1.max(y2));
        for y in a..=b {
            self.plot(x, y, 255);
        }
    }

    pub fn rectangle(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (xa, xb) = (x1.min(x2), x1.max(x2));
        let (ya, yb) = (y1.min(y2), y1.max(y2));
        self.hline(xa, xb, ya);
        if yb != ya {
            self.hline(xa, xb, yb);
        }
        if yb - ya > 1 {
            self.vline(xa, ya + 1, yb - 1);
            if xb != xa {
                self.vline(xb, ya + 1, yb - 1);
            }
        }
    }

    pub fn box_(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (ya, yb) = (y1.min(y2), y1.max(y2));
        for y in ya..=yb {
            self.hline(x1, x2, y);
        }
    }

    /// Bresenham line, both ends included.
    pub fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (mut x, mut y) = (x1, y1);
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.plot(x, y, 255);
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Wu's antialiased line.
    pub fn aaline(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (mut x0, mut y0, mut x1, mut y1) = (x1 as f64, y1 as f64, x2 as f64, y2 as f64);
        let steep = (y1 - y0).abs() > (x1 - x0).abs();
        if steep {
            std::mem::swap(&mut x0, &mut y0);
            std::mem::swap(&mut x1, &mut y1);
        }
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let dx = x1 - x0;
        let gradient = if dx == 0.0 { 1.0 } else { (y1 - y0) / dx };
        let mut y = y0;
        let mut x = x0 as i32;
        let xe = x1 as i32;
        while x <= xe {
            let yi = y.floor();
            let frac = y - yi;
            let w_hi = (frac * 255.0) as u32;
            let w_lo = 255 - w_hi;
            let yi = yi as i32;
            if steep {
                self.plot(yi, x, w_lo);
                self.plot(yi + 1, x, w_hi);
            } else {
                self.plot(x, yi, w_lo);
                self.plot(x, yi + 1, w_hi);
            }
            y += gradient;
            x += 1;
        }
    }

    pub fn filled_polygon(&self, pts: &[(i32, i32)]) {
        if pts.len() < 3 {
            return;
        }
        let ymin = pts.iter().map(|p| p.1).min().unwrap();
        let ymax = pts.iter().map(|p| p.1).max().unwrap();
        let n = pts.len();
        for y in ymin..=ymax {
            let mut xs: Vec<i32> = Vec::new();
            for i in 0..n {
                let (x1, y1) = pts[i];
                let (x2, y2) = pts[(i + 1) % n];
                if y1 == y2 {
                    continue;
                }
                let (lo, hi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                if y >= lo && y < hi {
                    let t = (y - y1) as f64 / (y2 - y1) as f64;
                    xs.push((x1 as f64 + t * (x2 - x1) as f64).round() as i32);
                }
            }
            xs.sort_unstable();
            for pair in xs.chunks(2) {
                if let [a, b] = pair {
                    self.hline(*a, *b, y);
                }
            }
        }
        // Edges of horizontal runs and the bottom row.
        for i in 0..n {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % n];
            if y1 == y2 {
                self.hline(x1, x2, y1);
            }
        }
    }

    pub fn thick_line(&self, x1: i32, y1: i32, x2: i32, y2: i32, width: i32) {
        if width <= 1 {
            self.line(x1, y1, x2, y2);
            return;
        }
        let (dx, dy) = ((x2 - x1) as f64, (y2 - y1) as f64);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            return;
        }
        let half = width as f64 / 2.0;
        let (nx, ny) = (-dy / len * half, dx / len * half);
        let r = |v: f64| v.round() as i32;
        let quad = [
            (r(x1 as f64 + nx), r(y1 as f64 + ny)),
            (r(x2 as f64 + nx), r(y2 as f64 + ny)),
            (r(x2 as f64 - nx), r(y2 as f64 - ny)),
            (r(x1 as f64 - nx), r(y1 as f64 - ny)),
        ];
        self.filled_polygon(&quad);
    }

    pub fn filled_ellipse(&self, cx: i32, cy: i32, rx: i32, ry: i32) {
        if rx < 0 || ry < 0 {
            return;
        }
        if ry == 0 {
            self.hline(cx - rx, cx + rx, cy);
            return;
        }
        for dy in -ry..=ry {
            let t = 1.0 - (dy as f64 / ry as f64).powi(2);
            let dx = (rx as f64 * t.max(0.0).sqrt()).round() as i32;
            self.hline(cx - dx, cx + dx, cy + dy);
        }
    }

    pub fn ellipse(&self, cx: i32, cy: i32, rx: i32, ry: i32) {
        if rx < 0 || ry < 0 {
            return;
        }
        for dy in -ry..=ry {
            let t = if ry == 0 { 0.0 } else { 1.0 - (dy as f64 / ry as f64).powi(2) };
            let dx = (rx as f64 * t.max(0.0).sqrt()).round() as i32;
            self.plot(cx - dx, cy + dy, 255);
            self.plot(cx + dx, cy + dy, 255);
        }
        for dx in -rx..=rx {
            let t = if rx == 0 { 0.0 } else { 1.0 - (dx as f64 / rx as f64).powi(2) };
            let dy = (ry as f64 * t.max(0.0).sqrt()).round() as i32;
            self.plot(cx + dx, cy - dy, 255);
            self.plot(cx + dx, cy + dy, 255);
        }
    }

    pub fn circle(&self, cx: i32, cy: i32, r: i32) {
        self.ellipse(cx, cy, r, r)
    }

    pub fn filled_circle(&self, cx: i32, cy: i32, r: i32) {
        self.filled_ellipse(cx, cy, r, r)
    }
}
