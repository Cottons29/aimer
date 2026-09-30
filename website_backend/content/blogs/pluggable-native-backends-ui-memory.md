# Pluggable Native Rendering and the UI Heap

A UI framework meets two kinds of platform-specific work. It has to turn a frame into commands for a graphics API, and it has to create and retire many values as the widget tree changes. Aimer handles those jobs in separate layers: Cupid's renderer works through a backend contract, while the UI runtime can allocate framework-owned values from an application heap.

The framework keeps graphics API details within renderer implementations and centralizes selected CPU allocations in an application-owned heap.

## One renderer contract, multiple GPU APIs

The renderer in `aimer_cupid` is generic over `GpuBackend`. That public trait describes the operations the renderer needs: creating buffers and textures, building pipelines, opening render passes, copying resources, submitting commands, and reading hardware limits. It also defines associated types for the concrete resources supplied by each backend.

The renderer and its pipelines refer to backend-provided associated types for their concrete resources. Each backend maps Aimer's shared descriptors and operations to its own resource and command model.

| Target or build        | Backend selection                                      |
|------------------------|--------------------------------------------------------|
| macOS and iOS          | Metal                                                  |
| Windows                | Direct3D 12                                            |
| Linux and Android      | Vulkan by default; an OpenGL backend is also available |
| WebAssembly            | WebGPU by default; WebGL 2 is also available           |
| `wgpu` feature enabled | The `wgpu` backend becomes the generic default         |

Here, “pluggable” means the renderer can be composed with a different `GpuBackend` type at compile time. Target configuration selects the default backend, and code using the generic renderer can select an explicit backend type where that API is exposed.

The shared contract covers resource and command operations, while each backend still speaks its graphics API's language. Shader source is backend-specific: WGSL for WebGPU and `wgpu`, MSL for Metal, HLSL for Direct3D 12, and SPIR-V for Vulkan. Aimer provides hooks for backend-specific built-in shader sources. Applications that supply custom pipelines for multiple backends provide source each backend accepts.

The renderer shares higher-level pipeline logic, while each implementation maps commands to its API's native resources and submission rules.

## An application-owned heap for UI allocations

The widget tree also has a platform-independent resource problem: building and rebuilding an interface creates many temporary and retained values. `UiMemory` gives an application an owned heap for framework allocations that opt into it.

An `AimerApp` starts with an effectively unlimited UI heap. Applications can configure a maximum through the builder:

```rust
let app = AimerApp::new()
    .ui_memory_limit(64 * 1024 * 1024)
    .child(root_widget);
```

The heap starts without acquired pages and grows lazily in regions of up to 2 MiB, subject to the configured limit. `UiMemory::committed_bytes()` reports the bytes the pool has acquired, and `limit_bytes()` reports its configured maximum.

Underneath, a per-application `dlmalloc` heap obtains pages through a budget-aware page backend. Rubick also keeps a small cache of common allocation sizes in that heap, so a dropped erased value can leave reusable storage for a later value in the same application.

During a frame, Aimer runs widget drawing inside a scope for that application's `UiAllocator`. Framework-owned Rubick values consult the active allocator when they need heap storage. `Shared` allocations can use the same active allocator, and the cloneable `UiAllocator` also implements `allocator_api2::alloc::Allocator` for allocator-aware collections and boxes.

The scope identifies the allocator for new allocations. The allocation keeps enough ownership information to return its storage to the same heap when it is dropped, even after the scope has ended. The heap itself is confined to the UI thread, matching the single-threaded widget tree.

The configured maximum bounds this UI heap. The character buffer owned by a normal `String` continues to use Rust's usual allocator; allocator-aware collections such as `allocator_api2::vec::Vec` can use `UiAllocator` explicitly. GPU resources are managed by the selected graphics backend and driver. On Unix, the reported committed amount tracks mapped bytes, whose physical backing may be lazy; on WebAssembly it tracks the pool's high-water linear-memory usage, since grown linear memory cannot shrink.

## Two independent choices

The GPU backend controls how rendering resources and commands reach the device. The UI allocator controls where selected CPU-side framework values live and how their use is accounted for. Backend selection and the application's UI heap limit are independent configuration decisions.

Together, these mechanisms give Aimer a consistent rendering model across platform APIs and an application-level way to observe and bound a portion of UI memory. The widgets keep their declarative interface; the platform-specific work stays behind the renderer contract, and allocator-aware UI values can stay with the heap that created them.
