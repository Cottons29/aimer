/// A device-pixel rectangle using half-open `[x, x + width)` bounds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DamageRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl DamageRect {
    /// Creates a device-pixel rectangle.
    #[inline]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the left edge.
    #[inline]
    pub const fn x(self) -> u32 {
        self.x
    }

    /// Returns the top edge.
    #[inline]
    pub const fn y(self) -> u32 {
        self.y
    }

    /// Returns the width.
    #[inline]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the height.
    #[inline]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns whether the rectangle covers no pixels.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the number of pixels covered by the rectangle.
    #[inline]
    pub fn area(self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height))
    }

    fn max_x(self) -> u64 {
        u64::from(self.x).saturating_add(u64::from(self.width))
    }

    fn max_y(self) -> u64 {
        u64::from(self.y).saturating_add(u64::from(self.height))
    }

    fn touches_or_overlaps(self, other: Self) -> bool {
        u64::from(self.x) <= other.max_x()
            && u64::from(other.x) <= self.max_x()
            && u64::from(self.y) <= other.max_y()
            && u64::from(other.y) <= self.max_y()
    }

    fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.max_x().max(other.max_x());
        let bottom = self.max_y().max(other.max_y());
        Self::new(
            left,
            top,
            right.saturating_sub(u64::from(left)) as u32,
            bottom.saturating_sub(u64::from(top)) as u32,
        )
    }

    fn clipped_to(self, width: u32, height: u32) -> Option<Self> {
        let left = self.x.min(width);
        let top = self.y.min(height);
        let right = self.max_x().min(u64::from(width)) as u32;
        let bottom = self.max_y().min(u64::from(height)) as u32;
        (right > left && bottom > top)
            .then_some(Self::new(left, top, right - left, bottom - top))
    }
}

/// Policy controlling when a damage set is promoted to a full repaint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamagePolicy {
    /// Promote when the coalesced pixel area reaches this surface fraction.
    pub full_frame_ratio: f32,
    /// Promote when the normalized set would contain more regions than this.
    pub max_regions: usize,
}

impl Default for DamagePolicy {
    fn default() -> Self {
        Self {
            full_frame_ratio: 0.5,
            max_regions: 32,
        }
    }
}

/// Result of adding one damage request to a [`DamageSet`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DamageAddResult {
    /// The request covered no pixels after normalization/clipping.
    Ignored,
    /// The request added a new disjoint region.
    Added,
    /// The request was coalesced with one or more existing regions.
    Merged,
    /// The set was promoted to a full-surface repaint.
    PromotedToFull,
}

/// Normalized, coalesced damage for one device-pixel surface.
///
/// The set is deliberately independent of rendering backends. It only answers
/// which pixels may be stale; a renderer still owns the persistent target,
/// layer ordering, and the decision to use a partial or full pass.
#[derive(Debug)]
pub struct DamageSet {
    width: u32,
    height: u32,
    policy: DamagePolicy,
    full_frame: bool,
    regions: Vec<DamageRect>,
    merged_regions: u64,
}

impl DamageSet {
    /// Creates an empty damage set with the default promotion policy.
    #[inline]
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_policy(width, height, DamagePolicy::default())
    }

    /// Creates an empty damage set with an explicit promotion policy.
    #[inline]
    pub fn with_policy(width: u32, height: u32, policy: DamagePolicy) -> Self {
        Self {
            width,
            height,
            policy: DamagePolicy {
                full_frame_ratio: if policy.full_frame_ratio.is_finite() {
                    policy.full_frame_ratio.clamp(0.0, 1.0)
                } else {
                    DamagePolicy::default().full_frame_ratio
                },
                max_regions: policy.max_regions.max(1),
            },
            full_frame: false,
            regions: Vec::new(),
            merged_regions: 0,
        }
    }

    /// Returns the surface width in device pixels.
    #[inline]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the surface height in device pixels.
    #[inline]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns whether the set requests a full-surface repaint.
    #[inline]
    pub const fn is_full_frame(&self) -> bool {
        self.full_frame
    }

    /// Returns the normalized, coalesced regions.
    #[inline]
    pub fn regions(&self) -> &[DamageRect] {
        &self.regions
    }

    /// Returns the total coalesced pixel area.
    #[inline]
    pub fn area(&self) -> u64 {
        if self.full_frame {
            u64::from(self.width).saturating_mul(u64::from(self.height))
        } else {
            self.regions.iter().map(|region| region.area()).sum()
        }
    }

    /// Returns how many pairwise region merges have occurred.
    #[inline]
    pub const fn merged_regions(&self) -> u64 {
        self.merged_regions
    }

    /// Adds a device-pixel rectangle after clipping it to the surface.
    #[inline]
    pub fn add_rect(&mut self, rect: DamageRect) -> DamageAddResult {
        let Some(rect) = rect.clipped_to(self.width, self.height) else {
            return DamageAddResult::Ignored;
        };
        self.add_normalized(rect)
    }

    /// Adds floating-point bounds using conservative floor/ceil pixel coverage.
    ///
    /// Empty bounds are ignored. Non-finite or negatively sized bounds promote
    /// to a full repaint because the caller cannot establish a safe region.
    #[inline]
    pub fn add_f32_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> DamageAddResult {
        if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
            self.promote_full_frame();
            return DamageAddResult::PromotedToFull;
        }
        if width < 0.0 || height < 0.0 {
            self.promote_full_frame();
            return DamageAddResult::PromotedToFull;
        }
        if width == 0.0 || height == 0.0 {
            return DamageAddResult::Ignored;
        }

        let right = f64::from(x) + f64::from(width);
        let bottom = f64::from(y) + f64::from(height);
        if !right.is_finite() || !bottom.is_finite() {
            self.promote_full_frame();
            return DamageAddResult::PromotedToFull;
        }

        let left = f64::from(x.floor()).clamp(0.0, f64::from(self.width)) as u32;
        let top = f64::from(y.floor()).clamp(0.0, f64::from(self.height)) as u32;
        let right = right.ceil().clamp(0.0, f64::from(self.width)) as u32;
        let bottom = bottom.ceil().clamp(0.0, f64::from(self.height)) as u32;
        if right <= left || bottom <= top {
            return DamageAddResult::Ignored;
        }
        self.add_normalized(DamageRect::new(left, top, right - left, bottom - top))
    }

    /// Promotes the set to a full-surface repaint.
    #[inline]
    pub fn promote_full_frame(&mut self) {
        self.full_frame = true;
        self.regions.clear();
    }

    /// Clears all regions while retaining the allocation for reuse.
    #[inline]
    pub fn reset(&mut self) {
        self.full_frame = false;
        self.regions.clear();
        self.merged_regions = 0;
    }

    fn add_normalized(&mut self, rect: DamageRect) -> DamageAddResult {
        if self.full_frame {
            return DamageAddResult::PromotedToFull;
        }

        let mut merged = false;
        let mut combined = rect;
        loop {
            let Some(index) = self
                .regions
                .iter()
                .position(|existing| existing.touches_or_overlaps(combined))
            else {
                break;
            };
            combined = self.regions.remove(index).union(combined);
            merged = true;
            self.merged_regions = self.merged_regions.saturating_add(1);
        }
        self.regions.push(combined);

        let surface_area = u64::from(self.width).saturating_mul(u64::from(self.height));
        let coverage = self.area() as f64;
        let threshold = surface_area as f64 * f64::from(self.policy.full_frame_ratio);
        if self.regions.len() > self.policy.max_regions || coverage >= threshold {
            self.promote_full_frame();
            DamageAddResult::PromotedToFull
        } else if merged {
            DamageAddResult::Merged
        } else {
            DamageAddResult::Added
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_bounds_are_clipped_to_half_open_device_pixels() {
        let mut damage = DamageSet::new(100, 80);

        assert_eq!(
            damage.add_f32_rect(-2.25, 4.2, 5.1, 6.1),
            DamageAddResult::Added
        );
        assert_eq!(damage.regions(), &[DamageRect::new(0, 4, 3, 7)]);
    }

    #[test]
    fn touching_regions_are_coalesced_and_area_is_not_double_counted() {
        let mut damage = DamageSet::new(100, 80);

        assert_eq!(
            damage.add_rect(DamageRect::new(2, 3, 5, 4)),
            DamageAddResult::Added
        );
        assert_eq!(
            damage.add_rect(DamageRect::new(7, 3, 5, 4)),
            DamageAddResult::Merged
        );

        assert_eq!(damage.regions(), &[DamageRect::new(2, 3, 10, 4)]);
        assert_eq!(damage.area(), 40);
        assert_eq!(damage.merged_regions(), 1);
    }

    #[test]
    fn coalescing_rechecks_regions_after_the_union_grows() {
        let mut damage = DamageSet::new(100, 80);
        damage.add_rect(DamageRect::new(0, 0, 2, 2));
        damage.add_rect(DamageRect::new(4, 0, 2, 2));

        assert_eq!(
            damage.add_rect(DamageRect::new(2, 0, 2, 2)),
            DamageAddResult::Merged
        );
        assert_eq!(damage.regions(), &[DamageRect::new(0, 0, 6, 2)]);
    }

    #[test]
    fn coverage_threshold_promotes_to_full_frame() {
        let mut damage = DamageSet::with_policy(
            100,
            80,
            DamagePolicy {
                full_frame_ratio: 0.5,
                max_regions: 16,
            },
        );

        assert_eq!(
            damage.add_rect(DamageRect::new(0, 0, 50, 80)),
            DamageAddResult::PromotedToFull
        );
        assert!(damage.is_full_frame());
        assert_eq!(damage.regions(), &[]);
        assert_eq!(damage.area(), 8_000);
    }

    #[test]
    fn invalid_float_bounds_promote_conservatively() {
        let mut damage = DamageSet::new(100, 80);

        assert_eq!(
            damage.add_f32_rect(f32::NAN, 0.0, 10.0, 10.0),
            DamageAddResult::PromotedToFull
        );
        assert!(damage.is_full_frame());
    }

    #[test]
    fn too_many_disjoint_regions_promote_to_full_frame() {
        let mut damage = DamageSet::with_policy(
            100,
            80,
            DamagePolicy {
                full_frame_ratio: 1.0,
                max_regions: 2,
            },
        );

        assert_eq!(
            damage.add_rect(DamageRect::new(0, 0, 1, 1)),
            DamageAddResult::Added
        );
        assert_eq!(
            damage.add_rect(DamageRect::new(10, 10, 1, 1)),
            DamageAddResult::Added
        );
        assert_eq!(
            damage.add_rect(DamageRect::new(20, 20, 1, 1)),
            DamageAddResult::PromotedToFull
        );
        assert!(damage.is_full_frame());
    }
}
