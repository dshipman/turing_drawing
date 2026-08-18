#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DirtyRect {
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    pub fn from_cell(x: i32, y: i32) -> Self {
        Self {
            x: x as u32,
            y: y as u32,
            width: 1,
            height: 1,
        }
    }

    /// Clamp to canvas bounds, returning a zero-area rect if fully outside.
    pub fn clamp(self, canvas_w: u32, canvas_h: u32) -> Self {
        let x = self.x.min(canvas_w);
        let y = self.y.min(canvas_h);
        let w = (self.x + self.width).min(canvas_w).saturating_sub(x);
        let h = (self.y + self.height).min(canvas_h).saturating_sub(y);
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn union(self, other: Self) -> Self {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.width).max(other.x + other.width);
        let y1 = (self.y + self.height).max(other.y + other.height);
        Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    }
}
