use super::*;

pub(super) struct ElementNode<E> {
    pub(super) id: Cell<ElementId>,
    pub(super) element: E,
}

impl<E: Element + 'static> ElementNode<E> {
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
    fn clear_retained_compositor_animation(&self, render_context: Option<&V2RenderContext>) {
        let Some(scope) = render_context.filter(|scope| scope.uses_retained_presentation()) else {
            return;
        };
        let Some(render_node) = scope.node_for_element(self.id.get()) else {
            return;
        };
        let _ = scope.tree.set_compositor_animation(
            render_node,
            aimer_cupid::utilities::Mat3::identity(),
            1.0,
            None,
        );
    }

    #[inline]
    fn draw_live(&self, ctx: &BuildContext, frame: Option<CompositorAnimationFrame>) {
        let before = element_tree_generation();
        if let Some(frame) = frame {
            self.element.draw_with_compositor_animation(ctx, frame);
        } else {
            self.element.draw(ctx);
        }
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
    }

    #[inline]
    fn draw_local_v2_compatibility_live(&self, ctx: &BuildContext) {
        let before = element_tree_generation();
        self.element.draw_local_v2_compatibility(ctx);
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
    fn draw_compositor_animation(&self, ctx: &BuildContext, frame: CompositorAnimationFrame) {
        let bounds = self.element.content_size(ctx);
        let priority = self.element.compositor_priority();
        let descriptor = SceneNodeDescriptor {
            id: SceneNodeId::from_raw(self.id.get().get()),
            bounds: Rect::new(0.0, 0.0, bounds.width, bounds.height),
            order: self.element.layer(),
            revision: SceneRevision::new(
                self.subtree_generation(),
                paint_element_was_invalidated(self.id.get())
                    .then_some(rebuild_invalidation_generation())
                    .unwrap_or(0),
                layout_invalidation_generation(),
                ctx.canvas.texture_cache_epoch(),
            ),
            // The provider has proved that its static paint is safe to retain;
            // the animation itself is represented by the scene state captured
            // after `frame.apply` below.
            cache_eligible: true,
            bounded: true,
            priority,
        };

        ctx.canvas.save();
        frame.apply(ctx);
        let scene_node = ctx.canvas.begin_scene_node(descriptor);
        self.element.sync_paint_geometry(ctx);

        let retained = if let Some(key) = crate::paint_isolated::PaintContract::new(
            ctx,
            bounds,
            self.subtree_generation(),
        ) {
            crate::paint_isolated::paint_or_replay_scene_node(
                self.id.get(),
                ctx,
                &self.element,
                key,
                priority,
                true,
            )
        } else {
            false
        };

        if retained {
            self.element.update_compositor_animation_damage(ctx, frame);
        }

        drop(scene_node);
        frame.clear(ctx);
        ctx.canvas.restore();

        if retained {
            if frame.active {
                request_animation_frame();
            }
        } else {
            // The retained recorder can reject a command stream or be
            // invalidated by an unknown paint source. Replay the same sample
            // on the live path so the controller is not ticked twice.
            self.draw_live(ctx, Some(frame));
        }
    }

    fn record_local_v2_paint(
        &self,
        ctx: &BuildContext,
        render_context: &V2RenderContext,
        render_node: RenderNodeId,
    ) -> bool {
        if !self.element.can_paint_local_v2(ctx) {
            return false;
        }
        let Ok(build_context) = render_context.tree.context(render_node) else {
            return false;
        };
        ctx.with_local_v2_paint_context(build_context, |ctx| {
            self.element.paint_local_v2(ctx)
        });
        render_context
            .tree
            .set_paint_source(render_node, RenderPaintSource::LocalV2)
            .is_ok()
    }

    #[inline]
    fn sync_local_v2_state(
        &self,
        ctx: &BuildContext,
        render_context: &V2RenderContext,
        render_node: RenderNodeId,
    ) -> bool {
        let Ok(paint_context) = render_context.tree.context(render_node) else {
            return false;
        };
        ctx.with_local_v2_paint_context(paint_context, |ctx| {
            ctx.canvas.with_paint_commands_suppressed(|| {
                self.element.sync_local_v2_state(ctx)
            })
        })
    }

    fn draw_v2_compatibility(&self, ctx: &BuildContext, priority: bool, stable: bool, bounded: bool) {
        ctx.with_local_v2_compatibility_paint(true, |ctx| {
            if has_active_v2_render_presentation() {
                ctx.canvas.with_paint_commands_suppressed(|| {
                    self.draw_local_v2_compatibility_live(ctx);
                });
                return;
            }

            if (stable && bounded) || priority {
                let bounds = self.element.content_size(ctx);
                let descriptor = SceneNodeDescriptor {
                    id: SceneNodeId::from_raw(self.id.get().get()),
                    bounds: Rect::new(0.0, 0.0, bounds.width, bounds.height),
                    order: self.element.layer(),
                    revision: SceneRevision::new(
                        self.subtree_generation(),
                        paint_element_was_invalidated(self.id.get())
                            .then_some(rebuild_invalidation_generation())
                            .unwrap_or(0),
                        layout_invalidation_generation(),
                        ctx.canvas.texture_cache_epoch(),
                    ),
                    cache_eligible: stable && bounded,
                    bounded,
                    priority,
                };
                let _scene_node = ctx.canvas.begin_scene_node(descriptor);
                self.element.sync_paint_geometry(ctx);
                self.draw_live(ctx, None);
            } else {
                self.draw_live(ctx, None);
            }
        });
    }

    fn draw_legacy_content(&self, ctx: &BuildContext, priority: bool, stable: bool, bounded: bool) {
        let _scene_node = if (stable && bounded) || priority {
            let bounds = self.element.content_size(ctx);
            let descriptor = SceneNodeDescriptor {
                id: SceneNodeId::from_raw(self.id.get().get()),
                bounds: Rect::new(0.0, 0.0, bounds.width, bounds.height),
                order: self.element.layer(),
                revision: SceneRevision::new(
                    self.subtree_generation(),
                    paint_element_was_invalidated(self.id.get())
                        .then_some(rebuild_invalidation_generation())
                        .unwrap_or(0),
                    layout_invalidation_generation(),
                    ctx.canvas.texture_cache_epoch(),
                ),
                cache_eligible: stable && bounded,
                bounded,
                priority,
            };
            let scene_node = Some(ctx.canvas.begin_scene_node(descriptor));
            self.element.sync_paint_geometry(ctx);

            #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
            if stable
                && bounded
                && let Some(key) = crate::paint_isolated::PaintContract::new(
                    ctx,
                    bounds,
                    self.subtree_generation(),
                )
                && crate::paint_isolated::paint_or_replay_scene_node(
                    self.id.get(),
                    ctx,
                    &self.element,
                    key,
                    priority,
                    stable,
                )
            {
                return;
            }
            scene_node
        } else {
            None
        };
        self.draw_live(ctx, None);
    }

    fn draw_legacy_island(
        &self,
        ctx: &BuildContext,
        render_context: &V2RenderContext,
        render_node: RenderNodeId,
        draw: impl FnOnce(),
    ) {
        ctx.with_local_v2_compatibility_paint(false, |_| {
            ctx.canvas.with_paint_commands_enabled(|| {
                let _ = render_context
                    .tree
                    .set_paint_source(render_node, RenderPaintSource::LegacyIsland);
                let start = legacy_draw_command_cursor(ctx);
                let _legacy_island = render_context.enter_legacy_island();
                draw();
                let end = legacy_draw_command_cursor(ctx);
                let _ = render_context
                    .tree
                    .set_legacy_command_range(render_node, start.zip(end));
            });
        });
    }
}

#[cfg(not(feature = "portable-guest"))]
fn legacy_draw_command_cursor(ctx: &BuildContext) -> Option<usize> {
    Some(
        ctx.canvas
            .get_inner_canvas()
            .draw_list()
            .commands()
            .len(),
    )
}

#[cfg(feature = "portable-guest")]
fn legacy_draw_command_cursor(_ctx: &BuildContext) -> Option<usize> {
    None
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
impl<E> Drop for ElementNode<E> {
    fn drop(&mut self) {
        crate::paint_isolated::drop_scene_paint_cache(self.id.get());
    }
}

unsafe impl<E: Element + 'static> ErasedFrom<ElementNode<E>> for dyn Element {
    const TEMPLATE: *const Self = std::ptr::null::<ElementNode<E>>() as *const dyn Element;
}

impl<E: Element + 'static> VisitorElement for ElementNode<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.visit_children(visitor);
    }

    fn visit_retained_v2_children<'a>(
        &'a self,
        visitor: &mut dyn FnMut(usize, &'a dyn Element),
    ) {
        self.element.visit_retained_v2_children(visitor);
    }

    fn debug_name(&self) -> &'static str {
        self.element.debug_name()
    }

    fn element_type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn reconciliation_key(&self) -> Option<&Key> {
        self.element.reconciliation_key()
    }

    fn element_id(&self) -> Option<ElementId> {
        Some(self.id.get())
    }

    fn set_element_id(&self, id: ElementId) {
        self.id.set(id);
    }
}

impl<E: Element + 'static> LayoutElement for ElementNode<E> {
    fn pos(&self) -> Option<Vec2d> {
        self.element.pos()
    }

    fn size(&self) -> Option<Size> {
        self.element.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        crate::frame_work_stats::record_layout_call();
        self.element.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.element.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.element.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.element.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.element.flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.element.is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.element.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        // Layout caches carry this generation in their keys, so invalidating
        // the retained root is one marker write rather than a recursive walk
        // through every child. The concrete element's old recursive method is
        // still available when a caller owns that concrete element directly;
        // erased trees use this boundary for the normal frame path.
        advance_layout_invalidation_generation();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.element.pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.element.event_tree_bounds()
    }
}

impl<E: Element + 'static> Rebuildable for ElementNode<E> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        #[cfg(feature = "frame-stats")]
        crate::rebuild_stats::record_visit();
        let mut traversal = begin_rebuild_traversal(self.id.get());
        let _path = RebuildPathGuard::push(self.id.get());
        let own_dirty = element_has_dirty_work(&self.element);
        let path_ready = DIRTY_PATHS_READY.with(Cell::get)
            && (REBUILD_TRAVERSAL_DEPTH.with(|depth| depth.get() > 1)
                || DIRTY_INDEXED_ROOTS.with(|roots| roots.borrow().contains(&self.id.get())));
        let forced_descend = REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.get() > 0);
        if path_ready
            && !forced_descend
            && !own_dirty
            && !dirty_path_contains(self.id.get())
        {
            traversal.complete = true;
            #[cfg(feature = "frame-stats")]
            crate::rebuild_stats::record_pruned();
            return;
        }

        set_element_rebuild_path(&self.element);
        let _descend = RebuildDescendGuard::enter(own_dirty);
        let before = element_tree_generation();
        self.element.rebuild_if_dirty(ctx);
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
        // A descendant rebuild advances the shared generation but does not
        // stale this node's own local paint. The descendant's dirty path and
        // retained-tree sync carry that change to its actual paint owner.
        if own_dirty || after != before {
            record_current_paint_invalidation(self.id.get(), own_dirty);
        }
        traversal.complete = true;
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.element.option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.element.is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.element.is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.element.compositor_priority()
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let before = element_tree_generation();
        self.element.adopt_runtime_state_from(old);
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.element.with_rebuild_context(ctx, callback);
    }

    fn mark_needs_rebuild(&self) {
        let (_mark, outermost) = begin_rebuild_mark();
        let has_rebuild_source = element_has_rebuild_source(&self.element);
        // The erased owner is the boundary used by the retained tree. Keep one
        // invalidation generation for the complete recursive mark, including
        // custom containers that implement their own forwarding method.
        let unknown = outermost && !has_rebuild_source;
        with_rebuild_invalidation(|| {
            if unknown {
                // A custom rebuild producer has no path to publish. Preserve
                // the old conservative behavior so its mark can never make
                // the index incorrectly skip work.
                mark_paint_invalidations_unknown();
                invalidate_dirty_paths();
            }
            self.element.mark_needs_rebuild();
        });
        // A stateful element may delegate the mark to a carrying child without
        // marking itself. If that child contains an untracked custom producer,
        // the retained path index has no source to point at; fall back to the
        // conservative walk for this explicit recursive mark.
        if outermost && has_rebuild_source && !element_has_dirty_work(&self.element) {
            mark_paint_invalidations_unknown();
            invalidate_dirty_paths();
        }
    }

    fn subtree_generation(&self) -> u64 {
        if !self.element.is_layout_stable() {
            return element_tree_generation();
        }
        STABLE_SUBTREE_GENERATIONS.with(|generations| {
            generations.borrow().get(&self.id.get()).copied().unwrap_or(0)
        })
    }

    fn set_subtree_generation(&self, generation: u64) {
        if self.element.is_layout_stable() {
            STABLE_SUBTREE_GENERATIONS.with(|generations| {
                generations.borrow_mut().insert(self.id.get(), generation);
            });
        }
    }
}

fn element_has_rebuild_source(element: &dyn Element) -> bool {
    let Some(value) = element.option_any() else {
        return false;
    };
    value.is::<crate::widget::stateless::StatelessElement>()
        || value.is::<crate::widget::stateful::StatefulElement>()
}

fn element_has_dirty_work(element: &dyn Element) -> bool {
    let Some(value) = element.option_any() else {
        return false;
    };
    if let Some(stateless) = value
        .downcast_ref::<crate::widget::stateless::StatelessElement>()
    {
        return stateless.dirty.get();
    }
    value
        .downcast_ref::<crate::widget::stateful::StatefulElement>()
        .is_some_and(|stateful| stateful.dirty.borrow().get())
}

fn set_element_rebuild_path(element: &dyn Element) {
    let Some(value) = element.option_any() else {
        return;
    };
    REBUILD_PATH.with(|path| {
        let path = path.borrow();
        if let Some(stateless) = value
            .downcast_ref::<crate::widget::stateless::StatelessElement>()
        {
            stateless.dirty_source.set_path(&path);
        } else if let Some(stateful) = value
            .downcast_ref::<crate::widget::stateful::StatefulElement>()
        {
            stateful.dirty_source.borrow().set_path(&path);
        }
    });
}

impl<E: Element + 'static> EventElement for ElementNode<E> {
    fn event_tree_role(&self) -> EventTreeRole {
        self.element.event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.element.focus_node()
    }

    fn autofocus(&self) -> bool {
        self.element.autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.element.traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let result = self.element.on_event(event);
        if result.needs_redraw() {
            super::mark_event_paint_invalidated(self.id.get());
        }
        result
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        let result = self.element.on_event_with_context(event, context);
        if result.needs_redraw() {
            super::mark_event_paint_invalidated(self.id.get());
        }
        result
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.event_children(visitor);
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.structural_children(visitor);
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.focus_children(visitor);
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.hit_test_children(visitor);
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.hit_test_children_reversed(visitor);
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.element.hit_test_children_at(pos, visitor);
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.element.hit_test_children_at_reversed(pos, visitor);
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.element.has_overlapping_hit_targets()
    }

}

impl<E: Element + 'static> Drawable for ElementNode<E> {
    fn draw(&self, ctx: &BuildContext) {
        crate::frame_work_stats::record_paint_call();
        #[cfg(feature = "frame-stats")]
        record_draw_traversal();
        record_paint_element(self.id.get());
        let (_draw, outermost) = begin_draw();
        let render_context = current_v2_render_context();
        let prepared_root = outermost
            && render_context
                .as_ref()
                .is_some_and(|scope| scope.take_prepared_root(self.id.get()));
        if outermost && !prepared_root {
            // Native frame dispatch enters through `draw`; keep the retained-
            // tree rebuild prepass here so direct draw callers also benefit
            // from the precise dirty-subtree index. Child draws belong to the
            // same pass and must not reset paths relative to a new root.
            self.rebuild_if_dirty(ctx);
        }
        let priority = self.element.compositor_priority();
        let stable = self.element.is_paint_stable() && self.element.is_layout_stable();
        let bounded = self.element.is_paint_bounded();
        let _invalidation_owner = (!stable || !bounded || self.element.is_stateful_element())
            .then(|| DrawInvalidationOwnerGuard::enter(self.id.get()));

        #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
        let retained_animation_candidate = render_context.as_ref().is_some_and(|scope| {
            scope.uses_retained_presentation()
                && scope.node_for_element(self.id.get()).is_some()
                && self.element.can_paint_local_v2(ctx)
        });
        #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
        if !stable && (bounded || retained_animation_candidate) {
            match self.element.compositor_animation(ctx) {
                CompositorAnimationDecision::Compositor(frame) => {
                    if frame.valid
                        && self.element.can_paint_local_v2(ctx)
                        && let Some(scope) = render_context
                            .as_ref()
                            .filter(|scope| scope.uses_retained_presentation())
                        && let Some(render_node) = scope.node_for_element(self.id.get())
                        && scope
                            .tree
                            .set_compositor_animation(
                                render_node,
                                compositor_transform_matrix(frame.transform, ctx.scale),
                                frame.opacity.unwrap_or(1.0),
                                frame.clip.map(|size| {
                                    Rect::new(0.0, 0.0, size.width, size.height)
                                }),
                            )
                            .is_ok()
                    {
                        self.draw_v2_compatibility(ctx, priority, stable, bounded);
                        if frame.active {
                            request_animation_frame();
                        }
                        return;
                    }
                    self.clear_retained_compositor_animation(render_context.as_ref());
                    if let Some(scope) = render_context
                        .as_ref()
                        .filter(|scope| !scope.inside_legacy_island())
                        && let Some(render_node) = scope.node_for_element(self.id.get())
                    {
                        self.draw_legacy_island(ctx, scope, render_node, || {
                            self.draw_compositor_animation(ctx, frame);
                        });
                    } else {
                        self.draw_compositor_animation(ctx, frame);
                    }
                    return;
                }
                CompositorAnimationDecision::Live(frame) => {
                    self.clear_retained_compositor_animation(render_context.as_ref());
                    if let Some(scope) = render_context
                        .as_ref()
                        .filter(|scope| !scope.inside_legacy_island())
                        && let Some(render_node) = scope.node_for_element(self.id.get())
                    {
                        self.draw_legacy_island(ctx, scope, render_node, || {
                            self.draw_live(ctx, Some(frame));
                        });
                    } else {
                        self.draw_live(ctx, Some(frame));
                    }
                    return;
                }
                CompositorAnimationDecision::None => {
                    self.clear_retained_compositor_animation(render_context.as_ref());
                }
            }
        }

        if let Some(scope) = render_context.as_ref() {
            if scope.inside_legacy_island() {
                self.draw_legacy_content(ctx, priority, stable, bounded);
                return;
            }
            if let Some(render_node) = scope.node_for_element(self.id.get()) {
                let source = scope
                    .tree
                    .paint_source(render_node)
                    .unwrap_or(RenderPaintSource::Unresolved);
                if source == RenderPaintSource::LegacyIsland {
                    if paint_subtree_was_invalidated(self.id.get())
                        || super::take_event_paint_invalidated(self.id.get())
                    {
                        let _ = scope.tree.invalidate_legacy_island(render_node);
                    }
                    // An element can become v2-capable after asynchronous
                    // content arrives. Retry the promotion before recording
                    // another legacy island frame.
                    if self.element.can_paint_local_v2(ctx)
                        && scope
                            .tree
                            .set_paint_source(render_node, RenderPaintSource::Unresolved)
                            .is_ok()
                    {
                        let state_ready = !scope.uses_retained_presentation()
                            || self.sync_local_v2_state(ctx, scope, render_node);
                        if state_ready && self.record_local_v2_paint(ctx, scope, render_node) {
                            self.draw_v2_compatibility(ctx, priority, stable, bounded);
                            return;
                        }
                        let _ = scope
                            .tree
                            .set_paint_source(render_node, RenderPaintSource::LegacyIsland);
                    }
                    self.draw_legacy_island(ctx, scope, render_node, || {
                        self.draw_legacy_content(ctx, priority, stable, bounded);
                    });
                    return;
                }

                if scope.uses_retained_presentation()
                    && (source == RenderPaintSource::LocalV2
                        || self.element.can_paint_local_v2(ctx))
                    && !self.sync_local_v2_state(ctx, scope, render_node)
                {
                    self.draw_legacy_island(ctx, scope, render_node, || {
                        self.draw_legacy_content(ctx, priority, stable, bounded);
                    });
                    return;
                }

                if source == RenderPaintSource::LocalV2
                    && (super::take_event_paint_invalidated(self.id.get())
                        || local_paint_element_was_invalidated(self.id.get())
                        || self.element.local_v2_paint_needs_recording(ctx))
                {
                    let _ = scope.tree.invalidate_paint(render_node);
                }
                let needs_recording = source == RenderPaintSource::Unresolved
                    || scope.tree.needs_recording(render_node).unwrap_or(true);
                if needs_recording && !self.record_local_v2_paint(ctx, scope, render_node) {
                    self.draw_legacy_island(ctx, scope, render_node, || {
                        self.draw_legacy_content(ctx, priority, stable, bounded);
                    });
                    return;
                }

                // Keep the compatibility traversal for draw-time state and
                // descendants. Direct retained presentation drops its paint
                // commands in `draw_v2_compatibility`; legacy islands reopen
                // recording for their own ranges.
                self.draw_v2_compatibility(ctx, priority, stable, bounded);
                return;
            }
        }

        self.draw_legacy_content(ctx, priority, stable, bounded);
    }

    #[inline]
    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool {
        self.element.can_paint_local_v2(ctx)
    }

    #[inline]
    fn paint_local_v2(&self, ctx: &BuildContext) {
        self.element.paint_local_v2(ctx)
    }

    #[inline]
    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        self.element.sync_local_v2_state(ctx)
    }

    #[inline]
    fn retained_clip(&self, ctx: &BuildContext) -> Option<aimer_cupid::draw_cmd_v2::Rect> {
        self.element.retained_clip(ctx)
    }

    #[inline]
    fn retained_v2_bounds(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        self.element.retained_v2_bounds(ctx)
    }

    #[inline]
    fn retained_v2_paint_outsets(&self, ctx: &BuildContext) -> Option<[f32; 4]> {
        self.element.retained_v2_paint_outsets(ctx)
    }

    #[inline]
    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        self.element.retained_v2_child_context(ctx, child)
    }

    #[inline]
    fn retained_v2_child_context_at<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<BuildContext<'a>> {
        self.element
            .retained_v2_child_context_at(ctx, child, child_index)
    }

    #[inline]
    fn retained_v2_child_geometry(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.element.retained_v2_child_geometry(ctx, child)
    }

    #[inline]
    fn retained_v2_child_geometry_at(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.element
            .retained_v2_child_geometry_at(ctx, child, child_index)
    }

    #[inline]
    fn local_v2_paint_needs_recording(&self, ctx: &BuildContext) -> bool {
        self.element.local_v2_paint_needs_recording(ctx)
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        record_paint_element(self.id.get());
        self.element.paint(ctx);
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.element.sync_paint_geometry(ctx);
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.element.prepare_layout(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.element.is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.element.is_paint_bounded()
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.element.draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.element.compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.element.draw_with_compositor_animation(ctx, frame);
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.element
            .update_compositor_animation_damage(ctx, frame);
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
fn compositor_transform_matrix(
    transform: CompositorTransform,
    device_scale: f32,
) -> aimer_cupid::utilities::Mat3 {
    use aimer_cupid::utilities::Mat3;

    let device_scale = if device_scale.is_finite() && device_scale > 0.0 {
        device_scale
    } else {
        1.0
    };
    match transform {
        CompositorTransform::Identity => Mat3::identity(),
        CompositorTransform::Translate { x, y } => {
            Mat3::translate(x / device_scale, y / device_scale)
        }
        CompositorTransform::Scale {
            sx,
            sy,
            origin_x,
            origin_y,
        } => Mat3::translate(origin_x / device_scale, origin_y / device_scale)
            .mul(&Mat3::scale(sx, sy))
            .mul(&Mat3::translate(-origin_x / device_scale, -origin_y / device_scale)),
        CompositorTransform::Rotate {
            radians,
            origin_x,
            origin_y,
        } => Mat3::translate(origin_x / device_scale, origin_y / device_scale)
            .mul(&Mat3::rotate(radians))
            .mul(&Mat3::translate(-origin_x / device_scale, -origin_y / device_scale)),
    }
}

impl VisitorElement for AnyElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }

    fn visit_retained_v2_children<'a>(
        &'a self,
        visitor: &mut dyn FnMut(usize, &'a dyn Element),
    ) {
        self.as_ref().visit_retained_v2_children(visitor)
    }

    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }

    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref().element_type_id()
    }

    fn reconciliation_key(&self) -> Option<&Key> {
        self.as_ref().reconciliation_key()
    }

    fn element_id(&self) -> Option<ElementId> {
        self.as_ref().element_id()
    }

    fn set_element_id(&self, id: ElementId) {
        self.as_ref().set_element_id(id);
    }
}

impl LayoutElement for AnyElement {
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }

    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.as_ref().is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().event_tree_bounds()
    }
}

impl Rebuildable for AnyElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        self.as_ref().adopt_runtime_state_from(old)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.as_ref().is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.as_ref().compositor_priority()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }

    fn subtree_generation(&self) -> u64 {
        self.as_ref().subtree_generation()
    }

    fn set_subtree_generation(&self, generation: u64) {
        self.as_ref().set_subtree_generation(generation)
    }
}

impl EventElement for AnyElement {
    fn event_tree_role(&self) -> EventTreeRole {
        self.as_ref().event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.as_ref().focus_node()
    }

    fn autofocus(&self) -> bool {
        self.as_ref().autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.as_ref().traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.as_ref().on_event_with_context(event, context)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().structural_children(visitor)
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().focus_children(visitor)
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children(visitor)
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children_reversed(visitor)
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at(pos, visitor)
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at_reversed(pos, visitor)
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.as_ref().has_overlapping_hit_targets()
    }

}

impl Drawable for AnyElement {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.as_ref().paint(ctx)
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.as_ref().sync_paint_geometry(ctx)
    }

    #[inline]
    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        self.as_ref().sync_local_v2_state(ctx)
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.as_ref().prepare_layout(ctx)
    }

    #[inline]
    fn retained_clip(&self, ctx: &BuildContext) -> Option<aimer_cupid::draw_cmd_v2::Rect> {
        self.as_ref().retained_clip(ctx)
    }

    #[inline]
    fn retained_v2_bounds(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        self.as_ref().retained_v2_bounds(ctx)
    }

    #[inline]
    fn retained_v2_paint_outsets(&self, ctx: &BuildContext) -> Option<[f32; 4]> {
        self.as_ref().retained_v2_paint_outsets(ctx)
    }

    #[inline]
    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        self.as_ref().retained_v2_child_context(ctx, child)
    }

    #[inline]
    fn retained_v2_child_context_at<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<BuildContext<'a>> {
        self.as_ref()
            .retained_v2_child_context_at(ctx, child, child_index)
    }

    #[inline]
    fn retained_v2_child_geometry(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.as_ref().retained_v2_child_geometry(ctx, child)
    }

    #[inline]
    fn retained_v2_child_geometry_at(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.as_ref()
            .retained_v2_child_geometry_at(ctx, child, child_index)
    }

    #[inline]
    fn local_v2_paint_needs_recording(&self, ctx: &BuildContext) -> bool {
        self.as_ref().local_v2_paint_needs_recording(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.as_ref().is_paint_bounded()
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.as_ref().compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref().draw_with_compositor_animation(ctx, frame)
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref()
            .update_compositor_animation_damage(ctx, frame)
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.as_ref().draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }
}

impl VisitorElement for Box<dyn Element> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }

    fn visit_retained_v2_children<'a>(
        &'a self,
        visitor: &mut dyn FnMut(usize, &'a dyn Element),
    ) {
        self.as_ref().visit_retained_v2_children(visitor)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref().element_type_id()
    }
    fn reconciliation_key(&self) -> Option<&Key> {
        self.as_ref().reconciliation_key()
    }
    fn element_id(&self) -> Option<ElementId> {
        self.as_ref().element_id()
    }
    fn set_element_id(&self, id: ElementId) {
        self.as_ref().set_element_id(id);
    }
}

impl LayoutElement for Box<dyn Element> {
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }
    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }
    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.as_ref().is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().event_tree_bounds()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        self.as_ref().adopt_runtime_state_from(old)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.as_ref().is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.as_ref().compositor_priority()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }

    fn subtree_generation(&self) -> u64 {
        self.as_ref().subtree_generation()
    }

    fn set_subtree_generation(&self, generation: u64) {
        self.as_ref().set_subtree_generation(generation)
    }
}

impl EventElement for Box<dyn Element> {
    fn event_tree_role(&self) -> EventTreeRole {
        self.as_ref().event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.as_ref().focus_node()
    }

    fn autofocus(&self) -> bool {
        self.as_ref().autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.as_ref().traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.as_ref().on_event_with_context(event, context)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().structural_children(visitor)
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().focus_children(visitor)
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children(visitor)
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children_reversed(visitor)
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at(pos, visitor)
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at_reversed(pos, visitor)
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.as_ref().has_overlapping_hit_targets()
    }

}

impl Drawable for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.as_ref().paint(ctx)
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.as_ref().sync_paint_geometry(ctx)
    }

    #[inline]
    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        self.as_ref().sync_local_v2_state(ctx)
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.as_ref().prepare_layout(ctx)
    }

    #[inline]
    fn retained_clip(&self, ctx: &BuildContext) -> Option<aimer_cupid::draw_cmd_v2::Rect> {
        self.as_ref().retained_clip(ctx)
    }

    #[inline]
    fn retained_v2_bounds(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        self.as_ref().retained_v2_bounds(ctx)
    }

    #[inline]
    fn retained_v2_paint_outsets(&self, ctx: &BuildContext) -> Option<[f32; 4]> {
        self.as_ref().retained_v2_paint_outsets(ctx)
    }

    #[inline]
    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        self.as_ref().retained_v2_child_context(ctx, child)
    }

    #[inline]
    fn retained_v2_child_context_at<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<BuildContext<'a>> {
        self.as_ref()
            .retained_v2_child_context_at(ctx, child, child_index)
    }

    #[inline]
    fn retained_v2_child_geometry(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.as_ref().retained_v2_child_geometry(ctx, child)
    }

    #[inline]
    fn retained_v2_child_geometry_at(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
        child_index: usize,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        self.as_ref()
            .retained_v2_child_geometry_at(ctx, child, child_index)
    }

    #[inline]
    fn local_v2_paint_needs_recording(&self, ctx: &BuildContext) -> bool {
        self.as_ref().local_v2_paint_needs_recording(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.as_ref().is_paint_bounded()
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.as_ref().compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref().draw_with_compositor_animation(ctx, frame)
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref()
            .update_compositor_animation_damage(ctx, frame)
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.as_ref().draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }
}
