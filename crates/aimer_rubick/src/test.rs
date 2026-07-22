
#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::mem::{align_of, size_of};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;
    use crate::{Rubick, INLINE_ALIGNMENT, INLINE_CAPACITY};

    trait Value {
        fn value(&self) -> usize;
        fn set_value(&mut self, value: usize);
    }

    #[derive(Debug)]
    struct ExactBoundary([u8; INLINE_CAPACITY]);

    impl Value for ExactBoundary {
        fn value(&self) -> usize {
            usize::from(self.0[0])
        }

        fn set_value(&mut self, value: usize) {
            self.0[0] = value as u8;
        }
    }

    struct Oversized([u8; INLINE_CAPACITY + 1]);

    impl Value for Oversized {
        fn value(&self) -> usize {
            usize::from(self.0[0])
        }

        fn set_value(&mut self, value: usize) {
            self.0[0] = value as u8;
        }
    }

    #[repr(align(32))]
    struct OverAligned(u8);

    #[repr(C, align(16))]
    struct MaximallyAligned([u8; INLINE_CAPACITY]);

    #[repr(align(32))]
    struct OverAlignedZeroSized;

    struct Envelope {
        prefix: usize,
        value: usize,
        suffix: usize,
    }

    fn project_envelope_value(value: &Envelope) -> &usize {
        assert_eq!(value.prefix, 0xA1);
        assert_eq!(value.suffix, 0xB2);
        &value.value
    }

    fn project_envelope_value_mut(value: &mut Envelope) -> &mut usize {
        assert_eq!(value.prefix, 0xA1);
        assert_eq!(value.suffix, 0xB2);
        &mut value.value
    }

    fn project_value<U: Value + 'static>(value: &U) -> &(dyn Value + 'static) {
        value
    }

    fn project_value_mut<U: Value + 'static>(value: &mut U) -> &mut (dyn Value + 'static) {
        value
    }

    #[test]
    fn fixed_layout_matches_representative_aimer_values() {
        assert_eq!(INLINE_CAPACITY, 4 * size_of::<usize>());
        assert_eq!(INLINE_ALIGNMENT, 16);
        assert!(INLINE_CAPACITY / size_of::<usize>() < 8);
    }

    #[test]
    fn sized_values_dereference_and_mutate() {
        let mut value = Rubick::new(String::from("Aimer"));
        value.push_str(" GUI");

        assert_eq!(&*value, "Aimer GUI");
        assert_eq!(value.as_ref(), "Aimer GUI");
    }

    #[test]
    fn storage_selection_checks_size_alignment_and_zero_sized_values() {
        assert!(Rubick::new(()).is_inline());
        assert!(Rubick::new(ExactBoundary([0; INLINE_CAPACITY])).is_inline());
        assert!(Rubick::new(Oversized([0; INLINE_CAPACITY + 1])).is_heap());
        let over_aligned = Rubick::new(OverAligned(7));
        assert!(over_aligned.is_heap());
        assert_eq!(over_aligned.0, 7);
        assert!(align_of::<OverAligned>() > INLINE_ALIGNMENT);
    }

    #[test]
    fn storage_selection_handles_both_alignment_boundaries() {
        let maximally_aligned = Rubick::new(MaximallyAligned([7; INLINE_CAPACITY]));
        let over_aligned_zero_sized = Rubick::new(OverAlignedZeroSized);

        assert_eq!(align_of::<MaximallyAligned>(), INLINE_ALIGNMENT);
        assert_eq!(maximally_aligned.0[INLINE_CAPACITY - 1], 7);
        assert!(maximally_aligned.is_inline());

        assert_eq!(size_of::<OverAlignedZeroSized>(), 0);
        assert!(align_of::<OverAlignedZeroSized>() > INLINE_ALIGNMENT);
        assert!(over_aligned_zero_sized.is_heap());
    }

    #[test]
    fn projected_trait_values_dispatch_inline_and_on_heap() {
        let mut inline: Rubick<dyn Value> = Rubick::new_projected(
            ExactBoundary([3; INLINE_CAPACITY]),
            project_value,
            project_value_mut,
        );
        let mut heap: Rubick<dyn Value> = Rubick::new_projected(
            Oversized([5; INLINE_CAPACITY + 1]),
            project_value,
            project_value_mut,
        );

        inline.set_value(11);
        heap.set_value(13);

        assert!(inline.is_inline());
        assert!(heap.is_heap());
        assert_eq!(inline.value(), 11);
        assert_eq!(heap.value(), 13);
    }

    #[test]
    fn moves_swaps_and_collection_growth_rebuild_projection() {
        let first: Rubick<dyn Value> = Rubick::new_projected(
            ExactBoundary([1; INLINE_CAPACITY]),
            project_value,
            project_value_mut,
        );
        let second: Rubick<dyn Value> = Rubick::new_projected(
            Oversized([2; INLINE_CAPACITY + 1]),
            project_value,
            project_value_mut,
        );
        let mut values = Vec::with_capacity(1);
        values.extend([first, second]);
        values.swap(0, 1);

        assert_eq!(values[0].value(), 2);
        assert_eq!(values[1].value(), 1);
    }

    #[test]
    fn repeated_relocation_preserves_a_projection_to_an_interior_field() {
        let owner: Rubick<usize> = Rubick::new_projected(
            Envelope {
                prefix: 0xA1,
                value: 7,
                suffix: 0xB2,
            },
            project_envelope_value,
            project_envelope_value_mut,
        );
        let mut owners = Vec::with_capacity(1);
        owners.push(owner);

        for expected in 8..=256 {
            owners.push(Rubick::new(expected));
            let last = owners.len() - 1;
            owners.swap(0, last);
            owners.swap(0, last);
            *owners[0] += 1;
            assert_eq!(*owners[0], expected);
        }
    }

    #[test]
    fn captured_projection_adapters_keep_state_across_owner_moves() {
        let shared_calls = Rc::new(Cell::new(0));
        let mutable_calls = Rc::new(Cell::new(0));
        let mut owner: Rubick<usize> = Rubick::new_projected(
            10_usize,
            {
                let shared_calls = Rc::clone(&shared_calls);
                move |value: &usize| {
                    shared_calls.set(shared_calls.get() + 1);
                    value
                }
            },
            {
                let mutable_calls = Rc::clone(&mutable_calls);
                move |value: &mut usize| {
                    mutable_calls.set(mutable_calls.get() + 1);
                    value
                }
            },
        );

        owner = std::hint::black_box(owner);
        assert_eq!(*owner, 10);
        *owner = 12;
        assert_eq!(*owner, 12);
        assert_eq!(shared_calls.get(), 2);
        assert_eq!(mutable_calls.get(), 1);
    }

    #[test]
    fn panicking_projection_does_not_corrupt_the_owned_value() {
        let should_panic = Rc::new(Cell::new(true));
        let mut owner: Rubick<usize> = Rubick::new_projected(
            41_usize,
            {
                let should_panic = Rc::clone(&should_panic);
                move |value: &usize| {
                    assert!(!should_panic.replace(false), "project once with a panic");
                    value
                }
            },
            |value: &mut usize| value,
        );

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = *owner;
        }));
        assert!(result.is_err());

        *owner += 1;
        assert_eq!(*owner, 42);
    }

    struct DropValue<const N: usize> {
        drops: Rc<Cell<usize>>,
        bytes: [u8; N],
    }

    impl<const N: usize> Drop for DropValue<N> {
        fn drop(&mut self) {
            self.drops
                .set(self.drops.get() + 1);
        }
    }

    trait DropTarget {}

    impl<const N: usize> DropTarget for DropValue<N> {}

    fn project_drop_target<U: DropTarget + 'static>(value: &U) -> &(dyn DropTarget + 'static) {
        value
    }

    fn project_drop_target_mut<U: DropTarget + 'static>(
        value: &mut U,
    ) -> &mut (dyn DropTarget + 'static) {
        value
    }

    #[test]
    fn values_drop_exactly_once_in_both_modes() {
        let inline_drops = Rc::new(Cell::new(0));
        let heap_drops = Rc::new(Cell::new(0));
        {
            let _inline = Rubick::new(DropValue::<0> {
                drops: Rc::clone(&inline_drops),
                bytes: [],
            });
            let _heap = Rubick::new(DropValue::<INLINE_CAPACITY> {
                drops: Rc::clone(&heap_drops),
                bytes: [0; INLINE_CAPACITY],
            });
        }

        assert_eq!(inline_drops.get(), 1);
        assert_eq!(heap_drops.get(), 1);
    }

    #[test]
    fn replacing_an_owner_drops_each_value_once() {
        let first_drops = Rc::new(Cell::new(0));
        let second_drops = Rc::new(Cell::new(0));
        let mut owner = Rubick::new(DropValue::<0> {
            drops: Rc::clone(&first_drops),
            bytes: [],
        });

        let old = std::mem::replace(
            &mut owner,
            Rubick::new(DropValue::<0> {
                drops: Rc::clone(&second_drops),
                bytes: [],
            }),
        );
        drop(old);
        assert_eq!(first_drops.get(), 1);
        assert_eq!(second_drops.get(), 0);

        drop(owner);
        assert_eq!(second_drops.get(), 1);
    }

    #[test]
    fn values_drop_during_panic_unwinding() {
        let inline_drops = Rc::new(Cell::new(0));
        let heap_drops = Rc::new(Cell::new(0));
        let result = catch_unwind(AssertUnwindSafe({
            let inline_drops = Rc::clone(&inline_drops);
            let heap_drops = Rc::clone(&heap_drops);
            move || {
                let _inline = Rubick::new(DropValue::<0> {
                    drops: inline_drops,
                    bytes: [],
                });
                let _heap = Rubick::new(DropValue::<INLINE_CAPACITY> {
                    drops: heap_drops,
                    bytes: [0; INLINE_CAPACITY],
                });
                panic!("exercise unwind drop");
            }
        }));

        assert!(result.is_err());
        assert_eq!(inline_drops.get(), 1);
        assert_eq!(heap_drops.get(), 1);
    }

    #[test]
    fn nested_and_mixed_mode_owners_drop_every_value_once() {
        const OWNER_COUNT: usize = 128;

        let drops = Rc::new(Cell::new(0));
        let mut owners = Vec::with_capacity(1);
        for index in 0..OWNER_COUNT {
            let owner: Rubick<dyn DropTarget> = if index % 2 == 0 {
                Rubick::new_projected(
                    DropValue::<0> {
                        drops: Rc::clone(&drops),
                        bytes: [],
                    },
                    project_drop_target,
                    project_drop_target_mut,
                )
            } else {
                Rubick::new_projected(
                    DropValue::<INLINE_CAPACITY> {
                        drops: Rc::clone(&drops),
                        bytes: [0; INLINE_CAPACITY],
                    },
                    project_drop_target,
                    project_drop_target_mut,
                )
            };
            if index % 2 == 0 {
                assert!(owner.is_inline());
            } else {
                assert!(owner.is_heap());
            }
            owners.push(owner);
        }
        owners.reverse();
        owners.rotate_left(37);
        drop(owners);
        assert_eq!(drops.get(), OWNER_COUNT);

        let nested_drops = Rc::new(Cell::new(0));
        let inner = Rubick::new(DropValue::<0> {
            drops: Rc::clone(&nested_drops),
            bytes: [],
        });
        let outer = Rubick::new(inner);
        assert!(outer.is_heap());
        drop(outer);
        assert_eq!(nested_drops.get(), 1);
    }

    #[test]
    fn owner_layout_and_unpin_contract_are_fixed() {
        fn assert_unpin<T: Unpin>() {}

        assert_unpin::<Rubick<u32>>();
        assert!(size_of::<Rubick<u32>>() >= INLINE_CAPACITY);
        assert!(align_of::<Rubick<u32>>() >= INLINE_ALIGNMENT);
    }

    #[test]
    fn owners_are_conservatively_not_send_or_sync() {
        trait AmbiguousIfSend<A> {
            fn check() {}
        }

        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

        trait AmbiguousIfSync<A> {
            fn check() {}
        }

        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

        <Rubick<u32> as AmbiguousIfSend<_>>::check();
        <Rubick<u32> as AmbiguousIfSync<_>>::check();
    }

    #[test]
    fn test_fixture_uses_heap_payload_bytes() {
        let value = DropValue::<3> {
            drops: Rc::new(Cell::new(0)),
            bytes: [1, 2, 3],
        };
        assert_eq!(value.bytes.len(), 3);
    }
}
