/// A 3x3 column-major matrix for 2D transforms.
#[derive(Debug, Clone, Copy)]
pub struct Mat3 {
    pub cols: [[f32; 3]; 3],
}

impl Mat3 {
    pub const fn identity() -> Self {
        Self { cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] }
    }

    pub const fn translate(tx: f32, ty: f32) -> Self {
        Self { cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [tx, ty, 1.0]] }
    }

    /// Multiply self * other (apply self after other).
    pub fn mul(&self, other: &Mat3) -> Mat3 {
        let a = &self.cols;
        let b = &other.cols;
        let mut out = [[0.0f32; 3]; 3];
        for c in 0..3 {
            for r in 0..3 {
                out[c][r] = a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2];
            }
        }
        Mat3 { cols: out }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self { cols: [[sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, 1.0]] }
    }

    pub fn rotate(radians: f32) -> Self {
        let c = radians.cos();
        let s = radians.sin();
        Self { cols: [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]] }
    }

    /// Transform a 2D point by this matrix.
    pub const fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let c = &self.cols;
        (c[0][0] * x + c[1][0] * y + c[2][0], c[0][1] * x + c[1][1] * y + c[2][1])
    }
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}
