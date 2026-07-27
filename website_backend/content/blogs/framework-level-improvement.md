# Pointer Capture Performance Improvement

Pointer movement used to expose an expensive part of Aimer's event system. For every captured
`PointerMove` or `PointerUp`, the dispatcher searched the entire element tree to discover which
element owned the pointer. If no owner was found, normal hit testing traversed the tree again. On a
large interface, one input event could therefore visit almost every element twice.

We replaced that full-tree capture scan with a persistent, source-aware capture registry.

### Stable Element Identity

Every element now receives a monotonic `ElementId`. The ID represents the logical element rather
than its memory address, which is important because Aimer's inline `AnyElement` storage may move.

Generated trees reconcile these identities during rebuilds. Compatible keyed elements preserve
their IDs across reordering, while compatible unkeyed elements preserve them by structural position.
A genuine replacement receives a new ID, so a stale capture can never be delivered to an unrelated
element that happens to occupy the same slot.

### Persistent Capture Routing

`EventResult` can now explicitly request pointer capture or release. The application-level
`EventDispatcher` records the producing element and stores two indexes:

```text
PointerKey -> ElementId -> root-to-target path
```

Looking up a captured pointer is now average `O(1)`. Resolving its saved path costs only `O(depth)`,
independent of how many unrelated siblings exist elsewhere in the tree. `PointerKey` includes both
the pointer source and numeric ID, so mouse pointer `0` cannot collide with touch pointer `0`.

Capture state survives compatible widget rebuilds and is removed safely when the owner disappears,
the saved path becomes invalid, a pointer is released, or cancellation occurs. Pointer-up events are
delivered to the owner before capture is automatically cleared.

### Correct Across Widget Boundaries

The routing changes also cover widgets that manage events through custom boundaries. Scrollable
children remain protected by their viewport during ordinary hit testing, but a child that legitimately
captures a pointer continues receiving move and up events outside that viewport. Modal content,
delegated SVG elements, gestures, and text selection use the same explicit ownership model.

### What This Improves

Captured drags no longer become slower as the overall interface grows. The dispatcher performs one
hash-map lookup and visits only the saved route instead of repeatedly scanning the complete tree.
We also removed duplicate generated-tree delivery and reuse one traversal scratch buffer, reducing
overhead in the uncaptured path.

The first uncaptured hit test is still a tree traversal—stable identity does not turn arbitrary
overlapping UI hit testing into constant-time work. This improvement targets the hot path where the
destination is already known: dragging, selection, scrolling, and other captured interactions.

> Should I need to set up the database for upload the post ?