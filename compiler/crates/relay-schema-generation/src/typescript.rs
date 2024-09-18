/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![allow(dead_code, unused)]

use std::fs::read_to_string;
use std::primitive;

use ::intern::intern;
use ::intern::string_key::Intern;
use ::intern::string_key::StringKey;
use ::intern::Lookup;
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
use schema_extractor::SchemaExtractor;
use swc_common::comments::Comments;
use swc_common::source_map::SmallPos;
use swc_common::sync::Lrc;
use swc_common::BytePos;
use swc_common::Spanned;
use swc_ecma_ast::ModuleItem;
use swc_ecma_ast::TsKeywordTypeKind;
use swc_ecma_ast::TsType;
use swc_ecma_ast::TsTypeAnn;

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

/**
 * Reprensents a subset of supported Flow type definitions
 */
#[derive(Debug)]
pub enum ResolverTypescriptData {
    Strong(FieldData), // strong object or field on an object
    Weak(WeakObjectData),
}

#[derive(Debug)]
pub struct FieldData {
    pub field_name: WithLocation<StringKey>,
    pub return_type: TsTypeAnn,
    pub entity_type: Option<TsTypeAnn>,
    pub arguments: Option<TsTypeAnn>,
    pub is_live: Option<Location>,
}

#[derive(Debug)]
pub struct WeakObjectData {
    pub field_name: WithLocation<StringKey>,
    pub type_alias: TsTypeAnn,
}

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
        node: &swc_ecma_ast::FnDecl,
    ) -> DiagnosticsResult<ResolverTypescriptData> {
        let ident = node.ident.sym.as_str();

        let field_name = WithLocation {
            item: ident.intern(),
            location: Location::new(self.current_location.clone(), to_relay_span(node.span())),
        };

        let return_type_annotation = node.function.return_type.as_ref().ok_or_else(|| {
            Diagnostic::error(
                SchemaGenerationError::MissingReturnType,
                Location::new(self.current_location.clone(), to_relay_span(node.span())),
            )
        })?;

        let (return_type_with_live, is_optional) = unwrap_nullable_type(return_type_annotation);

        let current_location =
            Location::new(self.current_location.clone(), to_relay_span(node.span()));

        let return_type = get_return_type(*return_type_with_live, current_location)?;

        let entity_type = {
            if node.function.params.is_empty() {
                None
            } else {
                let param = &node.function.params[0].pat;

                if let swc_ecma_ast::Pat::Ident(ident) = param {
                    let type_annotation = ident
                        .type_ann
                        .as_ref()
                        .ok_or_else(|| {
                            Diagnostic::error(
                                SchemaGenerationError::MissingParamType,
                                Location::new(
                                    self.current_location.clone(),
                                    to_relay_span(ident.span()),
                                ),
                            )
                        })?
                        .clone();

                    Some(*type_annotation)
                } else {
                    let printed_param = swc_ecma_codegen::to_code(param);

                    return Err(vec![Diagnostic::error(
                        SchemaGenerationError::UnsupportedType {
                            name: &printed_param.intern().lookup(),
                        },
                        Location::new(self.current_location.clone(), to_relay_span(node.span())),
                    )]);
                }
            }
        };

        Ok(ResolverTypescriptData::Strong(FieldData {
            field_name,
            return_type,
            entity_type,
            arguments: None,
            is_live: None,
        }))
    }

    fn extract_type_alias(
        &self,
        node: &swc_ecma_ast::TsTypeAliasDecl,
    ) -> DiagnosticsResult<WeakObjectData> {
        let field_name = WithLocation {
            item: (&node.id.sym.as_str()).intern(),
            location: Location::new(self.current_location, to_relay_span(node.span())),
        };
        Ok(WeakObjectData {
            field_name,
            type_alias: node.type_ann.as_ref().clone(),
        })
    }

    fn extract_entity_name(
        &self,
        entity_type: swc_ecma_ast::TsType,
    ) -> DiagnosticsResult<WithLocation<StringKey>> {
        match entity_type {
            TsType::TsKeywordType(TsKeywordTypeKind::TsNumberKeyword) => Ok(WithLocation {
                item: intern!("Float"),
                location: self.to_location(annot.as_ref()),
            }),
            TsType::StringTypeAnnotation(annot) => Ok(WithLocation {
                item: intern!("String"),
                location: self.to_location(annot.as_ref()),
            }),
            TsType::GenericTypeAnnotation(annot) => {
                let id = schema_extractor::get_identifier_for_flow_generic(WithLocation {
                    item: &annot,
                    location: self.to_location(annot.as_ref()),
                })?;
                if annot.type_parameters.is_some() {
                    return Err(vec![Diagnostic::error(
                        SchemaGenerationError::GenericNotSupported,
                        self.to_location(annot.as_ref()),
                    )]);
                }
                Ok(id)
            }
            TsType::NullableTypeAnnotation(annot) => Err(vec![Diagnostic::error(
                SchemaGenerationError::UnexpectedNullableStrongType,
                self.to_location(annot.as_ref()),
            )]),
            _ => Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: entity_type.name(),
                },
                self.to_location(&entity_type),
            )]),
        }
    }

    fn extract_graphql_types(
        &self,
        node: &swc_ecma_ast::ModuleItem,
        range: SourceRange,
    ) -> DiagnosticsResult<ResolverTypescriptData> {
        if let swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(
            ref node,
        )) = node
        {
            match &node.decl {
                swc_ecma_ast::Decl::Fn(fn_node) => self.extract_function(fn_node),
                swc_ecma_ast::Decl::TsTypeAlias(alias_node) => {
                    let data = self.extract_type_alias(alias_node)?;
                    Ok(ResolverTypescriptData::Weak(data))
                }
                _ => Err(vec![Diagnostic::error(
                    SchemaGenerationError::ExpectedFunctionOrTypeAlias,
                    Location::new(self.current_location, Span::new(range.start, range.end)),
                )]),
            }
        } else {
            Err(vec![Diagnostic::error(
                SchemaGenerationError::ExpectedNamedExport,
                Location::new(self.current_location, Span::new(range.start, range.end)),
            )])
        }
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
                    match self.extract_graphql_types(
                        statement,
                        SourceRange {
                            start: comment_span.lo().to_u32(),
                            end: statement.span().hi().to_u32(),
                        },
                    )? {
                        ResolverTypescriptData::Strong(FieldData {
                            field_name,
                            return_type,
                            entity_type,
                            arguments,
                            is_live,
                        }) => {
                            let name = resolver_value.field_value.unwrap_or(field_name);

                            // Heuristic to treat lowercase name as field definition, otherwise object definition
                            // if there is a `.` in the name, it is the old resolver synatx, e.g. @RelayResolver Client.field,
                            // we should treat it as a field definition
                            let is_field_definition = {
                                let name_str = name.item.lookup();
                                let is_lowercase_initial =
                                    name_str.chars().next().unwrap().is_lowercase();
                                is_lowercase_initial || name_str.contains('.')
                            };
                            if is_field_definition {
                                let entity_name = match entity_type {
                                    Some(entity_type) => {
                                        Some(self.extract_entity_name(entity_type)?)
                                    }
                                    None => None,
                                };

                                self.add_field_definition(
                                    &module_resolution,
                                    fragment_definitions,
                                    UnresolvedFieldDefinition {
                                        entity_name,
                                        field_name: name,
                                        return_type,
                                        arguments,
                                        source_hash,
                                        is_live,
                                        description,
                                        deprecated,
                                        root_fragment: None,
                                        entity_type: None,
                                    },
                                )?
                            } else {
                                self.add_type_definition(
                                    &module_resolution,
                                    name,
                                    return_type,
                                    source_hash,
                                    is_live,
                                    description,
                                )?
                            }
                        }
                        ResolverTypescriptData::Weak(WeakObjectData {
                            field_name,
                            type_alias,
                        }) => {
                            let name = resolver_value.field_value.unwrap_or(field_name);
                            self.add_weak_type_definition(
                                name,
                                type_alias,
                                source_hash,
                                source_module_path,
                                description,
                                false,
                            )?
                        }
                    }
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

fn unsupported(name: &str, current_location: Location) -> DiagnosticsResult<TsTypeAnn> {
    let name = name.to_string().intern();
    Err(vec![Diagnostic::error(
        SchemaGenerationError::UnsupportedType {
            name: name.lookup(),
        },
        current_location,
    )])
}

fn get_return_type(
    return_type_with_live: TsTypeAnn,
    current_location: Location,
) -> DiagnosticsResult<TsTypeAnn> {
    let span = return_type_with_live.type_ann.span();
    let type_ann = *return_type_with_live.clone().type_ann;

    let primitive_type: DiagnosticsResult<TsTypeAnn> = match type_ann {
        TsType::TsKeywordType(ts_keyword_type) => match ts_keyword_type.kind {
            swc_ecma_ast::TsKeywordTypeKind::TsBooleanKeyword => Ok(return_type_with_live),
            swc_ecma_ast::TsKeywordTypeKind::TsNumberKeyword => Ok(return_type_with_live),
            swc_ecma_ast::TsKeywordTypeKind::TsStringKeyword => Ok(return_type_with_live),
            _ => unsupported("Unsupported type", current_location),
        },
        TsType::TsTypeRef(ts_type_ref) => {
            // We only support type references with one type parameter
            if ts_type_ref
                .type_params
                .is_some_and(|params| params.params.len() > 1)
            {
                unsupported("Unsupported type", current_location)
            } else {
                Ok(return_type_with_live)
            }
        }
        _ => unsupported("Unsupported type", current_location),
    };

    primitive_type
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

fn unwrap_nullable_type(
    type_ann: &Box<swc_ecma_ast::TsTypeAnn>,
) -> (Box<swc_ecma_ast::TsTypeAnn>, bool) {
    let mut optional = false;
    let return_type = &type_ann.type_ann;
    let union_type = return_type.as_ts_union_or_intersection_type();

    let union_type = match union_type {
        Some(swc_ecma_ast::TsUnionOrIntersectionType::TsUnionType(ts_type)) => Some(ts_type),
        Some(swc_ecma_ast::TsUnionOrIntersectionType::TsIntersectionType(ts_type)) => {
            // TODO(mapol): Add some diagnostics here?
            // Perhaps should check this before we reach here?
            panic!("Intersection types are not supported in Relay Resolver")
        }
        None => None,
    };

    match union_type {
        Some(ts_type) => {
            // Check if this is a union with `null` and/or `undefined`
            let is_required = ts_type
                .types
                .iter()
                .filter_map(|type_ann| match type_ann.as_ts_keyword_type() {
                    Some(ts_keyword_type) => match ts_keyword_type.kind {
                        swc_ecma_ast::TsKeywordTypeKind::TsNullKeyword => Some(ts_keyword_type),
                        swc_ecma_ast::TsKeywordTypeKind::TsUndefinedKeyword => {
                            Some(ts_keyword_type)
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
                .is_empty();

            let non_optional_type = ts_type.types.iter();

            return (type_ann.clone(), is_required);
        }
        None => {}
    };

    let v = type_ann.clone();

    (v, optional)
}
