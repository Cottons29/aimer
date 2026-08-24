use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use quote::{ToTokens, format_ident, quote};
use sha2::{Digest, Sha256};
use syn::visit::{self, Visit};

use super::{ShadowError, ShadowErrorKind, discovery};

pub(crate) fn transform(
    root: &Path,
    package: &str,
    manifest: &toml::Value,
) -> Result<(), ShadowError> {
    let roots = discovery::source_roots(root, manifest)?;
    let mut model = Model::new(root, package, runtime_path(manifest)?);
    for source_root in &roots {
        model.collect_file(source_root, vec!["crate".to_owned()])?;
    }
    let files = model.files.clone();
    for (file, module) in files {
        model.transform_file(&file, &module)?;
    }
    Ok(())
}

#[derive(Clone)]
struct StructInfo {
    module: Vec<String>,
    fields: Vec<syn::Field>,
    adopted: BTreeSet<String>,
}

#[derive(Clone)]
struct EnumInfo {
    unit: bool,
    portable_value: bool,
}

struct Model<'a> {
    root: &'a Path,
    package: &'a str,
    runtime: syn::Path,
    files: BTreeMap<PathBuf, Vec<String>>,
    structs: BTreeMap<String, StructInfo>,
    enums: BTreeMap<String, EnumInfo>,
    aliases: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

impl<'a> Model<'a> {
    fn new(root: &'a Path, package: &'a str, runtime: syn::Path) -> Self {
        Self {
            root,
            package,
            runtime,
            files: BTreeMap::new(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            aliases: BTreeMap::new(),
        }
    }

    fn collect_file(&mut self, file: &Path, module: Vec<String>) -> Result<(), ShadowError> {
        let canonical = fs::canonicalize(file).map_err(|error| io_error("resolve", file, error))?;
        if !canonical.starts_with(self.root) {
            return Err(ShadowError::new(
                ShadowErrorKind::PathEscape,
                format!("module escapes shadow root: {}", file.display()),
            ));
        }
        if self.files.insert(canonical.clone(), module.clone()).is_some() {
            return Ok(());
        }
        let source = fs::read_to_string(&canonical)
            .map_err(|error| io_error("read", &canonical, error))?;
        let syntax = syn::parse_file(&source).map_err(|error| malformed(&canonical, error))?;
        let directory = module_directory(&canonical);
        self.collect_items(&syntax.items, module, &canonical, directory)
    }

    fn collect_items(
        &mut self,
        items: &[syn::Item],
        module: Vec<String>,
        file: &Path,
        directory: PathBuf,
    ) -> Result<(), ShadowError> {
        let module_key = module.join("::");
        let mut module_aliases = BTreeMap::new();
        for item in items {
            if let syn::Item::Use(item_use) = item {
                collect_use_aliases(&item_use.tree, Vec::new(), &module, &mut module_aliases);
            }
        }
        self.aliases.insert(module_key, module_aliases);

        let adopted = adopted_fields(items, &module);
        for item in items {
            match item {
                syn::Item::Struct(item_struct) if item_struct.generics.params.is_empty() => {
                    let key = type_key(&module, &item_struct.ident.to_string());
                    self.structs.insert(key.clone(), StructInfo {
                        module: module.clone(),
                        fields: item_struct.fields.iter().cloned().collect(),
                        adopted: adopted.get(&key).cloned().unwrap_or_default(),
                    });
                }
                syn::Item::Enum(item_enum) if item_enum.generics.params.is_empty() => {
                    let key = type_key(&module, &item_enum.ident.to_string());
                    self.enums.insert(key, EnumInfo {
                        unit: item_enum
                            .variants
                            .iter()
                            .all(|variant| matches!(variant.fields, syn::Fields::Unit)),
                        portable_value: has_portable_value_derive(&item_enum.attrs),
                    });
                }
                syn::Item::Mod(item_mod) => {
                    let mut child_module = module.clone();
                    child_module.push(item_mod.ident.to_string());
                    if let Some((_, child_items)) = &item_mod.content {
                        self.collect_items(
                            child_items,
                            child_module,
                            file,
                            directory.join(item_mod.ident.to_string()),
                        )?;
                    } else {
                        let child = discovery::resolve_module_file_for_transform(
                            self.root,
                            file,
                            &directory,
                            item_mod,
                        )?;
                        self.collect_file(&child, child_module)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn transform_file(&self, file: &Path, module: &[String]) -> Result<(), ShadowError> {
        let source = fs::read_to_string(file).map_err(|error| io_error("read", file, error))?;
        let syntax = syn::parse_file(&source).map_err(|error| malformed(file, error))?;
        let mut insertions = Vec::new();
        let generated = self.generated_items(
            &syntax.items,
            module,
            &source,
            &mut insertions,
        )?;
        if generated.is_empty() && insertions.is_empty() {
            return Ok(());
        }
        if !generated.is_empty() {
            insertions.push((source.len(), generated));
        }
        insertions.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
        let mut output = source;
        for (offset, items) in insertions {
            let mut generated = quote!(#(#items)*).to_string();
            generated.insert(0, '\n');
            generated.push('\n');
            output.insert_str(offset, &generated);
        }
        fs::write(file, output).map_err(|error| io_error("write", file, error))
    }

    fn generated_items(
        &self,
        items: &[syn::Item],
        module: &[String],
        source: &str,
        insertions: &mut Vec<(usize, Vec<syn::Item>)>,
    ) -> Result<Vec<syn::Item>, ShadowError> {
        for item in items {
            if let syn::Item::Mod(item_mod) = item
                && let Some((brace, child_items)) = &item_mod.content
            {
                let mut child_module = module.to_vec();
                child_module.push(item_mod.ident.to_string());
                let generated = self.generated_items(
                    child_items,
                    &child_module,
                    source,
                    insertions,
                )?;
                if !generated.is_empty() {
                    let offset = source_offset(
                        source,
                        brace.span.close().start(),
                    )
                    .ok_or_else(|| {
                        ShadowError::new(
                            ShadowErrorKind::MalformedSource,
                            format!(
                                "failed to locate the closing brace for inline module `{}`",
                                child_module.join("::")
                            ),
                        )
                    })?;
                    insertions.push((offset, generated));
                }
            }
        }

        let existing = items.iter().filter_map(|item| match item {
            syn::Item::Const(item_const) => Some(item_const.ident.to_string()),
            _ => None,
        }).collect::<BTreeSet<_>>();
        let mut generated_items = Vec::new();
        for item in items {
            let generated = match item {
                syn::Item::Struct(item_struct) if item_struct.generics.params.is_empty() => {
                    let portable_name = format!(
                        "{}::{}::{}",
                        self.package,
                        module.join("::"),
                        item_struct.ident,
                    );
                    let names = GeneratedNames::new(&portable_name);
                    if existing.contains(&names.schema.to_string()) {
                        None
                    } else {
                        Some(self.generate(item_struct, module, &portable_name, &names)?)
                    }
                }
                syn::Item::Enum(item_enum)
                    if item_enum.generics.params.is_empty()
                        && self
                            .enums
                            .get(&type_key(module, &item_enum.ident.to_string()))
                            .is_some_and(|info| info.unit && !info.portable_value) =>
                {
                    let portable_name = format!(
                        "{}::{}::{}",
                        self.package,
                        module.join("::"),
                        item_enum.ident,
                    );
                    let names = GeneratedNames::new(&portable_name);
                    if existing.contains(&names.schema.to_string()) {
                        None
                    } else {
                        Some(self.generate_enum(item_enum, &portable_name, &names)?)
                    }
                }
                _ => None,
            };
            if let Some(mut generated) = generated {
                generated_items.append(&mut generated);
            }
        }
        Ok(generated_items)
    }

    fn generate(
        &self,
        item: &syn::ItemStruct,
        module: &[String],
        portable_name: &str,
        names: &GeneratedNames,
    ) -> Result<Vec<syn::Item>, ShadowError> {
        let runtime = &self.runtime;
        let ident = &item.ident;
        let fields_ident = &names.fields;
        let schema_ident = &names.schema;
        let retained_ident = &names.retained;
        let type_name = syn::LitStr::new(&ident.to_string(), ident.span());
        let portable_name = syn::LitStr::new(portable_name, ident.span());
        let key = type_key(module, &ident.to_string());
        let info = self.structs.get(&key).expect("collected struct exists");
        let fields = item.fields.iter().enumerate().map(|(index, field)| {
            self.field_plan(field, index, info)
        }).collect::<Vec<_>>();

        let descriptor_constants = fields.iter().enumerate().map(|(index, field)| {
            let cfg = &field.cfg;
            let descriptor = names.field(index);
            let name = syn::LitStr::new(&field.name, ident.span());
            let rust_type = syn::LitStr::new(&field.rust_type, ident.span());
            let kind = field.kind.tokens(runtime);
            let ty = &field.ty;
            let stable_type = match ty {
                syn::Type::Path(path)
                    if path.qself.is_none() && self.resolve_source(&path.path, module).is_some() =>
                {
                    quote! { .stable_type_id(<#ty as #runtime::AimerReflectionType>::TYPE_ID) }
                }
                _ => quote! {},
            };
            quote! {
                #(#cfg)*
                #[doc(hidden)]
                const #descriptor: #runtime::FieldDescriptor =
                    #runtime::FieldDescriptor::new(#name, #rust_type, #kind)#stable_type;
            }
        });
        let descriptor_values = fields.iter().enumerate().map(|(index, field)| {
            let cfg = &field.cfg;
            let descriptor = names.field(index);
            quote! { #(#cfg)* #descriptor }
        });
        let encode = fields.iter().enumerate().map(|(index, field)| {
            let cfg = &field.cfg;
            let descriptor = names.field(index);
            if field.kind == FieldKind::Retained {
                let member = &field.member;
                quote! {
                    #(#cfg)*
                    encoder.field(&#descriptor, |encoder| {
                        #runtime::PortableEncode::encode(&self.#member, encoder)
                    })?;
                }
            } else {
                quote! {
                    #(#cfg)*
                    encoder.field(&#descriptor, |_encoder| Ok(()))?;
                }
            }
        });
        let blockers = fields.iter().filter(|field| {
            field.kind != FieldKind::Retained
                || !self.type_fully_decodable(&field.ty, module, &mut BTreeSet::new())
        }).collect::<Vec<_>>();
        let decode_impl = blockers.iter().all(|field| !field.cfg.is_empty()).then(|| {
            let gates = blockers.iter().map(|field| cfg_absence_gate(&field.cfg));
            let values = fields.iter().enumerate().map(|(index, _)| {
                let field = &fields[index];
                let cfg = &field.cfg;
                let descriptor = names.field(index);
                quote! {
                    #(#cfg)*
                    decoder.field(&#descriptor)?
                        .expect("generated retained field metadata is consistent")
                }
            }).collect::<Vec<_>>();
            let construct = construct(item, &fields, &values);
            quote! {
                #(#gates)*
                impl #runtime::PortableDecode for #ident {
                    fn decode(decoder: &mut #runtime::Decoder<'_>) -> Result<Self, #runtime::DecodeError> {
                        decoder.nested(|decoder| {
                            let _ = &mut *decoder;
                            Ok(#construct)
                        })
                    }
                }
            }
        });

        let retained = fields.iter().enumerate()
            .filter(|(_, field)| field.kind == FieldKind::Retained)
            .collect::<Vec<_>>();
        let retained_struct_fields = retained.iter().map(|(index, field)| {
            let cfg = &field.cfg;
            let name = names.retained_field(*index);
            let ty = &field.ty;
            let retained_type = if self.direct_partial_source(ty, module) {
                quote! { <#ty as #runtime::PortableApply>::Retained }
            } else {
                quote! { #ty }
            };
            quote! { #(#cfg)* #name: #retained_type }
        });
        let retained_type = quote! { #retained_ident };
        let decode_retained = fields.iter().enumerate().map(|(index, field)| {
            let cfg = &field.cfg;
            let descriptor = names.field(index);
            match field.kind {
                FieldKind::Retained => {
                    let ty = &field.ty;
                    if self.direct_partial_source(ty, module) {
                        quote! {
                            #(#cfg)*
                            <#ty as #runtime::PortableApply>::decode_retained(decoder)?
                        }
                    } else {
                        quote! {
                            #(#cfg)*
                            decoder.field(&#descriptor)?
                                .expect("generated retained field metadata is consistent")
                        }
                    }
                }
                FieldKind::Fresh | FieldKind::Unsupported => quote! {
                    #(#cfg)*
                    {
                        let _ = decoder.field::<u8>(&#descriptor)?;
                    }
                },
            }
        }).collect::<Vec<_>>();
        let retained_values = decode_retained.iter().enumerate().filter_map(|(index, value)| {
            (fields[index].kind == FieldKind::Retained).then(|| {
                let cfg = &fields[index].cfg;
                let name = names.retained_field(index);
                quote! { #(#cfg)* #name: #value }
            })
        });
        let retained_value = quote! {
            #retained_ident {
                #(#retained_values,)*
            }
        };
        let validation = decode_retained.iter().enumerate().filter_map(|(index, value)| {
            (fields[index].kind != FieldKind::Retained).then_some(value)
        });
        let apply = retained.iter().map(|(index, field)| {
            let cfg = &field.cfg;
            let member = &field.member;
            let binding = names.retained_field(*index);
            if self.direct_partial_source(&field.ty, module) {
                let ty = &field.ty;
                quote! {
                    #(#cfg)*
                    <#ty as #runtime::PortableApply>::apply_retained(
                        &mut self.#member,
                        retained.#binding,
                    );
                }
            } else {
                quote! { #(#cfg)* self.#member = retained.#binding; }
            }
        });

        let generated = quote! {
            #(#descriptor_constants)*
            #[doc(hidden)]
            const #fields_ident: &[#runtime::FieldDescriptor] = &[#(#descriptor_values,)*];
            #[doc(hidden)]
            pub struct #retained_ident {
                #(#retained_struct_fields,)*
            }
            #[doc(hidden)]
            const #schema_ident: #runtime::TypeSchema = #runtime::TypeSchema::new(
                #type_name,
                #runtime::StableId128::from_path("aimer.type.v1", #portable_name),
                #fields_ident,
            );
            impl #runtime::AimerReflectionType for #ident {
                const TYPE_ID: #runtime::StableTypeId =
                    #runtime::StableId128::from_path("aimer.type.v1", #portable_name);

                fn schema() -> &'static #runtime::TypeSchema {
                    &#schema_ident
                }
            }
            impl #runtime::PortableEncode for #ident {
                fn encode(&self, encoder: &mut #runtime::Encoder<'_>) -> Result<(), #runtime::EncodeError> {
                    encoder.nested(|encoder| {
                        let _ = &mut *encoder;
                        #(#encode)*
                        Ok(())
                    })
                }
            }
            #decode_impl
            impl #runtime::PortableApply for #ident {
                type Retained = #retained_type;

                fn decode_retained(
                    decoder: &mut #runtime::Decoder<'_>,
                ) -> Result<Self::Retained, #runtime::DecodeError> {
                    decoder.nested(|decoder| {
                        let _ = &mut *decoder;
                        #(#validation)*
                        Ok(#retained_value)
                    })
                }

                fn apply_retained(&mut self, retained: Self::Retained) {
                    #(#apply)*
                }
            }
        };
        syn::parse2::<syn::File>(generated)
            .map(|file| file.items)
            .map_err(|error| ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!(
                    "failed to generate reflection for {}: {error}",
                    portable_name.value(),
                ),
            ))
    }

    fn generate_enum(
        &self,
        item: &syn::ItemEnum,
        portable_name: &str,
        names: &GeneratedNames,
    ) -> Result<Vec<syn::Item>, ShadowError> {
        let runtime = &self.runtime;
        let ident = &item.ident;
        let fields_ident = &names.fields;
        let schema_ident = &names.schema;
        let type_name = syn::LitStr::new(&ident.to_string(), ident.span());
        let portable_name = syn::LitStr::new(portable_name, ident.span());
        let encode_arms = item.variants.iter().enumerate().map(|(tag, variant)| {
            let variant_ident = &variant.ident;
            let tag = tag as u32;
            quote! { Self::#variant_ident => #tag }
        });
        let decode_arms = item.variants.iter().enumerate().map(|(tag, variant)| {
            let variant_ident = &variant.ident;
            let tag = tag as u32;
            quote! { #tag => Ok(Self::#variant_ident) }
        });
        let generated = quote! {
            #[doc(hidden)]
            const #fields_ident: &[#runtime::FieldDescriptor] = &[];
            #[doc(hidden)]
            const #schema_ident: #runtime::TypeSchema = #runtime::TypeSchema::new(
                #type_name,
                #runtime::StableId128::from_path("aimer.type.v1", #portable_name),
                #fields_ident,
            );
            impl #runtime::AimerReflectionType for #ident {
                const TYPE_ID: #runtime::StableTypeId =
                    #runtime::StableId128::from_path("aimer.type.v1", #portable_name);

                fn schema() -> &'static #runtime::TypeSchema {
                    &#schema_ident
                }
            }
            impl #runtime::PortableEncode for #ident {
                fn encode(
                    &self,
                    encoder: &mut #runtime::Encoder<'_>,
                ) -> Result<(), #runtime::EncodeError> {
                    encoder.nested(|encoder| {
                        let tag = match self {
                            #(#encode_arms,)*
                        };
                        #runtime::PortableEncode::encode(&tag, encoder)
                    })
                }
            }
            impl #runtime::PortableDecode for #ident {
                fn decode(
                    decoder: &mut #runtime::Decoder<'_>,
                ) -> Result<Self, #runtime::DecodeError> {
                    decoder.nested(|decoder| {
                        let tag = <u32 as #runtime::PortableDecode>::decode(decoder)?;
                        match tag {
                            #(#decode_arms,)*
                            _ => Err(#runtime::DecodeError::InvalidEnumTag(tag)),
                        }
                    })
                }
            }
        };
        syn::parse2::<syn::File>(generated)
            .map(|file| file.items)
            .map_err(|error| ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!(
                    "failed to generate reflection for {}: {error}",
                    portable_name.value(),
                ),
            ))
    }

    fn field_plan(&self, field: &syn::Field, index: usize, info: &StructInfo) -> FieldPlan {
        let name = field.ident.as_ref().map(ToString::to_string)
            .unwrap_or_else(|| index.to_string());
        let member = field.ident.clone().map(syn::Member::Named)
            .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)));
        let kind = if info.adopted.contains(&name) || self.type_is_fresh(&field.ty, &info.module) {
            FieldKind::Fresh
        } else if self.type_is_retained(&field.ty, &info.module) {
            FieldKind::Retained
        } else {
            FieldKind::Unsupported
        };
        FieldPlan {
            name,
            rust_type: compact_tokens(&field.ty),
            ty: field.ty.clone(),
            member,
            kind,
            cfg: cfg_attributes(field),
        }
    }

    fn type_is_retained(&self, ty: &syn::Type, module: &[String]) -> bool {
        match ty {
            syn::Type::Path(path) if path.qself.is_none() => {
                let Some(last) = path.path.segments.last() else { return false; };
                let name = last.ident.to_string();
                if is_primitive(&name) {
                    return true;
                }
                if let Some(key) = self.resolve_source(&path.path, module) {
                    return self.structs.contains_key(&key)
                        || self.enums.get(&key).is_some_and(|info| {
                            info.unit || info.portable_value
                        });
                }
                if matches!(name.as_str(), "Option" | "Vec" | "Box") {
                    return one_type_argument(last)
                        .is_some_and(|inner| self.type_is_retained(inner, module));
                }
                false
            }
            syn::Type::Array(array) => self.type_is_retained(&array.elem, module),
            syn::Type::Tuple(tuple) if tuple.elems.len() <= 12 => {
                !tuple.elems.is_empty()
                    && tuple.elems.iter().all(|element| self.type_is_retained(element, module))
            }
            syn::Type::Paren(paren) => self.type_is_retained(&paren.elem, module),
            _ => false,
        }
    }

    fn type_is_fresh(&self, ty: &syn::Type, module: &[String]) -> bool {
        match ty {
            syn::Type::BareFn(_) => true,
            syn::Type::TraitObject(object) => object.bounds.iter().any(bound_is_callback),
            syn::Type::ImplTrait(object) => object.bounds.iter().any(bound_is_callback),
            syn::Type::Path(path) if path.qself.is_none() => {
                if self.resolve_source(&path.path, module).is_some() {
                    return false;
                }
                let Some(last) = path.path.segments.last() else { return false; };
                let name = last.ident.to_string();
                // Runtime controller handles own native resources and cannot be
                // serialized into the portable state bundle. They are fresh
                // configuration when imported from a framework crate; source
                // types were resolved above and still take their normal path.
                if name == "StateUpdater"
                    || name.contains("Callback")
                    || name.ends_with("Handler")
                    || name.ends_with("Controller")
                {
                    return true;
                }
                matches!(name.as_str(), "Option" | "Vec" | "Box")
                    && one_type_argument(last).is_some_and(|inner| self.type_is_fresh(inner, module))
            }
            syn::Type::Array(array) => self.type_is_fresh(&array.elem, module),
            syn::Type::Tuple(tuple) => tuple.elems.iter().any(|element| self.type_is_fresh(element, module)),
            syn::Type::Paren(paren) => self.type_is_fresh(&paren.elem, module),
            _ => false,
        }
    }

    fn type_fully_decodable(
        &self,
        ty: &syn::Type,
        module: &[String],
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        match ty {
            syn::Type::Path(path) if path.qself.is_none() => {
                let Some(last) = path.path.segments.last() else { return false; };
                let name = last.ident.to_string();
                if is_primitive(&name) {
                    return true;
                }
                if let Some(key) = self.resolve_source(&path.path, module) {
                    if let Some(info) = self.structs.get(&key) {
                        if !visiting.insert(key.clone()) {
                            return true;
                        }
                        let complete = info.fields.iter().enumerate().all(|(index, field)| {
                            let plan = self.field_plan(field, index, info);
                            plan.kind == FieldKind::Retained
                                && self.type_fully_decodable(&field.ty, &info.module, visiting)
                        });
                        visiting.remove(&key);
                        return complete;
                    }
                    return self.enums.get(&key).is_some_and(|info| {
                        info.unit || info.portable_value
                    });
                }
                matches!(name.as_str(), "Option" | "Vec" | "Box")
                    && one_type_argument(last)
                        .is_some_and(|inner| self.type_fully_decodable(inner, module, visiting))
            }
            syn::Type::Array(array) => self.type_fully_decodable(&array.elem, module, visiting),
            syn::Type::Tuple(tuple) if tuple.elems.len() <= 12 => {
                !tuple.elems.is_empty()
                    && tuple.elems.iter().all(|element| {
                        self.type_fully_decodable(element, module, visiting)
                    })
            }
            syn::Type::Paren(paren) => self.type_fully_decodable(&paren.elem, module, visiting),
            _ => false,
        }
    }

    fn direct_partial_source(&self, ty: &syn::Type, module: &[String]) -> bool {
        let syn::Type::Path(path) = ty else { return false; };
        path.qself.is_none()
            && self.resolve_source(&path.path, module).is_some()
            && !self.type_fully_decodable(ty, module, &mut BTreeSet::new())
    }

    fn resolve_source(&self, path: &syn::Path, module: &[String]) -> Option<String> {
        let mut segments = path.segments.iter().map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        if segments.is_empty() {
            return None;
        }
        if let Some(alias) = self.aliases.get(&module.join("::"))
            .and_then(|aliases| aliases.get(&segments[0]))
        {
            segments = alias.iter().cloned().chain(segments.into_iter().skip(1)).collect();
        }
        let candidates = path_candidates(&segments, module);
        candidates.into_iter().map(|candidate| candidate.join("::"))
            .find(|candidate| {
                self.structs.contains_key(candidate) || self.enums.contains_key(candidate)
            })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FieldKind {
    Retained,
    Fresh,
    Unsupported,
}

impl FieldKind {
    fn tokens(self, runtime: &syn::Path) -> impl ToTokens {
        match self {
            Self::Retained => quote! { #runtime::FieldKind::Retained },
            Self::Fresh => quote! { #runtime::FieldKind::Fresh },
            Self::Unsupported => quote! { #runtime::FieldKind::Unsupported },
        }
    }
}

struct FieldPlan {
    name: String,
    rust_type: String,
    ty: syn::Type,
    member: syn::Member,
    kind: FieldKind,
    cfg: Vec<syn::Attribute>,
}

struct GeneratedNames {
    fields: syn::Ident,
    schema: syn::Ident,
    retained: syn::Ident,
}

impl GeneratedNames {
    fn new(identity: &str) -> Self {
        let digest = hex::encode(Sha256::digest(identity.as_bytes()));
        let suffix = digest[..16].to_ascii_uppercase();
        Self {
            fields: format_ident!("__AIMER_REFLECTION_FIELDS_{suffix}"),
            schema: format_ident!("__AIMER_REFLECTION_SCHEMA_{suffix}"),
            retained: format_ident!("__AIMER_PORTABLE_RETAINED_{suffix}"),
        }
    }

    fn field(&self, index: usize) -> syn::Ident {
        let schema = self.schema.to_string();
        format_ident!("{schema}_FIELD_{index}")
    }

    fn retained_field(&self, index: usize) -> syn::Ident {
        format_ident!("field_{index}")
    }
}

fn construct(
    item: &syn::ItemStruct,
    plans: &[FieldPlan],
    values: &[impl ToTokens],
) -> impl ToTokens {
    match &item.fields {
        syn::Fields::Named(fields) => {
            let initializers = fields.named.iter().zip(plans).zip(values).map(
                |((field, plan), value)| {
                    let cfg = &plan.cfg;
                    let name = field.ident.as_ref().expect("named field");
                    quote! { #(#cfg)* #name: #value }
                },
            );
            quote! { Self { #(#initializers,)* } }
        }
        syn::Fields::Unnamed(_) => {
            let initializers = plans.iter().zip(values).map(|(plan, value)| {
                let cfg = &plan.cfg;
                quote! { #(#cfg)* #value }
            });
            quote! { Self(#(#initializers,)*) }
        }
        syn::Fields::Unit => quote! { Self },
    }
}

fn cfg_attributes(field: &syn::Field) -> Vec<syn::Attribute> {
    field.attrs.iter().filter(|attribute| attribute.path().is_ident("cfg"))
        .cloned().collect()
}

fn cfg_absence_gate(attributes: &[syn::Attribute]) -> impl ToTokens {
    let predicates = attributes.iter().filter_map(|attribute| match &attribute.meta {
        syn::Meta::List(list) => Some(&list.tokens),
        _ => None,
    });
    quote! { #[cfg(not(all(#(#predicates),*)))] }
}

fn has_portable_value_derive(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("derive")
            && attribute.meta.to_token_stream().to_string().contains("PortableValue")
    })
}

fn adopted_fields(
    items: &[syn::Item],
    module: &[String],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut result = BTreeMap::new();
    for item in items {
        let syn::Item::Impl(item_impl) = item else { continue; };
        let syn::Type::Path(self_ty) = item_impl.self_ty.as_ref() else { continue; };
        let Some(type_name) = self_ty.path.segments.last().map(|segment| segment.ident.to_string()) else {
            continue;
        };
        for impl_item in &item_impl.items {
            let syn::ImplItem::Fn(method) = impl_item else { continue; };
            if method.sig.ident != "adopt_config_from" {
                continue;
            }
            let candidates = method.sig.inputs.iter().filter_map(|input| match input {
                syn::FnArg::Typed(argument) if matches!(argument.ty.as_ref(), syn::Type::Path(path)
                    if path.path.is_ident("Self")) => match argument.pat.as_ref() {
                        syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
                        _ => None,
                    },
                _ => None,
            }).collect::<BTreeSet<_>>();
            let mut visitor = AdoptVisitor { candidates: &candidates, fields: BTreeSet::new() };
            visitor.visit_block(&method.block);
            result.entry(type_key(module, &type_name)).or_insert_with(BTreeSet::new)
                .extend(visitor.fields);
        }
    }
    result
}

struct AdoptVisitor<'a> {
    candidates: &'a BTreeSet<String>,
    fields: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for AdoptVisitor<'_> {
    fn visit_expr_assign(&mut self, expression: &'ast syn::ExprAssign) {
        if let (Some((left_owner, left)), Some((right_owner, right))) = (
            member_access_with_owner(&expression.left),
            member_access_with_owner(&expression.right),
        ) && left_owner == "self" && left == right && self.candidates.contains(&right_owner) {
            self.fields.insert(left);
        }
        visit::visit_expr_assign(self, expression);
    }
}

fn member_access_with_owner(expression: &syn::Expr) -> Option<(String, String)> {
    let syn::Expr::Field(field) = expression else { return None; };
    let syn::Expr::Path(owner) = field.base.as_ref() else { return None; };
    let owner = owner.path.get_ident()?.to_string();
    let member = match &field.member {
        syn::Member::Named(name) => name.to_string(),
        syn::Member::Unnamed(index) => index.index.to_string(),
    };
    Some((owner, member))
}

fn runtime_path(manifest: &toml::Value) -> Result<syn::Path, ShadowError> {
    let dependencies = manifest.get("dependencies").and_then(toml::Value::as_table);
    let mut selected = None;
    for wanted in ["aimer_widget", "aimer"] {
        if let Some(table) = dependencies {
            for (name, value) in table {
                let package = value.as_table().and_then(|entry| entry.get("package"))
                    .and_then(toml::Value::as_str).unwrap_or(name);
                if package == wanted {
                    selected = Some(name.replace('-', "_"));
                    break;
                }
            }
        }
        if selected.is_some() {
            break;
        }
    }
    let crate_name = selected.unwrap_or_else(|| "aimer_widget".to_owned());
    syn::parse_str(&format!("::{crate_name}::portable")).map_err(|error| {
        ShadowError::new(
            ShadowErrorKind::Manifest,
            format!("invalid portable runtime dependency name `{crate_name}`: {error}"),
        )
    })
}

fn path_candidates(path: &[String], module: &[String]) -> Vec<Vec<String>> {
    if path.first().is_some_and(|segment| segment == "crate") {
        return vec![path.to_vec()];
    }
    if path.first().is_some_and(|segment| segment == "self") {
        return vec![module.iter().cloned().chain(path.iter().skip(1).cloned()).collect()];
    }
    if path.first().is_some_and(|segment| segment == "super") {
        let mut base = module.to_vec();
        let mut index = 0;
        while path.get(index).is_some_and(|segment| segment == "super") {
            if base.len() > 1 {
                base.pop();
            }
            index += 1;
        }
        base.extend_from_slice(&path[index..]);
        return vec![base];
    }
    vec![
        module.iter().cloned().chain(path.iter().cloned()).collect(),
        std::iter::once("crate".to_owned()).chain(path.iter().cloned()).collect(),
    ]
}

fn collect_use_aliases(
    tree: &syn::UseTree,
    prefix: Vec<String>,
    module: &[String],
    aliases: &mut BTreeMap<String, Vec<String>>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            collect_use_aliases(&path.tree, next, module, aliases);
        }
        syn::UseTree::Name(name) => {
            let mut target = prefix;
            target.push(name.ident.to_string());
            aliases.insert(name.ident.to_string(), normalize_use_path(target, module));
        }
        syn::UseTree::Rename(rename) => {
            let mut target = prefix;
            target.push(rename.ident.to_string());
            aliases.insert(rename.rename.to_string(), normalize_use_path(target, module));
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_aliases(item, prefix.clone(), module, aliases);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

fn normalize_use_path(path: Vec<String>, module: &[String]) -> Vec<String> {
    if path.first().is_some_and(|segment| segment == "self") {
        module.iter().cloned().chain(path.into_iter().skip(1)).collect()
    } else {
        path
    }
}

fn bound_is_callback(bound: &syn::TypeParamBound) -> bool {
    matches!(bound, syn::TypeParamBound::Trait(trait_bound)
        if trait_bound.path.segments.last().is_some_and(|segment| {
            matches!(segment.ident.to_string().as_str(), "Fn" | "FnMut" | "FnOnce")
        }))
}

fn one_type_argument(segment: &syn::PathSegment) -> Option<&syn::Type> {
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else { return None; };
    if arguments.args.len() != 1 {
        return None;
    }
    match arguments.args.first()? {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "bool" | "char" | "String"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "f32" | "f64"
    )
}

fn type_key(module: &[String], name: &str) -> String {
    format!("{}::{name}", module.join("::"))
}

fn source_offset(source: &str, location: proc_macro2::LineColumn) -> Option<usize> {
    let mut line_starts = vec![0usize];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    let line_start = *line_starts.get(location.line.checked_sub(1)?)?;
    let offset = line_start.checked_add(location.column)?;
    (offset <= source.len()).then_some(offset)
}

fn compact_tokens(tokens: &impl ToTokens) -> String {
    tokens.to_token_stream().to_string().chars()
        .filter(|character| !character.is_whitespace()).collect()
}

fn module_directory(file: &Path) -> PathBuf {
    let parent = file.parent().expect("Rust source has a parent");
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => parent.to_owned(),
        _ => parent.join(file.file_stem().expect("Rust source has a stem")),
    }
}

fn malformed(path: &Path, error: syn::Error) -> ShadowError {
    ShadowError::new(
        ShadowErrorKind::MalformedSource,
        format!("failed to parse Rust module {}: {error}", path.display()),
    )
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> ShadowError {
    ShadowError::new(
        ShadowErrorKind::Io,
        format!("failed to {action} Rust module {}: {error}", path.display()),
    )
}
