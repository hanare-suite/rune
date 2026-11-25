//! A [`TextSystem`]'s animated icons: classifying them and reading their frames.

use super::*;

impl TextSystem {
    /// Whether any monochrome animated icon has been classified in this `TextSystem` yet.
    /// Single boolean guard keeping static-only apps free of any animated-icon check in `El::cache`.
    #[doc(hidden)]
    #[inline]
    pub fn has_animated_icons(&self) -> bool {
        self.animated_icons.has_any()
    }

    /// Whether `icon` is a finite-duration animated icon (so callers should track mount-relative
    /// time rather than sampling the global runtime clock).
    #[doc(hidden)]
    #[inline]
    pub fn icon_anim_is_finite(&mut self, icon: &'static Icon) -> bool {
        let icon = icon::swapped(icon);
        self.animated_icons.is_finite(icon as *const Icon as usize)
    }

    /// Whether animated `icon` is still live (moving or waiting for a future segment) at runtime
    /// time `t` (with `mount_t` used for finite-duration icons).
    #[doc(hidden)]
    pub fn icon_anim_live(&mut self, icon: &'static Icon, t: f64, mount_t: f64) -> bool {
        if self.icon_kind(icon) != IconKind::AnimatedMono {
            return false;
        }
        let icon = icon::swapped(icon);
        self.animated_icons.is_live_at(icon as *const Icon as usize, t, mount_t)
    }

    /// Whether both double-buffered atlas tiles for animated `icon` are already resident on
    /// non-evicted pages, so a loop-only step can update it in place without allocating or
    /// evicting a glyph page.
    #[doc(hidden)]
    pub fn icon_anim_slots_ready(
        &self,
        icon: &'static Icon,
        sized: (u32, f32),
        facing: Facing,
        scope_id: u64,
    ) -> bool {
        let icon = icon::swapped(icon);
        self.animated_icons.slots_ready(&self.atlas, icon as *const Icon as usize, sized, facing, scope_id)
    }

    /// Evaluates monochrome animated `icon` into a double-buffered glyph-atlas tile, flipping
    /// between its two slots only when quantized coverage bytes change.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn animated_icon_quad(
        &mut self,
        icon: &'static Icon,
        sized: (u32, f32),
        origin: [f32; 2],
        facing: Facing,
        t: f64,
        mount_t: f64,
        scope_id: u64,
    ) -> Option<AnimatedQuad> {
        let icon = icon::swapped(icon);
        self.animated_icons.frame_quad(&mut self.atlas, icon as *const Icon as usize, sized, origin, facing, t, mount_t, scope_id)
    }

    /// Loop-step variant of [`Self::animated_icon_quad`] that updates only pre-allocated slots in
    /// place via [`atlas::InPlaceAtlas`] and cannot allocate in or evict from the glyph atlas.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn animated_icon_quad_in_place(
        &mut self,
        icon: &'static Icon,
        sized: (u32, f32),
        origin: [f32; 2],
        facing: Facing,
        t: f64,
        mount_t: f64,
        scope_id: u64,
    ) -> Option<AnimatedQuad> {
        let icon = icon::swapped(icon);
        self.animated_icons.frame_quad_in_place(self.atlas.in_place(), icon as *const Icon as usize, sized, origin, facing, t, mount_t, scope_id)
    }
}
