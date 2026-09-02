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

/// A logical rectangle that contributes to a layer's screen-space footprint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl DamageBounds {
    /// Creates logical bounds for a retained layer or effect envelope.
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the left edge.
    #[inline]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the top edge.
    #[inline]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the logical width.
    #[inline]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the logical height.
    #[inline]
    pub const fn height(self) -> f32 {
        self.height
    }

    fn transformed(self, transform: DamageTransform) -> Result<Self, ()> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width < 0.0
            || self.height < 0.0
            || !transform.is_finite()
        {
            return Err(());
        }

        let right = self.x + self.width;
        let bottom = self.y + self.height;
        if !right.is_finite() || !bottom.is_finite() {
            return Err(());
        }

        let corners = [
            (self.x, self.y),
            (right, self.y),
            (self.x, bottom),
            (right, bottom),
        ];
        let (first_x, first_y) = transform.apply(corners[0]);
        if !first_x.is_finite() || !first_y.is_finite() {
            return Err(());
        }
        let mut min_x = first_x;
        let mut max_x = first_x;
        let mut min_y = first_y;
        let mut max_y = first_y;
        for corner in corners.into_iter().skip(1) {
            let (x, y) = transform.apply(corner);
            if !x.is_finite() || !y.is_finite() {
                return Err(());
            }
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }

        Ok(Self::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    fn expanded(self, amount: f32) -> Result<Self, ()> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(());
        }
        let x = self.x - amount;
        let y = self.y - amount;
        let width = self.width + amount * 2.0;
        let height = self.height + amount * 2.0;
        (x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite())
            .then_some(Self::new(x, y, width, height))
            .ok_or(())
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > left && bottom > top).then_some(Self::new(
            left,
            top,
            right - left,
            bottom - top,
        ))
    }
}

/// A finite affine transform used to map layer-local bounds into the canvas.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl DamageTransform {
    /// Returns the identity transform.
    #[inline]
    pub const fn identity() -> Self {
        Self::affine(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }

    /// Creates an affine transform in the form `x' = ax + cy + tx` and
    /// `y' = bx + dy + ty`.
    #[inline]
    pub const fn affine(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> Self {
        Self {
            a,
            b,
            c,
            d,
            tx,
            ty,
        }
    }

    /// Creates a translation transform.
    #[inline]
    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self::affine(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// Converts the canvas's column-major matrix into a damage transform.
    #[inline]
    pub fn from_matrix(matrix: crate::utilities::Mat3) -> Self {
        Self::affine(
            matrix.cols[0][0],
            matrix.cols[0][1],
            matrix.cols[1][0],
            matrix.cols[1][1],
            matrix.cols[2][0],
            matrix.cols[2][1],
        )
    }

    fn is_finite(self) -> bool {
        self.a.is_finite()
            && self.b.is_finite()
            && self.c.is_finite()
            && self.d.is_finite()
            && self.tx.is_finite()
            && self.ty.is_finite()
    }

    fn apply(self, point: (f32, f32)) -> (f32, f32) {
        let (x, y) = point;
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }
}

/// Geometry and conservative effect envelope for one retained layer.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageGeometry {
    bounds: DamageBounds,
    clip: Option<DamageBounds>,
    transform: DamageTransform,
    effect_expansion: f32,
}

impl DamageGeometry {
    /// Creates an unclipped, untransformed layer footprint.
    #[inline]
    pub const fn new(bounds: DamageBounds) -> Self {
        Self {
            bounds,
            clip: None,
            transform: DamageTransform::identity(),
            effect_expansion: 0.0,
        }
    }

    /// Replaces the optional local clip rectangle.
    #[inline]
    pub const fn with_clip(mut self, clip: Option<DamageBounds>) -> Self {
        self.clip = clip;
        self
    }

    /// Replaces the local-to-canvas transform.
    #[inline]
    pub const fn with_transform(mut self, transform: DamageTransform) -> Self {
        self.transform = transform;
        self
    }

    /// Expands the footprint for shadows, filters, and other bounded effects.
    #[inline]
    pub const fn with_effect_expansion(mut self, effect_expansion: f32) -> Self {
        self.effect_expansion = effect_expansion;
        self
    }

    fn footprint(self) -> Result<Option<DamageBounds>, ()> {
        let bounds = self.bounds.transformed(self.transform)?.expanded(self.effect_expansion)?;
        let Some(clip) = self.clip else {
            return Ok(Some(bounds));
        };
        let clip = clip.transformed(self.transform)?;
        Ok(bounds.intersect(clip))
    }
}

/// A retained-layer transition that can contribute one or more damage areas.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageLayerChange {
    layer_id: u64,
    old: Option<DamageGeometry>,
    new: Option<DamageGeometry>,
    paint_invalidated: bool,
    ordering_changed: bool,
    resource_changed: bool,
}

impl DamageLayerChange {
    /// Creates a transition from the previous to the current layer footprint.
    #[inline]
    pub const fn new(
        layer_id: u64,
        old: Option<DamageGeometry>,
        new: Option<DamageGeometry>,
    ) -> Self {
        Self {
            layer_id,
            old,
            new,
            paint_invalidated: false,
            ordering_changed: false,
            resource_changed: false,
        }
    }

    /// Marks the retained paint as needing a fresh recording.
    #[inline]
    pub const fn with_paint_invalidated(mut self, value: bool) -> Self {
        self.paint_invalidated = value;
        self
    }

    /// Marks an ordering change whose affected layers are not locally known.
    #[inline]
    pub const fn with_ordering_changed(mut self, value: bool) -> Self {
        self.ordering_changed = value;
        self
    }

    /// Marks a resource readiness or replacement transition.
    #[inline]
    pub const fn with_resource_changed(mut self, value: bool) -> Self {
        self.resource_changed = value;
        self
    }

    /// Returns the retained layer identity.
    #[inline]
    pub const fn layer_id(self) -> u64 {
        self.layer_id
    }
}

/// Derives normalized damage from retained-layer transitions.
#[doc(hidden)]
pub struct DamageTracker {
    damage: DamageSet,
    device_scale: f32,
    scale_valid: bool,
}

impl DamageTracker {
    /// Creates a tracker using the default full-frame promotion policy.
    #[inline]
    pub fn new(width: u32, height: u32, device_scale: f32) -> Self {
        Self::with_policy(width, height, device_scale, DamagePolicy::default())
    }

    /// Creates a tracker with an explicit promotion policy.
    #[inline]
    pub fn with_policy(
        width: u32,
        height: u32,
        device_scale: f32,
        policy: DamagePolicy,
    ) -> Self {
        let scale_valid = device_scale.is_finite() && device_scale > 0.0;
        let mut damage = DamageSet::with_policy(width, height, policy);
        if !scale_valid {
            damage.promote_full_frame();
        }
        Self {
            damage,
            device_scale: if scale_valid { device_scale } else { 1.0 },
            scale_valid,
        }
    }

    /// Records one retained-layer transition and returns the normalized result.
    #[inline]
    pub fn record(&mut self, change: DamageLayerChange) -> DamageAddResult {
        if self.damage.is_full_frame() {
            return DamageAddResult::PromotedToFull;
        }
        if !self.scale_valid || change.ordering_changed {
            self.damage.promote_full_frame();
            return DamageAddResult::PromotedToFull;
        }

        let geometry_changed = change.old != change.new;
        if !geometry_changed
            && !change.paint_invalidated
            && !change.resource_changed
        {
            return DamageAddResult::Ignored;
        }
        if change.old.is_none() && change.new.is_none() {
            self.damage.promote_full_frame();
            return DamageAddResult::PromotedToFull;
        }

        let mut result = DamageAddResult::Ignored;
        for geometry in [change.old, change.new].into_iter().flatten() {
            let next = self.record_geometry(geometry);
            if next == DamageAddResult::PromotedToFull {
                return next;
            }
            result = merge_damage_results(result, next);
        }
        result
    }

    /// Returns the damage derived from all records so far.
    #[inline]
    pub fn damage(&self) -> &DamageSet {
        &self.damage
    }

    /// Clears records while retaining the tracker allocation and policy.
    #[inline]
    pub fn reset(&mut self) {
        self.damage.reset();
        if !self.scale_valid {
            self.damage.promote_full_frame();
        }
    }

    fn record_geometry(&mut self, geometry: DamageGeometry) -> DamageAddResult {
        let bounds = match geometry.footprint() {
            Ok(Some(bounds)) => bounds,
            Ok(None) => return DamageAddResult::Ignored,
            Err(()) => {
                self.damage.promote_full_frame();
                return DamageAddResult::PromotedToFull;
            }
        };
        self.damage.add_f32_rect(
            bounds.x * self.device_scale,
            bounds.y * self.device_scale,
            bounds.width * self.device_scale,
            bounds.height * self.device_scale,
        )
    }
}

#[inline]
fn merge_damage_results(
    first: DamageAddResult,
    second: DamageAddResult,
) -> DamageAddResult {
    match (first, second) {
        (DamageAddResult::PromotedToFull, _) | (_, DamageAddResult::PromotedToFull) => {
            DamageAddResult::PromotedToFull
        }
        (DamageAddResult::Ignored, result) | (result, DamageAddResult::Ignored) => result,
        (DamageAddResult::Merged, _) | (_, DamageAddResult::Merged) => DamageAddResult::Merged,
        _ => DamageAddResult::Added,
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

    #[test]
    fn layer_damage_covers_old_and_new_transformed_bounds() {
        let mut tracker = DamageTracker::new(100, 80, 1.0);
        let old = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 10.0, 10.0));
        let new = DamageGeometry::new(DamageBounds::new(40.0, 20.0, 10.0, 10.0))
            .with_transform(DamageTransform::translation(3.0, 4.0));

        assert_eq!(
            tracker.record(
                DamageLayerChange::new(1, Some(old), Some(new)).with_paint_invalidated(true)
            ),
            DamageAddResult::Added
        );
        assert_eq!(
            tracker.damage().regions(),
            &[
                DamageRect::new(10, 10, 10, 10),
                DamageRect::new(43, 24, 10, 10),
            ]
        );
    }

    #[test]
    fn clip_and_transform_changes_damage_both_visible_footprints() {
        let mut tracker = DamageTracker::new(100, 80, 1.0);
        let old = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 20.0, 20.0))
            .with_clip(Some(DamageBounds::new(10.0, 10.0, 10.0, 10.0)));
        let new = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 20.0, 20.0))
            .with_clip(Some(DamageBounds::new(0.0, 0.0, 30.0, 30.0)))
            .with_transform(DamageTransform::translation(30.0, 5.0));

        assert_eq!(
            tracker.record(DamageLayerChange::new(2, Some(old), Some(new))),
            DamageAddResult::Added
        );
        assert_eq!(
            tracker.damage().regions(),
            &[
                DamageRect::new(10, 10, 10, 10),
                DamageRect::new(40, 15, 20, 20),
            ]
        );
    }

    #[test]
    fn resource_changes_expand_the_old_and_new_effect_envelope() {
        let mut tracker = DamageTracker::new(100, 80, 1.0);
        let geometry = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 20.0, 20.0))
            .with_effect_expansion(3.0);

        assert_eq!(
            tracker.record(
                DamageLayerChange::new(3, Some(geometry), Some(geometry))
                    .with_resource_changed(true)
            ),
            DamageAddResult::Merged
        );
        assert_eq!(tracker.damage().regions(), &[DamageRect::new(7, 7, 26, 26)]);
    }

    #[test]
    fn ordering_or_unknown_geometry_promotes_to_full_frame() {
        let geometry = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 20.0, 20.0));

        let mut ordering = DamageTracker::new(100, 80, 1.0);
        assert_eq!(
            ordering.record(
                DamageLayerChange::new(4, Some(geometry), Some(geometry))
                    .with_ordering_changed(true)
            ),
            DamageAddResult::PromotedToFull
        );
        assert!(ordering.damage().is_full_frame());

        let mut unknown = DamageTracker::new(100, 80, 1.0);
        assert_eq!(
            unknown.record(DamageLayerChange::new(5, None, None).with_paint_invalidated(true)),
            DamageAddResult::PromotedToFull
        );
        assert!(unknown.damage().is_full_frame());
    }

    #[test]
    fn unchanged_layers_do_not_create_damage() {
        let geometry = DamageGeometry::new(DamageBounds::new(10.0, 10.0, 20.0, 20.0));
        let mut tracker = DamageTracker::new(100, 80, 1.0);

        assert_eq!(
            tracker.record(DamageLayerChange::new(6, Some(geometry), Some(geometry))),
            DamageAddResult::Ignored
        );
        assert!(tracker.damage().regions().is_empty());
    }
}
