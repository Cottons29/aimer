pub mod image_pipeline;
pub mod rect_pipeline;
pub mod svg_pipeline;
pub mod text_pipeline;

/// Selects the edge antialiasing strategy used by Cupid's render pipelines.
///
/// [`Self::Analytic`] renders directly to the surface with one sample. Rounded
/// rectangles, borders, clips, images, and text retain their shader- or
/// coverage-based antialiasing without allocating a full-window multisample
/// texture. The MSAA variants add hardware multisampling for tessellated and
/// custom geometry that cannot use analytic coverage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AntiAlias {
    /// Use analytic shader coverage and signed-distance fields without MSAA.
    #[default]
    Analytic,
    /// Use analytic coverage together with two-sample MSAA.
    Msaa2x,
    /// Use analytic coverage together with four-sample MSAA.
    Msaa4x,
}

impl AntiAlias {
    /// Returns the sample count required by this antialiasing strategy.
    #[inline]
    pub const fn sample_count(self) -> u32 {
        match self {
            Self::Analytic => 1,
            Self::Msaa2x => 2,
            Self::Msaa4x => 4,
        }
    }

    /// Returns whether this strategy requires a multisampled color target.
    #[inline]
    pub const fn uses_multisampling(self) -> bool {
        self.sample_count() > 1
    }
}

pub(crate) fn multisample_state(antialiasing: AntiAlias) -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: antialiasing.sample_count(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antialiasing_modes_select_the_expected_sample_count() {
        assert_eq!(AntiAlias::Analytic.sample_count(), 1);
        assert_eq!(AntiAlias::Msaa2x.sample_count(), 2);
        assert_eq!(AntiAlias::Msaa4x.sample_count(), 4);
    }

    #[test]
    fn analytic_antialiasing_does_not_require_a_multisample_target() {
        assert!(!AntiAlias::Analytic.uses_multisampling());
        assert!(AntiAlias::Msaa2x.uses_multisampling());
        assert!(AntiAlias::Msaa4x.uses_multisampling());
    }
}
