/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![allow(dead_code, unused)]

use std::fs::read_to_string;

use ::intern::intern;
use ::intern::string_key::Intern;
use ::intern::string_key::StringKey;
use common::Diagnostic;
use common::DiagnosticsResult;
use common::Location;
use common::ScalarName;
use common::SourceLocationKey;
use common::Span;
use common::WithLocation;
use docblock_shared::ResolverSourceHash;
use docblock_shared::DEPRECATED_FIELD;
use docblock_syntax::parse_docblock;
use docblock_syntax::DocblockAST;
use docblock_syntax::DocblockSection;
use errors::try_all;
use fnv::FnvBuildHasher;
use graphql_ir::FragmentDefinitionName;
use graphql_syntax::ConstantArgument;
use graphql_syntax::ConstantDirective;
use graphql_syntax::ConstantValue;
use graphql_syntax::ExecutableDefinition;
use graphql_syntax::FieldDefinition;
use graphql_syntax::Identifier;
use graphql_syntax::InputValueDefinition;
use graphql_syntax::IntNode;
use graphql_syntax::List;
use graphql_syntax::ListTypeAnnotation;
use graphql_syntax::NamedTypeAnnotation;
use graphql_syntax::NonNullTypeAnnotation;
use graphql_syntax::StringNode;
use graphql_syntax::Token;
use graphql_syntax::TokenKind;
use graphql_syntax::TypeAnnotation;
use hermes_estree::SourceRange;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use relay_config::CustomType;
use relay_config::CustomTypeImport;
use relay_docblock::Argument;
use relay_docblock::DocblockIr;
use relay_docblock::IrField;
use relay_docblock::PopulatedIrField;
use relay_docblock::ResolverTypeDocblockIr;
use relay_docblock::StrongObjectIr;
use relay_docblock::TerseRelayResolverIr;
use relay_docblock::UnpopulatedIrField;
use relay_docblock::WeakObjectIr;
use rustc_hash::FxHashMap;
use swc_common::comments::Comments;
use swc_common::source_map::SmallPos;
use swc_common::sync::Lrc;
use swc_common::BytePos;
use swc_common::Spanned;

use crate::errors::SchemaGenerationError;
use crate::find_resolver_imports::ImportExportVisitor;
use crate::find_resolver_imports::JSImportType;
use crate::find_resolver_imports::ModuleResolution;
use crate::find_resolver_imports::ModuleResolutionKey;
use crate::get_deprecated;
use crate::get_description;
use crate::invert_custom_scalar_map;
use crate::FnvIndexMap;
use crate::RelayResolverExtractor;
use crate::ResolverFlowData;

pub struct TSRelayResolverExtractor {
    /// Cross module states
    type_definitions: FxHashMap<ModuleResolutionKey, DocblockIr>,
    unresolved_field_definitions: Vec<(UnresolvedTSFieldDefinition, SourceLocationKey)>,
    resolved_field_definitions: Vec<TerseRelayResolverIr>,
    module_resolutions: FxHashMap<SourceLocationKey, ModuleResolution>,

    // Needs to keep track of source location because hermes_parser currently
    // does not embed the information
    current_location: SourceLocationKey,

    // Used to map Flow types in return/argument types to GraphQL custom scalars
    custom_scalar_map: FnvIndexMap<CustomType, ScalarName>,
}

#[allow(dead_code)]
struct UnresolvedTSFieldDefinition {
    entity_name: Option<WithLocation<StringKey>>,
    field_name: WithLocation<StringKey>,
    return_type: swc_ecma_ast::TsType,
    arguments: Option<Vec<swc_ecma_ast::Param>>,
    source_hash: ResolverSourceHash,
    is_live: Option<Location>,
    description: Option<WithLocation<StringKey>>,
    deprecated: Option<IrField>,
    root_fragment: Option<(WithLocation<FragmentDefinitionName>, Vec<Argument>)>,
    entity_type: Option<WithLocation<StringKey>>,
}

impl Default for TSRelayResolverExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl TSRelayResolverExtractor {
    pub fn new() -> Self {
        Self {
            type_definitions: Default::default(),
            unresolved_field_definitions: Default::default(),
            resolved_field_definitions: vec![],
            module_resolutions: Default::default(),
            current_location: SourceLocationKey::generated(),
            custom_scalar_map: FnvIndexMap::default(),
        }
    }

    pub fn extract_function(
        &self,
        _node: &swc_ecma_ast::FnDecl,
    ) -> DiagnosticsResult<ResolverFlowData> {
        todo!()
    }
}

impl RelayResolverExtractor for TSRelayResolverExtractor {
    fn set_custom_scalar_map(
        &mut self,
        custom_scalar_types: &FnvIndexMap<ScalarName, CustomType>,
    ) -> DiagnosticsResult<()> {
        self.custom_scalar_map = invert_custom_scalar_map(custom_scalar_types)?;
        Ok(())
    }

    #[allow(dead_code, unused)]
    fn parse_document(
        &mut self,
        text: &str,
        source_module_path: &str,
        fragment_definitions: Option<&Vec<ExecutableDefinition>>,
    ) -> DiagnosticsResult<()> {
        // Assume the caller knows the text contains at least one RelayResolver decorator
        self.current_location = SourceLocationKey::standalone(source_module_path);
        let source_hash = ResolverSourceHash::new(text);
        let mut errors = Vec::new();
        let comments = swc_common::comments::SingleThreadedComments::default();
        let path_lrc = Lrc::new(swc_common::FileName::Custom(text.to_string()));
        let source = swc_common::SourceFile::new(
            path_lrc.clone(),
            false,
            path_lrc.clone(),
            text.to_string(),
            BytePos::from_usize(text.len()),
        );
        let parsed_module = swc_ecma_parser::parse_file_as_program(
            &source,
            swc_ecma_parser::Syntax::Typescript(swc_ecma_parser::TsSyntax::default()),
            swc_ecma_ast::EsVersion::EsNext,
            Some(&comments),
            &mut errors,
        )
        .map_err(|err| {
            let error = err.kind();
            let span = err.span();
            Diagnostic::error(
                error.msg(),
                Location::new(self.current_location, to_relay_span(span)),
            )
        })?
        .expect_module();

        let module_resolution = extract_module_resolution(&parsed_module, &self.current_location);

        let result = try_all(parsed_module.body.iter().map(|statement| {
            let pos = statement.span().lo();
            if comments.has_leading(pos) {
                let pos_comments = comments.get_leading(pos).unwrap();
                let comment_span = pos_comments
                    .first()
                    .unwrap()
                    .span
                    .between(pos_comments.last().unwrap().span);
                let full_comment = pos_comments
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<&str>>()
                    .join("\n");
                if full_comment.contains("@RelayResolver") {
                    let docblock = parse_docblock(&full_comment, self.current_location)?;
                    let resolver_value = docblock.find_field(intern!("RelayResolver")).unwrap();

                    let deprecated = get_deprecated(&docblock);
                    let description = get_description(
                        &docblock,
                        SourceRange {
                            start: comment_span.lo().to_u32(),
                            end: comment_span.hi().to_u32(),
                        },
                    )?;
                    self.extract_graphql_types(statement);
                }
            }
            Ok(())
        }));

        Ok(())
    }

    fn resolve(self) -> DiagnosticsResult<(Vec<DocblockIr>, Vec<TerseRelayResolverIr>)> {
        Ok((Vec::new(), Vec::new()))
    }
}

fn extract_module_resolution(
    module: &swc_ecma_ast::Module,
    source_location: &SourceLocationKey,
) -> ModuleResolution {
    let mut imports = FxHashMap::default();
    let mut exports = FxHashMap::default();
    let current_module = match source_location {
        SourceLocationKey::Embedded { path, .. } => path,
        SourceLocationKey::Standalone { path } => path,
        SourceLocationKey::Generated => {
            panic!("Generated modules aren't supported in relay live resolver generator")
        }
    };

    module.body.iter().for_each(|item| match item {
        swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::Import(import_decl)) => {
            let source = import_decl.src.value.to_string().intern();
            imports.extend(
                import_decl
                    .specifiers
                    .iter()
                    .map(|specifier| match specifier {
                        swc_ecma_ast::ImportSpecifier::Named(node) => {
                            let name = node.local.sym.as_str().intern();
                            (
                                name,
                                ModuleResolutionKey {
                                    module_name: source,
                                    import_type: JSImportType::Named(
                                        node.imported
                                            .as_ref()
                                            .map(|n| n.atom().as_str().intern())
                                            .unwrap_or(name),
                                    ),
                                },
                            )
                        }
                        swc_ecma_ast::ImportSpecifier::Default(node) => (
                            node.local.sym.as_str().intern(),
                            ModuleResolutionKey {
                                module_name: source,
                                import_type: JSImportType::Default,
                            },
                        ),
                        swc_ecma_ast::ImportSpecifier::Namespace(node) => (
                            node.local.sym.as_str().intern(),
                            ModuleResolutionKey {
                                module_name: source,
                                import_type: JSImportType::Namespace(Location::new(
                                    source_location.clone(),
                                    to_relay_span(node.span),
                                )),
                            },
                        ),
                    }),
            )
        }
        swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(export_decl)) => {
            if let swc_ecma_ast::Decl::TsTypeAlias(node) = &export_decl.decl {
                let name = node.id.sym.as_str().intern();
                exports.insert(
                    name,
                    ModuleResolutionKey {
                        module_name: current_module.clone(),
                        import_type: JSImportType::Named(name),
                    },
                );
            }
        }
        _ => {}
    });

    ModuleResolution { imports, exports }
}

fn to_relay_span(span: swc_common::Span) -> Span {
    Span::new(span.lo().to_u32(), span.hi().to_u32())
}
