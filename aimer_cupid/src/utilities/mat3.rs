/// A 3x3 column-major matrix for 2D transforms.
#[derive(Debug, Clone, Copy)]
pub struct Mat3 {
    pub cols: [[f32; 3]; 3],
}

impl Mat3 {
    pub const fn identity() -> Self {
        Self {
            cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub const fn translate(tx: f32, ty: f32) -> Self {
        Self {
            cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [tx, ty, 1.0]],
        }
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
        Self {
            cols: [[sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn rotate(radians: f32) -> Self {
        let c = radians.cos();
        let s = radians.sin();
        Self {
            cols: [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    /// Transform a 2D point by this matrix.
    pub const fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let c = &self.cols;
        (
            c[0][0] * x + c[1][0] * y + c[2][0],
            c[0][1] * x + c[1][1] * y + c[2][1],
        )
    }

    /// Inverts this affine transform at one point.
    ///
    /// Returns `None` when the linear portion is singular or non-finite. Text
    /// interaction uses this to map pointer coordinates back into the local
    /// paragraph space without assuming that the canvas only translated.
    pub fn inverse_transform_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let a = self.cols[0][0];
        let b = self.cols[1][0];
        let c = self.cols[0][1];
        let d = self.cols[1][1];
        let determinant = a * d - b * c;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }

        let translated_x = x - self.cols[2][0];
        let translated_y = y - self.cols[2][1];
        let inverse_x = (d * translated_x - b * translated_y) / determinant;
        let inverse_y = (-c * translated_x + a * translated_y) / determinant;
        (inverse_x.is_finite() && inverse_y.is_finite()).then_some((inverse_x, inverse_y))
    }

    /// Align the transform origin to physical pixel boundaries without
    /// changing its scale or rotation.
    pub fn pixel_aligned(mut self) -> Self {
        self.cols[2][0] = self.cols[2][0].round();
        self.cols[2][1] = self.cols[2][1].round();
        self
    }
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::Mat3;

    #[test]
    fn pixel_alignment_rounds_only_translation() {
        let transform = Mat3::translate(10.49, -4.51).mul(&Mat3::scale(1.5, 2.0));
        let moved_one_pixel = Mat3::translate(11.49, -3.51).mul(&Mat3::scale(1.5, 2.0));

        let aligned = transform.pixel_aligned();
        let aligned_move = moved_one_pixel.pixel_aligned();

        assert_eq!(aligned.cols[2], [10.0, -5.0, 1.0]);
        assert_eq!(aligned_move.cols[2], [11.0, -4.0, 1.0]);
        assert_eq!(aligned.cols[0], transform.cols[0]);
        assert_eq!(aligned.cols[1], transform.cols[1]);
    }

    #[test]
    fn pixel_alignment_keeps_integer_translation_stable() {
        let transform = Mat3::translate(12.0, -7.0);

        assert_eq!(transform.pixel_aligned().cols, transform.cols);
    }

    #[test]
    fn inverse_transform_point_round_trips_scaled_rotated_translation() {
        let transform = Mat3::translate(20.0, 30.0)
            .mul(&Mat3::rotate(0.37))
            .mul(&Mat3::scale(2.0, 3.0));
        let local = (4.0, -5.0);
        let world = transform.transform_point(local.0, local.1);

        let recovered = transform
            .inverse_transform_point(world.0, world.1)
            .expect("the affine transform is invertible");
        assert!((recovered.0 - local.0).abs() < 0.00001);
        assert!((recovered.1 - local.1).abs() < 0.00001);
    }

    #[test]
    fn inverse_transform_point_rejects_singular_transforms() {
        assert!(Mat3::scale(0.0, 2.0)
            .inverse_transform_point(1.0, 1.0)
            .is_none());
    }
}
