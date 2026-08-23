use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use quote::{format_ident, quote};
use sha2::{Digest, Sha256};
use syn::visit::{self, Visit};
use syn::{Item, ItemFn};

use super::{RootDiscovery, ShadowError, ShadowErrorKind, SourceType, fingerprint_source};

const FACTORY: &str = "__aimer_generated_root_factory";

pub(crate) fn generate(
    source_root: &Path,
    shadow_root: &Path,
    package: &str,
    discovery: &RootDiscovery,
    source_types: &[SourceType],
) -> Result<(), ShadowError> {
    let owner_source = shadow_path(source_root, shadow_root, discovery.root_span().path())?;
    let owner_identity = discovery.call_path().last().expect("root flow has an owner");
    inject_factory(&owner_source, owner_identity.type_name(), discovery.root_expression())?;
    remove_traced_startup(source_root, shadow_root, discovery)?;

    let crate_root = shadow_path(source_root, shadow_root, discovery.crate_source())?;
    let mut syntax = parse_file(&crate_root)?;
    let generated = generated_items(
        root_factory_path(owner_identity.module())?,
        source_fingerprint(discovery.root_expression())?,
        stable_id("aimer.guest.application.v1", package),
        stable_id("aimer.guest.state.v1", package),
        state_schema_id(package, source_types)?,
    )?;
    syntax.items.extend(generated);
    write_syntax(&crate_root, syntax)
}

fn inject_factory(path: &Path, owner: &str, expression: &str) -> Result<(), ShadowError> {
    let mut syntax = parse_file(path)?;
    let root = syn::parse_str::<syn::Expr>(expression).map_err(|error| {
        ShadowError::new(ShadowErrorKind::MalformedSource, format!("failed to parse discovered root expression: {error}"))
    })?;
    let mut inserted = false;
    insert_factory_in_items(&mut syntax.items, owner, &root, &mut inserted)?;
    if !inserted {
        return Err(ShadowError::new(
            ShadowErrorKind::UnresolvedFlow,
            format!("failed to locate root-owning function `{owner}` in copied source"),
        ));
    }
    write_syntax(path, syntax)
}

fn insert_factory_in_items(
    items: &mut Vec<Item>,
    owner: &str,
    root: &syn::Expr,
    inserted: &mut bool,
) -> Result<(), ShadowError> {
    let mut index = 0;
    while index < items.len() {
        match &mut items[index] {
            Item::Fn(function) if function.sig.ident == owner && owns_root(function, root) => {
                if *inserted {
                    return Err(ShadowError::new(
                        ShadowErrorKind::AmbiguousFlow,
                        format!("multiple copied functions match root owner `{owner}`"),
                    ));
                }
                let factory = format_ident!("{FACTORY}");
                let item = syn::parse2::<Item>(quote! {
                    #[doc(hidden)]
                    pub fn #factory() -> impl ::aimer::Widget { #root }
                }).expect("generated root factory is valid Rust");
                items[index] = item;
                *inserted = true;
            }
            Item::Mod(module) => {
                if let Some((_, nested)) = &mut module.content {
                    insert_factory_in_items(nested, owner, root, inserted)?;
                }
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

fn owns_root(function: &ItemFn, root: &syn::Expr) -> bool {
    let mut visitor = RootCallVisitor {
        root: quote!(#root).to_string(),
        found: false,
    };
    visitor.visit_block(&function.block);
    visitor.found
}

struct RootCallVisitor {
    root: String,
    found: bool,
}

impl<'ast> Visit<'ast> for RootCallVisitor {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "child"
            && call.args.len() == 1
            && call.args.first().is_some_and(|argument| quote!(#argument).to_string() == self.root)
        {
            self.found = true;
        }
        if !self.found {
            visit::visit_expr_method_call(self, call);
        }
    }
}

fn remove_traced_startup(
    source_root: &Path,
    shadow_root: &Path,
    discovery: &RootDiscovery,
) -> Result<(), ShadowError> {
    let mut files = BTreeMap::<PathBuf, (String, BTreeSet<(String, String)>)>::new();
    for startup in discovery.startup_functions() {
        let path = shadow_path(source_root, shadow_root, &startup.source)?;
        let (file_module, functions) = files
            .entry(path)
            .or_insert_with(|| (startup.file_module.clone(), BTreeSet::new()));
        debug_assert_eq!(file_module, &startup.file_module);
        functions.insert((startup.identity.module().to_owned(), startup.identity.type_name().to_owned()));
    }
    for (path, (file_module, functions)) in files {
        let mut syntax = parse_file(&path)?;
        let mut module = file_module.split("::").map(str::to_owned).collect();
        remove_traced_items(&mut syntax.items, &mut module, &functions);
        write_syntax(&path, syntax)?;
    }
    Ok(())
}

fn remove_traced_items(
    items: &mut Vec<Item>,
    module: &mut Vec<String>,
    functions: &BTreeSet<(String, String)>,
) {
    let mut retained = Vec::with_capacity(items.len());
    for mut item in items.drain(..) {
        if matches!(&item, Item::Fn(function) if functions.contains(&(module.join("::"), function.sig.ident.to_string()))) {
            continue;
        }
        if let Item::Mod(item_mod) = &mut item
            && let Some((_, nested)) = &mut item_mod.content
        {
            module.push(item_mod.ident.to_string());
            remove_traced_items(nested, module, functions);
            module.pop();
        }
        retained.push(item);
    }
    *items = retained;
}

fn root_factory_path(module: &str) -> Result<syn::Path, ShadowError> {
    let mut path = String::from("crate");
    for segment in module.split("::").skip_while(|segment| *segment == "crate") {
        path.push_str("::");
        path.push_str(segment);
    }
    path.push_str("::");
    path.push_str(FACTORY);
    syn::parse_str(&path).map_err(|error| {
        ShadowError::new(ShadowErrorKind::MalformedSource, format!("invalid generated root factory path `{path}`: {error}"))
    })
}

fn generated_items(
    root_path: syn::Path,
    source_id: [u8; 16],
    application_id: [u8; 16],
    state_id: [u8; 16],
    schema_id: [u8; 16],
) -> Result<Vec<Item>, ShadowError> {
    let source = byte_array(source_id);
    let application = byte_array(application_id);
    let state = byte_array(state_id);
    let schema = byte_array(schema_id);
    let tokens = quote! {
        #[doc(hidden)]
        pub const __AIMER_GENERATED_GUEST_LIMITS: ::aimer_wasm_guest::GuestLimits =
            ::aimer_wasm_guest::GuestLimits::new(
                ::aimer::anteros::ModelLimits::new(16_777_216, 65_536, 1_048_576, 16_777_216),
                64, 16_777_216, 16,
            );
        const __AIMER_GENERATED_WIDGET_LIMITS: ::aimer::portable::PortableWidgetLimits =
            ::aimer::portable::PortableWidgetLimits::new(65_536, 262_144, 65_536, 65_536, 1_048_576, 16_777_216)
                .with_max_blob_bytes(16_777_216);
        const __AIMER_GENERATED_STATE_LIMITS: ::aimer::portable::PortableLimits =
            ::aimer::portable::PortableLimits::new(256, 65_536, 1_048_576, 65_536, 16_777_216);
        const __AIMER_GENERATED_APPLICATION_ID: ::aimer::anteros::StableId128 =
            ::aimer::anteros::StableId128::from_bytes(#application);
        const __AIMER_GENERATED_STATE_ID: ::aimer::anteros::StableId128 =
            ::aimer::anteros::StableId128::from_bytes(#state);
        const __AIMER_GENERATED_STATE_SCHEMA_ID: ::aimer::anteros::StableId128 =
            ::aimer::anteros::StableId128::from_bytes(#schema);

        ::std::thread_local! {
            static __AIMER_GENERATED_BUILD_CONTEXT: ::std::cell::RefCell<Option<::aimer::portable::PortableBuildContext>> =
                const { ::std::cell::RefCell::new(None) };
        }

        #[doc(hidden)]
        #[derive(Default)]
        pub struct __AimerGeneratedGuestProgram {
            generation_id: u64,
            built: bool,
        }

        fn __aimer_generated_error(status: ::aimer::anteros::AbiStatus) -> ::aimer_wasm_guest::GuestError {
            ::aimer_wasm_guest::GuestError::new(status)
        }

        fn __aimer_generated_map_build_error(
            error: ::aimer::portable::PortableBuildError,
        ) -> ::aimer_wasm_guest::GuestError {
            use ::aimer::portable::{PortableBuildError, PortableCallbackError};
            let error = match error {
                PortableBuildError::Model(error) => {
                    return ::aimer_wasm_guest::GuestError::from_model(error);
                }
                error => error,
            };
            let status = match &error {
                PortableBuildError::State(_) => ::aimer::anteros::AbiStatus::StateIncompatible,
                PortableBuildError::LimitExceeded { .. } | PortableBuildError::LengthOverflow { .. } =>
                    ::aimer::anteros::AbiStatus::ResourceExhausted,
                PortableBuildError::Callback(PortableCallbackError::Duplicate { .. }) =>
                    ::aimer::anteros::AbiStatus::DuplicateId,
                PortableBuildError::Callback(PortableCallbackError::Unknown { .. }) =>
                    ::aimer::anteros::AbiStatus::UnknownId,
                PortableBuildError::Callback(PortableCallbackError::Retired) =>
                    ::aimer::anteros::AbiStatus::RetiredGeneration,
                _ => ::aimer::anteros::AbiStatus::ApplicationError,
            };
            let diagnostic = error.into_guest_diagnostic();
            ::aimer_wasm_guest::GuestError::with_diagnostic(status, diagnostic)
        }

        fn __aimer_generated_async_failure(
            failure: ::aimer::portable::PortableAsyncFailure,
        ) -> ::aimer_wasm_guest::GuestError {
            let message = format!(
                "async callback task {} for callback {:02x?} failed: {}",
                failure.task_id().value(),
                failure.callback_id().to_bytes(),
                failure.message(),
            );
            ::aimer_wasm_guest::GuestError::with_diagnostic(
                ::aimer::anteros::AbiStatus::ApplicationError,
                ::aimer::anteros::GuestDiagnostic::new(
                    ::aimer::anteros::GuestOperation::Build,
                    ::aimer::anteros::GuestDiagnosticCategory::Callback,
                    message,
                ),
            )
        }

        fn __aimer_generated_build(
            context: &mut ::aimer::portable::PortableBuildContext,
        ) -> Result<Vec<u8>, ::aimer_wasm_guest::GuestError> {
            context.run_async_microtasks();
            if let Some(failure) = context.take_async_failure() {
                return Err(__aimer_generated_async_failure(failure));
            }
            context.apply_queued_mutations().map_err(__aimer_generated_map_build_error)?;
            let root = #root_path();
            let source = ::aimer::portable::SourceFingerprint::new(
                ::aimer::portable::StableId128::from_bytes(#source),
            );
            let node = ::aimer::PortableWidget::to_portable_node(
                root,
                context,
                source,
            ).map_err(__aimer_generated_map_build_error)?;
            let document = context.finish_document(node).map_err(__aimer_generated_map_build_error)?;
            document.encode().map_err(__aimer_generated_map_build_error)
        }

        impl ::aimer_wasm_guest::GuestProgram for __AimerGeneratedGuestProgram {
            fn manifest(&self, limits: ::aimer::anteros::ModelLimits) -> Result<Vec<u8>, ::aimer_wasm_guest::GuestError> {
                ::aimer::anteros::ApplicationManifest::new(
                    ::aimer::anteros::AbiVersion::new(1, 0),
                    ::aimer::anteros::AbiVersion::new(1, 0),
                    ::aimer::anteros::WIDGET_IR_FORMAT_VERSION,
                    ::aimer::anteros::CALLBACK_EVENT_FORMAT_VERSION,
                    ::aimer::anteros::STATE_FORMAT_VERSION,
                    __AIMER_GENERATED_APPLICATION_ID,
                    &[],
                ).encode(limits).map_err(::aimer_wasm_guest::GuestError::from_model)
            }

            fn initialize(&mut self, generation_id: u64) -> Result<(), ::aimer_wasm_guest::GuestError> {
                if generation_id == 0 {
                    return Err(__aimer_generated_error(::aimer::anteros::AbiStatus::InvalidArgument));
                }
                let context = ::aimer::portable::PortableBuildContext::new(
                    generation_id, 0, __AIMER_GENERATED_WIDGET_LIMITS, __AIMER_GENERATED_STATE_LIMITS,
                ).map_err(__aimer_generated_map_build_error)?;
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
                self.generation_id = generation_id;
                self.built = false;
                Ok(())
            }

            fn build(&mut self, _limits: ::aimer::anteros::ModelLimits) -> Result<Vec<u8>, ::aimer_wasm_guest::GuestError> {
                let output = __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let mut slot = slot.try_borrow_mut().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    __aimer_generated_build(slot.as_mut().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?)
                })?;
                self.built = true;
                Ok(output)
            }

            fn dispatch_event(
                &mut self,
                event: &::aimer::anteros::CallbackEventView<'_>,
                _limits: ::aimer::anteros::ModelLimits,
            ) -> Result<Option<Vec<u8>>, ::aimer_wasm_guest::GuestError> {
                if event.generation_id() != self.generation_id {
                    return Err(__aimer_generated_error(::aimer::anteros::AbiStatus::RetiredGeneration));
                }
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let mut slot = slot.try_borrow_mut().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    let context = slot.as_mut().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?;
                    let callback_id = ::aimer::portable::StableId128::from_bytes(*event.callback_id().as_bytes());
                    context.callback_registry().dispatch_start(callback_id, context).map_err(|error| {
                        __aimer_generated_map_build_error(::aimer::portable::PortableBuildError::Callback(error))
                    })?;
                    if context.take_rebuild_request() {
                        __aimer_generated_build(context).map(Some)
                    } else {
                        Ok(None)
                    }
                })
            }

            fn poll_async(
                &mut self,
                _limits: ::aimer::anteros::ModelLimits,
            ) -> Result<Option<Vec<u8>>, ::aimer_wasm_guest::GuestError> {
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let mut slot = slot.try_borrow_mut().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    let context = slot.as_mut().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?;
                    context.run_async_microtasks();
                    if context.take_rebuild_request() {
                        __aimer_generated_build(context).map(Some)
                    } else {
                        Ok(None)
                    }
                })
            }

            fn has_async_work(&self) -> bool {
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .is_some_and(::aimer::portable::PortableBuildContext::has_async_work)
                })
            }

            fn dispatch_async_event(
                &mut self,
                event: &::aimer::anteros::AsyncCallbackEventView<'_>,
                _limits: ::aimer::anteros::ModelLimits,
            ) -> Result<Option<Vec<u8>>, ::aimer_wasm_guest::GuestError> {
                if event.generation_id() != self.generation_id {
                    return Err(__aimer_generated_error(::aimer::anteros::AbiStatus::RetiredGeneration));
                }
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let mut slot = slot.try_borrow_mut().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    let context = slot.as_mut().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?;
                    context.dispatch_external_async_event(event).map_err(|error| {
                        __aimer_generated_map_build_error(::aimer::portable::PortableBuildError::AsyncEvent(error))
                    })?;
                    if context.take_rebuild_request() {
                        __aimer_generated_build(context).map(Some)
                    } else {
                        Ok(None)
                    }
                })
            }

            fn export_state(&self, limits: ::aimer::anteros::ModelLimits) -> Result<Vec<u8>, ::aimer_wasm_guest::GuestError> {
                let payload = __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let slot = slot.try_borrow().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    slot.as_ref().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?.state_registry().export().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible)
                    })
                })?;
                let entries = [::aimer::anteros::StateEntry::new(
                    __AIMER_GENERATED_STATE_ID,
                    __AIMER_GENERATED_STATE_SCHEMA_ID,
                    ::aimer::anteros::Version::new(1, 0),
                    ::aimer::anteros::StatePolicy::Required,
                    &payload,
                )];
                ::aimer::anteros::StateBundle::new(
                    __AIMER_GENERATED_APPLICATION_ID, self.generation_id, &entries,
                ).encode(limits).map_err(::aimer_wasm_guest::GuestError::from_model)
            }

            fn import_state(&mut self, state: &::aimer::anteros::StateBundleView<'_>) -> Result<(), ::aimer_wasm_guest::GuestError> {
                if state.application_id() != __AIMER_GENERATED_APPLICATION_ID || state.entry_count() != 1 {
                    return Err(__aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible));
                }
                let entry = state.entry(0).ok_or_else(|| {
                    __aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible)
                })?;
                if entry.state_id() != __AIMER_GENERATED_STATE_ID
                    || entry.schema_id() != __AIMER_GENERATED_STATE_SCHEMA_ID
                    || entry.schema_version() != ::aimer::anteros::Version::new(1, 0)
                    || entry.policy() != ::aimer::anteros::StatePolicy::Required
                {
                    return Err(__aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible));
                }
                if !self.built {
                    let _ = <Self as ::aimer_wasm_guest::GuestProgram>::build(
                        self,
                        ::aimer::anteros::ModelLimits::new(16_777_216, 65_536, 1_048_576, 16_777_216),
                    )?;
                }
                __AIMER_GENERATED_BUILD_CONTEXT.with(|slot| {
                    let mut slot = slot.try_borrow_mut().map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::ApplicationError)
                    })?;
                    slot.as_mut().ok_or_else(|| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::NotActive)
                    })?.state_registry_mut().import(entry.payload()).map_err(|_| {
                        __aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible)
                    })
                })
            }

            fn migrate_state(
                &mut self,
                _state: &::aimer::anteros::StateBundleView<'_>,
                _limits: ::aimer::anteros::ModelLimits,
            ) -> Result<Vec<u8>, ::aimer_wasm_guest::GuestError> {
                Err(__aimer_generated_error(::aimer::anteros::AbiStatus::StateIncompatible))
            }
        }

        ::aimer_wasm_guest::export_guest!(__AimerGeneratedGuestProgram, __AIMER_GENERATED_GUEST_LIMITS);
    };
    syn::parse2::<syn::File>(tokens).map(|file| file.items).map_err(|error| {
        ShadowError::new(ShadowErrorKind::MalformedSource, format!("failed to parse generated guest adapter: {error}"))
    })
}

fn state_schema_id(package: &str, source_types: &[SourceType]) -> Result<[u8; 16], ShadowError> {
    let mut hasher = Sha256::new();
    hasher.update(b"aimer.guest.state-schema.v1\0");
    hasher.update(package.as_bytes());
    let mut files = BTreeSet::new();
    for source_type in source_types {
        hasher.update([0]);
        hasher.update(source_type.identity().portable_name().as_bytes());
        files.insert(source_type.span().path());
    }
    let mut declarations = Vec::new();
    for path in files {
        let source = fs::read_to_string(path).map_err(|error| {
            ShadowError::new(
                ShadowErrorKind::Io,
                format!("failed to read state schema source {}: {error}", path.display()),
            )
        })?;
        let syntax = syn::parse_file(&source).map_err(|error| {
            ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!("failed to parse state schema source {}: {error}", path.display()),
            )
        })?;
        collect_struct_declarations(&syntax.items, &mut declarations);
    }
    declarations.sort_unstable();
    for declaration in declarations {
        hasher.update([0]);
        hasher.update(declaration.as_bytes());
    }
    Ok(first_128(hasher.finalize()))
}

fn collect_struct_declarations(items: &[Item], declarations: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Struct(item_struct) => declarations.push(quote!(#item_struct).to_string()),
            Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    collect_struct_declarations(nested, declarations);
                }
            }
            _ => {}
        }
    }
}

fn source_fingerprint(expression: &str) -> Result<[u8; 16], ShadowError> {
    let fingerprint = fingerprint_source(expression, None)?;
    let bytes = hex::decode(fingerprint.as_str()).expect("SHA-256 fingerprint is hex");
    Ok(bytes[..16].try_into().expect("SHA-256 has sixteen bytes"))
}

fn stable_id(domain: &str, value: &str) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    first_128(hasher.finalize())
}

fn first_128(bytes: impl AsRef<[u8]>) -> [u8; 16] {
    bytes.as_ref()[..16].try_into().expect("digest has sixteen bytes")
}

fn byte_array(bytes: [u8; 16]) -> syn::ExprArray {
    let values = bytes.map(|byte| byte.to_string()).join(",");
    syn::parse_str(&format!("[{values}]")).expect("generated byte array is valid Rust")
}

fn shadow_path(source_root: &Path, shadow_root: &Path, source: &Path) -> Result<PathBuf, ShadowError> {
    let relative = source.strip_prefix(source_root).map_err(|_| {
        ShadowError::new(ShadowErrorKind::PathEscape, format!("generated source escapes application: {}", source.display()))
    })?;
    Ok(shadow_root.join(relative))
}

fn parse_file(path: &Path) -> Result<syn::File, ShadowError> {
    let source = fs::read_to_string(path).map_err(|error| source_io(path, error))?;
    syn::parse_file(&source).map_err(|error| {
        ShadowError::new(ShadowErrorKind::MalformedSource, format!("failed to parse copied source {}: {error}", path.display()))
    })
}

fn write_syntax(path: &Path, syntax: syn::File) -> Result<(), ShadowError> {
    let mut source = quote!(#syntax).to_string();
    source.push('\n');
    fs::write(path, source).map_err(|error| source_io(path, error))
}

fn source_io(path: &Path, error: std::io::Error) -> ShadowError {
    ShadowError::new(ShadowErrorKind::Io, format!("failed to access copied source {}: {error}", path.display()))
}
