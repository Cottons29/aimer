use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::visit::{self, Visit};

use super::{ShadowError, ShadowErrorKind};

/// Byte range in one UTF-8 source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    path: PathBuf,
    start: usize,
    end: usize,
}

impl SourceSpan {
    pub(crate) fn new(path: PathBuf, start: usize, end: usize) -> Self {
        Self { path, start, end }
    }

    /// Returns the source file containing this range.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the inclusive start byte offset.
    #[inline]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Returns the exclusive end byte offset.
    #[inline]
    pub const fn end(&self) -> usize {
        self.end
    }
}

/// Stable package/module/type identity for a source declaration.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableIdentity {
    package: String,
    module: String,
    type_name: String,
}

impl StableIdentity {
    fn new(package: &str, module: &[String], type_name: impl Into<String>) -> Self {
        Self {
            package: package.to_owned(),
            module: module.join("::"),
            type_name: type_name.into(),
        }
    }
    /// Returns the Cargo package name.
    #[inline]
    pub fn package(&self) -> &str { &self.package }
    /// Returns the canonical module path beginning with `crate`.
    #[inline]
    pub fn module(&self) -> &str { &self.module }
    /// Returns the declared Rust type name.
    #[inline]
    pub fn type_name(&self) -> &str { &self.type_name }
    /// Emits the unambiguous portable identity text.
    #[inline]
    pub fn portable_name(&self) -> String {
        format!("{}::{}::{}", self.package, self.module, self.type_name)
    }
}

/// One struct declaration found in an owned application module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceType {
    identity: StableIdentity,
    span: SourceSpan,
}

impl SourceType {
    /// Returns the declaration's stable identity.
    #[inline]
    pub fn identity(&self) -> &StableIdentity { &self.identity }
    /// Returns the declaration's source span.
    #[inline]
    pub fn span(&self) -> &SourceSpan { &self.span }
}

/// Unique statically resolved application root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootDiscovery {
    entry: StableIdentity,
    entry_source: PathBuf,
    crate_source: PathBuf,
    root_expression: String,
    root_span: SourceSpan,
    call_path: Vec<StableIdentity>,
    startup_functions: Vec<StartupFunction>,
}

impl RootDiscovery {
    /// Returns the `#[aimer::main]` function identity.
    #[inline]
    pub fn entry(&self) -> &StableIdentity { &self.entry }
    /// Returns the owned source file containing `#[aimer::main]`.
    #[inline]
    pub fn entry_source(&self) -> &Path { &self.entry_source }
    /// Returns the selected Cargo library crate-root source file.
    #[inline]
    pub fn crate_source(&self) -> &Path { &self.crate_source }
    /// Returns deterministic token text for the expression passed to `child`.
    #[inline]
    pub fn root_expression(&self) -> &str { &self.root_expression }
    /// Returns the source range containing the root expression.
    #[inline]
    pub fn root_span(&self) -> &SourceSpan { &self.root_span }
    /// Returns followed functions from entry to the function owning the chain.
    #[inline]
    pub fn call_path(&self) -> &[StableIdentity] { &self.call_path }

    pub(crate) fn startup_functions(&self) -> &[StartupFunction] { &self.startup_functions }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StartupFunction {
    pub(crate) identity: StableIdentity,
    pub(crate) source: PathBuf,
    pub(crate) file_module: String,
}

pub(crate) fn discover(
    root: &Path,
    package: &str,
    manifest: &toml::Value,
    max_depth: usize,
) -> Result<(RootDiscovery, Vec<SourceType>), ShadowError> {
    let roots = source_roots(root, manifest)?;
    let mut graph = ModuleGraph::new(root, package);
    for source_root in roots {
        graph.load_file(
            source_root.clone(),
            vec!["crate".to_owned()],
            None,
            &source_root,
        )?;
    }
    graph.finish(max_depth)
}

#[derive(Clone)]
struct Function {
    identity: StableIdentity,
    module: Vec<String>,
    file: PathBuf,
    file_module: String,
    crate_source: PathBuf,
    source: String,
    item: syn::ItemFn,
    aliases: BTreeMap<String, Vec<String>>,
}

struct ModuleGraph<'a> {
    root: &'a Path,
    package: &'a str,
    owners: BTreeMap<PathBuf, String>,
    functions: BTreeMap<String, Function>,
    entries: Vec<String>,
    source_types: Vec<SourceType>,
    first_source: Option<SourceSpan>,
}

impl<'a> ModuleGraph<'a> {
    fn new(root: &'a Path, package: &'a str) -> Self {
        Self {
            root,
            package,
            owners: BTreeMap::new(),
            functions: BTreeMap::new(),
            entries: Vec::new(),
            source_types: Vec::new(),
            first_source: None,
        }
    }

    fn load_file(
        &mut self,
        file: PathBuf,
        module: Vec<String>,
        declared_at: Option<SourceSpan>,
        crate_source: &Path,
    ) -> Result<(), ShadowError> {
        let canonical = fs::canonicalize(&file).map_err(|error| {
            ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!("failed to resolve module {}: {error}", file.display()),
            )
        })?;
        if !canonical.starts_with(self.root) {
            return Err(ShadowError::new(
                ShadowErrorKind::PathEscape,
                format!("module escapes shadow root: {}", file.display()),
            ));
        }
        let owner = module.join("::");
        if let Some(previous) = self.owners.insert(canonical.clone(), owner.clone()) {
            return Err(ShadowError::new(
                ShadowErrorKind::DuplicateModule,
                format!("module file {} is owned by both {previous} and {owner}", canonical.display()),
            ).at(declared_at.unwrap_or_else(|| SourceSpan::new(canonical, 0, 0))));
        }
        let source = fs::read_to_string(&canonical).map_err(|error| {
            ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!("failed to read Rust module {}: {error}", canonical.display()),
            )
        })?;
        let syntax = syn::parse_file(&source).map_err(|error| {
            ShadowError::new(
                ShadowErrorKind::MalformedSource,
                format!("failed to parse Rust module {}: {error}", canonical.display()),
            ).at(SourceSpan::new(canonical.clone(), 0, source.len()))
        })?;
        self.first_source.get_or_insert_with(|| {
            SourceSpan::new(canonical.clone(), 0, source.len())
        });
        let module_directory = module_directory(&canonical);
        let file_module = module.join("::");
        self.load_items(
            &syntax.items,
            module,
            &file_module,
            &canonical,
            &source,
            module_directory,
            crate_source,
        )
    }

    fn load_items(
        &mut self,
        items: &[syn::Item],
        module: Vec<String>,
        file_module: &str,
        file: &Path,
        source: &str,
        module_directory: PathBuf,
        crate_source: &Path,
    ) -> Result<(), ShadowError> {
        let mut aliases = BTreeMap::new();
        for item in items {
            if let syn::Item::Use(item_use) = item {
                collect_use_aliases(&item_use.tree, Vec::new(), &module, &mut aliases);
            }
        }
        for item in items {
            match item {
                syn::Item::Fn(function) => {
                    let identity = StableIdentity::new(self.package, &module, function.sig.ident.to_string());
                    let key = function_key(&module, &function.sig.ident.to_string());
                    if self.functions.contains_key(&key) {
                        return Err(ShadowError::new(
                            ShadowErrorKind::AmbiguousFlow,
                            format!("duplicate function identity {key}"),
                        ).at(span_for_name(file, source, &function.sig.ident.to_string())));
                    }
                    if is_aimer_main(function) {
                        self.entries.push(key.clone());
                    }
                    let mut function_aliases = aliases.clone();
                    collect_block_aliases(&function.block, &module, &mut function_aliases);
                    self.functions.insert(key, Function {
                        identity,
                        module: module.clone(),
                        file: file.to_owned(),
                        file_module: file_module.to_owned(),
                        crate_source: crate_source.to_owned(),
                        source: source.to_owned(),
                        item: function.clone(),
                        aliases: function_aliases,
                    });
                }
                syn::Item::Struct(item_struct) => {
                    self.source_types.push(SourceType {
                        identity: StableIdentity::new(self.package, &module, item_struct.ident.to_string()),
                        span: span_for_name(file, source, &item_struct.ident.to_string()),
                    });
                }
                syn::Item::Mod(item_mod) => {
                    let mut child_module = module.clone();
                    child_module.push(item_mod.ident.to_string());
                    let declaration_span = span_for_name(file, source, &item_mod.ident.to_string());
                    if let Some((_, child_items)) = &item_mod.content {
                        self.load_items(
                            child_items,
                            child_module,
                            file_module,
                            file,
                            source,
                            module_directory.join(item_mod.ident.to_string()),
                            crate_source,
                        )?;
                    } else {
                        let child = resolve_module_file(
                            self.root,
                            file,
                            &module_directory,
                            item_mod,
                            declaration_span.clone(),
                        )?;
                        self.load_file(child, child_module, Some(declaration_span), crate_source)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn finish(mut self, max_depth: usize) -> Result<(RootDiscovery, Vec<SourceType>), ShadowError> {
        if self.entries.len() != 1 {
            let span = self.entries.first()
                .and_then(|entry| self.functions.get(entry))
                .map(function_span)
                .or_else(|| self.first_source.clone());
            let mut error = ShadowError::new(
                if self.entries.is_empty() { ShadowErrorKind::MissingFlow } else { ShadowErrorKind::AmbiguousFlow },
                format!("expected exactly one #[aimer::main], found {}", self.entries.len()),
            );
            if let Some(span) = span { error = error.at(span); }
            return Err(error);
        }
        let entry_key = self.entries[0].clone();
        let mut visiting = BTreeSet::new();
        let candidate = resolve_flow(
            &entry_key,
            &self.functions,
            max_depth,
            0,
            &mut visiting,
        )?;
        self.source_types.sort_unstable_by(|left, right| left.identity.cmp(&right.identity));
        let entry_function = self.functions.get(&entry_key).expect("entry function exists");
        let entry = entry_function.identity.clone();
        let entry_source = entry_function.file.clone();
        let crate_source = entry_function.crate_source.clone();
        let startup_functions = candidate.path.iter().map(|identity| {
            let function = self.functions.values()
                .find(|function| function.identity == *identity)
                .expect("resolved call path function exists");
            StartupFunction {
                identity: identity.clone(),
                source: function.file.clone(),
                file_module: function.file_module.clone(),
            }
        }).collect();
        Ok((RootDiscovery {
            entry,
            entry_source,
            crate_source,
            root_expression: candidate.expression,
            root_span: candidate.span,
            call_path: candidate.path,
            startup_functions,
        }, self.source_types))
    }
}

struct RootCandidate {
    expression: String,
    span: SourceSpan,
    path: Vec<StableIdentity>,
}

fn resolve_flow(
    key: &str,
    functions: &BTreeMap<String, Function>,
    max_depth: usize,
    depth: usize,
    visiting: &mut BTreeSet<String>,
) -> Result<RootCandidate, ShadowError> {
    let function = functions.get(key).expect("resolved function exists");
    if depth > max_depth {
        return Err(ShadowError::new(
            ShadowErrorKind::LimitExceeded,
            format!("direct local call depth exceeds {max_depth}"),
        ).at(function_span(function)));
    }
    if !visiting.insert(key.to_owned()) {
        return Err(ShadowError::new(
            ShadowErrorKind::DynamicFlow,
            format!("recursive application root flow through {key}"),
        ).at(function_span(function)));
    }
    let scan = scan_function(function);
    if scan.chains.len() > 1 {
        visiting.remove(key);
        return Err(ShadowError::new(
            ShadowErrorKind::AmbiguousFlow,
            format!("function {key} contains multiple AimerApp root chains"),
        ).at(scan.chains[1].1.clone()));
    }
    if let Some((expression, span)) = scan.chains.into_iter().next() {
        visiting.remove(key);
        return Ok(RootCandidate {
            expression,
            span,
            path: vec![function.identity.clone()],
        });
    }
    let mut candidates = Vec::new();
    let mut first_unresolved = None;
    for call in scan.calls {
        match resolve_call(&call.path, function, functions) {
            Some(target) => match resolve_flow(&target, functions, max_depth, depth + 1, visiting) {
                Ok(mut candidate) => {
                    candidate.path.insert(0, function.identity.clone());
                    candidates.push(candidate);
                }
                Err(error) if error.kind() == ShadowErrorKind::MissingFlow => {}
                Err(error) => {
                    visiting.remove(key);
                    return Err(error);
                }
            },
            None if !is_external_call(&call.path) => {
                first_unresolved.get_or_insert(call.span);
            }
            None => {}
        }
    }
    visiting.remove(key);
    if candidates.len() > 1 {
        return Err(ShadowError::new(
            ShadowErrorKind::AmbiguousFlow,
            format!("function {key} reaches multiple AimerApp root chains"),
        ).at(candidates[1].span.clone()));
    }
    if let Some(candidate) = candidates.pop() {
        return Ok(candidate);
    }
    let dynamic = scan.dynamic.is_some();
    if let Some(span) = scan.dynamic.or(first_unresolved) {
        return Err(ShadowError::new(
            if dynamic { ShadowErrorKind::DynamicFlow } else { ShadowErrorKind::UnresolvedFlow },
            if dynamic {
                format!("function {key} invokes a dynamic root-flow target")
            } else {
                format!("function {key} invokes an unresolved local function")
            },
        ).at(span));
    }
    Err(ShadowError::new(
        ShadowErrorKind::MissingFlow,
        format!("function {key} does not reach AimerApp::new().child(...).run()"),
    ).at(function_span(function)))
}

struct CallSite {
    path: Vec<String>,
    span: SourceSpan,
}

#[derive(Default)]
struct FunctionScan {
    chains: Vec<(String, SourceSpan)>,
    calls: Vec<CallSite>,
    dynamic: Option<SourceSpan>,
    bindings: BTreeSet<String>,
}

fn scan_function(function: &Function) -> FunctionScan {
    let mut scan = FunctionScan::default();
    for statement in &function.item.block.stmts {
        if let syn::Stmt::Local(local) = statement
            && let syn::Pat::Ident(ident) = &local.pat
        {
            scan.bindings.insert(ident.ident.to_string());
        }
    }
    let mut visitor = FlowVisitor { function, scan: &mut scan };
    visitor.visit_block(&function.item.block);
    scan
}

struct FlowVisitor<'a> {
    function: &'a Function,
    scan: &'a mut FunctionScan,
}

impl<'ast> Visit<'ast> for FlowVisitor<'_> {
    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if let Some(root) = root_from_chain(expression) {
            let text = root.to_token_stream().to_string();
            self.scan.chains.push((
                text.clone(),
                span_for_tokens(&self.function.file, &self.function.source, &text),
            ));
            return;
        }
        visit::visit_expr_method_call(self, expression);
    }

    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        match expression.func.as_ref() {
            syn::Expr::Path(path) if path.qself.is_none() => {
                let segments = path.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>();
                let span = span_for_tokens(
                    &self.function.file,
                    &self.function.source,
                    &path.to_token_stream().to_string(),
                );
                if segments.len() == 1 && self.scan.bindings.contains(&segments[0]) {
                    self.scan.dynamic.get_or_insert(span);
                } else {
                    self.scan.calls.push(CallSite { path: segments, span });
                }
            }
            other => {
                let text = other.to_token_stream().to_string();
                self.scan.dynamic.get_or_insert_with(|| {
                    span_for_tokens(&self.function.file, &self.function.source, &text)
                });
            }
        }
    }

    fn visit_expr_closure(&mut self, _expression: &'ast syn::ExprClosure) {}
}

fn root_from_chain(expression: &syn::ExprMethodCall) -> Option<&syn::Expr> {
    if expression.method != "run" || !expression.args.is_empty() {
        return None;
    }
    let mut receiver = expression.receiver.as_ref();
    let mut child = None;
    loop {
        match receiver {
            syn::Expr::MethodCall(method) => {
                if method.method == "child" {
                    if child.is_some() || method.args.len() != 1 {
                        return None;
                    }
                    child = method.args.first();
                }
                receiver = method.receiver.as_ref();
            }
            syn::Expr::Call(call) => {
                let syn::Expr::Path(path) = call.func.as_ref() else { return None; };
                let segments = path.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>();
                return (segments.ends_with(&["AimerApp".to_owned(), "new".to_owned()])
                    && call.args.is_empty()).then_some(child?);
            }
            _ => return None,
        }
    }
}

fn resolve_call(
    call: &[String],
    function: &Function,
    functions: &BTreeMap<String, Function>,
) -> Option<String> {
    if call.is_empty() { return None; }
    let expanded = function.aliases.get(&call[0]).map(|alias| {
        alias.iter().cloned().chain(call[1..].iter().cloned()).collect::<Vec<_>>()
    }).unwrap_or_else(|| call.to_vec());
    let mut candidates = Vec::new();
    if expanded[0] == "crate" {
        candidates.push(expanded.clone());
    } else if expanded[0] == "self" {
        candidates.push(function.module.iter().cloned().chain(expanded[1..].iter().cloned()).collect());
    } else if expanded[0] == "super" {
        let mut base = function.module.clone();
        let mut index = 0;
        while expanded.get(index).is_some_and(|segment| segment == "super") {
            if base.len() > 1 { base.pop(); }
            index += 1;
        }
        base.extend_from_slice(&expanded[index..]);
        candidates.push(base);
    } else {
        candidates.push(function.module.iter().cloned().chain(expanded.iter().cloned()).collect());
        candidates.push(std::iter::once("crate".to_owned()).chain(expanded).collect());
    }
    candidates.into_iter().map(|path| path.join("::")).find(|key| functions.contains_key(key))
}

fn is_external_call(call: &[String]) -> bool {
    call.iter().any(|segment| segment.chars().next().is_some_and(char::is_uppercase))
        || call.last().is_some_and(|segment| segment == "new" || segment == "default")
}

pub(crate) fn source_roots(root: &Path, manifest: &toml::Value) -> Result<Vec<PathBuf>, ShadowError> {
    let explicit_lib = manifest.get("lib").and_then(|lib| lib.get("path")).and_then(toml::Value::as_str);
    let default_lib = root.join("src/lib.rs");
    if explicit_lib.is_some() || default_lib.is_file() {
        return Ok(vec![root.join(explicit_lib.unwrap_or("src/lib.rs"))]);
    }
    let mut bins = manifest
        .get("bin")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|bin| bin.get("path").and_then(toml::Value::as_str))
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    let default_bin = root.join("src/main.rs");
    if default_bin.is_file() && !bins.contains(&default_bin) {
        bins.push(default_bin);
    }
    bins.sort_unstable();
    if bins.is_empty() {
        return Err(ShadowError::new(ShadowErrorKind::Manifest, "package has no Rust lib or bin source root"));
    }
    Ok(bins)
}

pub(crate) fn resolve_module_file_for_transform(
    root: &Path,
    parent_file: &Path,
    module_directory: &Path,
    item: &syn::ItemMod,
) -> Result<PathBuf, ShadowError> {
    resolve_module_file(
        root,
        parent_file,
        module_directory,
        item,
        SourceSpan::new(parent_file.to_owned(), 0, 0),
    )
}

fn resolve_module_file(
    root: &Path,
    parent_file: &Path,
    module_directory: &Path,
    item: &syn::ItemMod,
    span: SourceSpan,
) -> Result<PathBuf, ShadowError> {
    if let Some(path) = path_attribute(item)? {
        let candidate = parent_file.parent().expect("module file has parent").join(path);
        let canonical = fs::canonicalize(&candidate).map_err(|error| {
            ShadowError::new(ShadowErrorKind::MalformedSource, format!("failed to resolve {}: {error}", candidate.display())).at(span.clone())
        })?;
        if !canonical.starts_with(root) {
            return Err(ShadowError::new(ShadowErrorKind::PathEscape, "#[path] module escapes shadow root").at(span));
        }
        return Ok(canonical);
    }
    let name = item.ident.to_string();
    let flat = module_directory.join(format!("{name}.rs"));
    let nested = module_directory.join(&name).join("mod.rs");
    match (flat.is_file(), nested.is_file()) {
        (true, false) => Ok(flat),
        (false, true) => Ok(nested),
        (true, true) => Err(ShadowError::new(
            ShadowErrorKind::DuplicateModule,
            format!("module {name} has both {} and {}", flat.display(), nested.display()),
        ).at(span)),
        (false, false) => Err(ShadowError::new(
            ShadowErrorKind::MalformedSource,
            format!("module {name} has no source file"),
        ).at(span)),
    }
}

fn path_attribute(item: &syn::ItemMod) -> Result<Option<String>, ShadowError> {
    for attribute in &item.attrs {
        if attribute.path().is_ident("path") {
            let value = attribute.parse_args::<syn::LitStr>().or_else(|_| {
                match &attribute.meta {
                    syn::Meta::NameValue(name_value) => match &name_value.value {
                        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(value), .. }) => Ok(value.clone()),
                        _ => Err(syn::Error::new_spanned(&name_value.value, "path must be a string")),
                    },
                    _ => Err(syn::Error::new_spanned(attribute, "path must be a string")),
                }
            }).map_err(|error| ShadowError::new(ShadowErrorKind::MalformedSource, error.to_string()))?;
            return Ok(Some(value.value()));
        }
    }
    Ok(None)
}

fn collect_block_aliases(
    block: &syn::Block,
    module: &[String],
    aliases: &mut BTreeMap<String, Vec<String>>,
) {
    for statement in &block.stmts {
        if let syn::Stmt::Item(syn::Item::Use(item_use)) = statement {
            collect_use_aliases(&item_use.tree, Vec::new(), module, aliases);
        }
    }
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
    if path.first().is_some_and(|segment| segment == "crate") {
        path
    } else if path.first().is_some_and(|segment| segment == "self") {
        module.iter().cloned().chain(path.into_iter().skip(1)).collect()
    } else {
        path
    }
}

fn is_aimer_main(function: &syn::ItemFn) -> bool {
    function.attrs.iter().any(|attribute| {
        let segments = attribute.path().segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>();
        segments == ["aimer", "main"]
    })
}

fn module_directory(file: &Path) -> PathBuf {
    let parent = file.parent().expect("Rust source has a parent");
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => parent.to_owned(),
        _ => parent.join(file.file_stem().expect("Rust source has a stem")),
    }
}

fn function_key(module: &[String], name: &str) -> String {
    format!("{}::{name}", module.join("::"))
}

fn function_span(function: &Function) -> SourceSpan {
    span_for_name(&function.file, &function.source, &function.item.sig.ident.to_string())
}

fn span_for_name(file: &Path, source: &str, name: &str) -> SourceSpan {
    let start = source.find(name).unwrap_or(0);
    SourceSpan::new(file.to_owned(), start, start.saturating_add(name.len()).min(source.len()))
}

fn span_for_tokens(file: &Path, source: &str, tokens: &str) -> SourceSpan {
    let (compact_source, offsets) = compact_with_offsets(source);
    let compact_tokens = tokens.chars().filter(|character| !character.is_whitespace()).collect::<String>();
    if let Some(start) = compact_source.find(&compact_tokens) {
        let end = start + compact_tokens.len();
        return SourceSpan::new(
            file.to_owned(),
            offsets.get(start).copied().unwrap_or(0),
            offsets.get(end).copied().unwrap_or(source.len()),
        );
    }
    SourceSpan::new(file.to_owned(), 0, source.len())
}

fn compact_with_offsets(source: &str) -> (String, Vec<usize>) {
    let mut compact = String::with_capacity(source.len());
    let mut offsets = Vec::with_capacity(source.len() + 1);
    for (offset, character) in source.char_indices() {
        if !character.is_whitespace() {
            offsets.extend(std::iter::repeat_n(offset, character.len_utf8()));
            compact.push(character);
        }
    }
    offsets.push(source.len());
    (compact, offsets)
}