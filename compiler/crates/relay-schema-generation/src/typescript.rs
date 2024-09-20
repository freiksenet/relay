/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::hash_map::Entry;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

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
use docblock_syntax::parse_docblock;
use errors::try_all;
use graphql_ir::FragmentDefinitionName;
use graphql_syntax::ExecutableDefinition;
use graphql_syntax::FieldDefinition;
use graphql_syntax::Identifier;
use graphql_syntax::InputValueDefinition;
use graphql_syntax::List;
use graphql_syntax::ListTypeAnnotation;
use graphql_syntax::NamedTypeAnnotation;
use graphql_syntax::NonNullTypeAnnotation;
use graphql_syntax::StringNode;
use graphql_syntax::Token;
use graphql_syntax::TokenKind;
use graphql_syntax::TypeAnnotation;
use hermes_estree::SourceRange;
use relay_config::CustomType;
use relay_config::CustomTypeImport;
use relay_docblock::Argument;
use relay_docblock::DocblockIr;
use relay_docblock::IrField;
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
use swc_common::SourceFile;
use swc_common::SourceMap;
use swc_common::Spanned;
use swc_ecma_ast::Expr;
use swc_ecma_ast::TsEntityName;
use swc_ecma_ast::TsKeywordType;
use swc_ecma_ast::TsKeywordTypeKind;
use swc_ecma_ast::TsLit;
use swc_ecma_ast::TsLitType;
use swc_ecma_ast::TsType;
use swc_ecma_ast::TsTypeElement;
use swc_ecma_ast::TsTypeLit;

use crate::errors::SchemaGenerationError;
use crate::find_resolver_imports::JSImportType;
use crate::find_resolver_imports::ModuleResolution;
use crate::find_resolver_imports::ModuleResolutionKey;
use crate::generated_token;
use crate::get_deprecated;
use crate::get_description;
use crate::invert_custom_scalar_map;
use crate::semantic_non_null_levels_to_directive;
use crate::string_key_to_identifier;
use crate::typescript_extract;
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
    pub return_type: TsType,
    pub entity_type: Option<TsType>,
    pub arguments: Option<TsType>,
    pub is_live: Option<Location>,
}

#[derive(Debug)]
pub struct WeakObjectData {
    pub field_name: WithLocation<StringKey>,
    pub type_alias: TsType,
}

pub struct TSRelayResolverExtractor {
    /// Cross module states
    type_definitions: FxHashMap<ModuleResolutionKey, DocblockIr>,
    unresolved_field_definitions: Vec<(UnresolvedTSFieldDefinition, SourceLocationKey)>,
    resolved_field_definitions: Vec<TerseRelayResolverIr>,
    module_resolutions: FxHashMap<SourceLocationKey, ModuleResolution>,

    // Used to map Flow types in return/argument types to GraphQL custom scalars
    custom_scalar_map: FnvIndexMap<CustomType, ScalarName>,

    // Need to keep track of source files to map span to location
    location_handler: Option<LocationHandler>,
}

struct UnresolvedTSFieldDefinition {
    entity_name: Option<WithLocation<StringKey>>,
    field_name: WithLocation<StringKey>,
    return_type: swc_ecma_ast::TsType,
    arguments: Option<TsType>,
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
            custom_scalar_map: FnvIndexMap::default(),
            location_handler: None,
        }
    }

    pub fn extract_function(
        &self,
        node: &swc_ecma_ast::FnDecl,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<ResolverTypescriptData> {
        let ident = node.ident.sym.as_str();

        // Field name is the function name
        let field_name = WithLocation {
            item: ident.intern(),
            location: location_handler.to_location(&node.ident),
        };

        let (return_type, is_live) =
            typescript_extract::extract_return_type_from_resolver_function(node, location_handler)?;

        // Entity type is the type of the first argument to the function
        let entity_type =
            typescript_extract::extract_entity_type_from_resolver_function(node, location_handler)?;

        let arguments =
            typescript_extract::extract_params_from_second_argument(node, location_handler)?;

        Ok(ResolverTypescriptData::Strong(FieldData {
            field_name,
            return_type,
            entity_type,
            arguments,
            is_live,
        }))
    }

    fn extract_type_alias(
        &self,
        node: &swc_ecma_ast::TsTypeAliasDecl,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<WeakObjectData> {
        let field_name = WithLocation {
            item: (&node.id.sym.as_str()).intern(),
            location: location_handler.to_location(node),
        };
        Ok(WeakObjectData {
            field_name,
            type_alias: node.type_ann.as_ref().clone(),
        })
    }

    fn extract_entity_name(
        &self,
        entity_type: &swc_ecma_ast::TsType,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<WithLocation<StringKey>> {
        let location = location_handler.to_location(entity_type);
        let result = match entity_type {
            TsType::TsTypeRef(type_ref) => Ok(WithLocation {
                item: get_unqualified_identifier_or_fail(&type_ref.type_name, location)?.item,
                location: location_handler.to_location(&type_ref.type_name),
            }),
            TsType::TsKeywordType(keyword_type) => match keyword_type.kind {
                TsKeywordTypeKind::TsNumberKeyword => Ok(WithLocation {
                    item: intern!("Float"),
                    location: location_handler.to_location(keyword_type),
                }),
                TsKeywordTypeKind::TsStringKeyword => Ok(WithLocation {
                    item: intern!("String"),
                    location: location_handler.to_location(keyword_type),
                }),

                _ => Err(vec![Diagnostic::error(
                    SchemaGenerationError::UnexpectedNullableStrongType,
                    location_handler.to_location(keyword_type),
                )]),
            },
            _ => Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: format!("{:?}", entity_type).intern().lookup(),
                },
                location,
            )]),
        };

        result.map_err(|_e| {
            vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: format!("{:?}", entity_type).leak(),
                },
                location,
            )]
        })
    }

    fn extract_graphql_types(
        &self,
        node: &swc_ecma_ast::ModuleItem,
        range: SourceRange,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<ResolverTypescriptData> {
        if let swc_ecma_ast::ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(
            ref node,
        )) = node
        {
            match &node.decl {
                swc_ecma_ast::Decl::Fn(fn_node) => self.extract_function(fn_node, location_handler),
                swc_ecma_ast::Decl::TsTypeAlias(alias_node) => {
                    let data = self.extract_type_alias(alias_node, location_handler)?;
                    Ok(ResolverTypescriptData::Weak(data))
                }
                _ => Err(vec![Diagnostic::error(
                    SchemaGenerationError::ExpectedFunctionOrTypeAlias,
                    Location::new(
                        location_handler.source_location_key,
                        Span::new(range.start, range.end),
                    ),
                )]),
            }
        } else {
            Err(vec![Diagnostic::error(
                SchemaGenerationError::ExpectedNamedExport,
                Location::new(
                    location_handler.source_location_key,
                    Span::new(range.start, range.end),
                ),
            )])
        }
    }

    fn add_field_definition(
        &mut self,
        module_resolution: &ModuleResolution,
        fragment_definitions: Option<&Vec<ExecutableDefinition>>,
        mut field_definition: UnresolvedTSFieldDefinition,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<()> {
        if let Some(entity_name) = field_definition.entity_name {
            let name = entity_name.item;
            let key = module_resolution.get(name).ok_or_else(|| {
                vec![Diagnostic::error(
                    SchemaGenerationError::ExpectedFlowDefinitionForType { name },
                    entity_name.location,
                )]
            })?;

            if key.module_name.lookup().ends_with(".graphql") && name.lookup().ends_with("$key") {
                let fragment_name = name.lookup().strip_suffix("$key").unwrap();
                let fragment_definition_result = relay_docblock::assert_fragment_definition(
                    entity_name,
                    fragment_name.intern(),
                    fragment_definitions,
                );
                let fragment_definition = fragment_definition_result.map_err(|err| vec![err])?;

                field_definition.entity_type = Some(WithLocation::from_span(
                    fragment_definition.location.source_location(),
                    fragment_definition.type_condition.span,
                    fragment_definition.type_condition.type_.value,
                ));
                let fragment = WithLocation::from_span(
                    fragment_definition.location.source_location(),
                    fragment_definition.name.span,
                    FragmentDefinitionName(fragment_definition.name.value),
                );
                let fragment_arguments =
                    relay_docblock::extract_fragment_arguments(&fragment_definition).transpose()?;
                field_definition.root_fragment =
                    Some((fragment, fragment_arguments.unwrap_or(vec![])));
            }
        }
        self.unresolved_field_definitions
            .push((field_definition, location_handler.source_location_key));

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_type_definition(
        &mut self,
        module_resolution: &ModuleResolution,
        name: WithLocation<StringKey>,
        // TODO: Fix this up
        // mut return_type: TsType,
        return_type: TsType,
        source_hash: ResolverSourceHash,
        is_live: Option<Location>,
        description: Option<WithLocation<StringKey>>,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<()> {
        let strong_object = StrongObjectIr {
            type_name: string_key_to_identifier(name),
            rhs_location: name.location,
            root_fragment: WithLocation::new(
                name.location,
                FragmentDefinitionName(format!("{}__id", name.item).intern()),
            ),
            description,
            deprecated: None,
            live: is_live.map(|loc| UnpopulatedIrField { key_location: loc }),
            location: name.location,
            implements_interfaces: vec![],
            source_hash,
            semantic_non_null: None,
        };

        let location = location_handler.to_location(&return_type);

        // // We ignore nullable annotation since both nullable and non-nullable types are okay for
        // // defining a strong object
        // return_type = if let FlowTypeAnnotation::NullableTypeAnnotation(return_type) = return_type {
        //     return_type.type_annotation
        // } else {
        //     return_type
        // };
        // For now, we assume the flow type for the strong object is always imported
        // from a separate file
        match return_type {
            TsType::TsTypeRef(type_ref) => {
                let name = get_unqualified_identifier_or_fail(&type_ref.type_name, location)?;

                let key = module_resolution.get(name.item).ok_or_else(|| {
                    vec![Diagnostic::error(
                        SchemaGenerationError::ExpectedFlowDefinitionForType { name: name.item },
                        name.location,
                    )]
                })?;
                if let JSImportType::Namespace(import_location) = key.import_type {
                    return Err(vec![Diagnostic::error(
                        SchemaGenerationError::UseNamedOrDefaultImport,
                        name.location,
                    )
                    .annotate(format!("{} is imported from", name.item), import_location)]);
                };

                self.insert_type_definition(
                    key.clone(),
                    DocblockIr::Type(ResolverTypeDocblockIr::StrongObjectResolver(strong_object)),
                )
            }
            TsType::TsTypeLit(_) => Err(vec![Diagnostic::error(
                SchemaGenerationError::ObjectNotSupported,
                location,
            )]),
            _ => Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: format!("{:?}", return_type).leak(),
                },
                location,
            )]),
        }
    }

    fn add_weak_type_definition(
        &mut self,
        name: WithLocation<StringKey>,
        type_alias: TsType,
        source_hash: ResolverSourceHash,
        source_module_path: &str,
        description: Option<WithLocation<StringKey>>,
        should_generate_fields: bool,
        location_handler: &LocationHandler,
    ) -> DiagnosticsResult<()> {
        let location = location_handler.to_location(&type_alias);
        let weak_object = WeakObjectIr {
            type_name: string_key_to_identifier(name),
            rhs_location: name.location,
            description,
            hack_source: None,
            deprecated: None,
            location: name.location,
            implements_interfaces: vec![],
            source_hash,
        };
        let haste_module_name = Path::new(source_module_path)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();
        let key = ModuleResolutionKey {
            module_name: haste_module_name.intern(),
            import_type: JSImportType::Named(name.item),
        };

        // TODO: this generates the IR but not the runtime JS
        if should_generate_fields {
            if let TsType::TsTypeLit(object_node) = type_alias {
                let field_map = get_object_fields(&object_node, location)?;
                if !field_map.is_empty() {
                    try_all(field_map.into_iter().map(|(field_name, field_type)| {
                        self.unresolved_field_definitions.push((
                            UnresolvedTSFieldDefinition {
                                entity_name: Some(name),
                                field_name,
                                return_type: field_type.clone(),
                                arguments: None,
                                source_hash,
                                is_live: None,
                                description,
                                deprecated: None,
                                root_fragment: None,
                                entity_type: Some(
                                    weak_object
                                        .type_name
                                        .name_with_location(weak_object.location.source_location()),
                                ),
                            },
                            location_handler.source_location_key,
                        ));
                        Ok(())
                    }))?;
                } else {
                    return Err(vec![Diagnostic::error(
                        SchemaGenerationError::ExpectedWeakObjectToHaveFields,
                        location,
                    )]);
                }
            } else {
                return Err(vec![Diagnostic::error(
                    SchemaGenerationError::ExpectedTypeAliasToBeObject,
                    location,
                )]);
            }
        }

        // Add weak object
        self.insert_type_definition(
            key,
            DocblockIr::Type(ResolverTypeDocblockIr::WeakObjectType(weak_object)),
        )
    }

    fn insert_type_definition(
        &mut self,
        key: ModuleResolutionKey,
        data: DocblockIr,
    ) -> DiagnosticsResult<()> {
        match self.type_definitions.entry(key) {
            Entry::Occupied(entry) => Err(vec![Diagnostic::error(
                SchemaGenerationError::DuplicateTypeDefinitions {
                    module_name: entry.key().module_name,
                    import_type: entry.key().import_type,
                },
                data.location(),
            )
            .annotate("Previous type definition", entry.get().location())]),
            Entry::Vacant(entry) => {
                entry.insert(data);
                Ok(())
            }
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

    fn parse_document(
        &mut self,
        text: &str,
        source_module_path: &str,
        fragment_definitions: Option<&Vec<ExecutableDefinition>>,
    ) -> DiagnosticsResult<()> {
        // Assume the caller knows the text contains at least one RelayResolver decorator
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

        let location_handler: LocationHandler =
            LocationHandler::new(&source, SourceLocationKey::standalone(source_module_path));

        let parsed_module = swc_ecma_parser::parse_file_as_module(
            &source,
            swc_ecma_parser::Syntax::Typescript(swc_ecma_parser::TsSyntax::default()),
            swc_ecma_ast::EsVersion::EsNext,
            Some(&comments),
            &mut errors,
        )
        .map_err(|err| {
            let error = err.kind();
            let span = err.span();
            Diagnostic::error(error.msg(), location_handler.to_location(&span))
        })?;

        let module_resolution = extract_module_resolution(
            &parsed_module,
            &location_handler.source_location_key,
            |span| location_handler.to_location(span),
        );

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
                    let docblock =
                        parse_docblock(&full_comment, location_handler.source_location_key)?;
                    let resolver_value = docblock.find_field(intern!("RelayResolver")).unwrap();

                    let deprecated = get_deprecated(&docblock);
                    let description = get_description(
                        &docblock,
                        // TODO: Check if these should be characters pos?
                        SourceRange {
                            start: comment_span.lo().to_u32(),
                            end: comment_span.hi().to_u32(),
                        },
                    )?;

                    let extracted_graphql = self.extract_graphql_types(
                        statement,
                        // TODO: Check if these should be characters pos?
                        SourceRange {
                            start: comment_span.lo().to_u32(),
                            end: statement.span().hi().to_u32(),
                        },
                        &location_handler,
                    )?;

                    match extracted_graphql {
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
                                    Some(entity_type) => Some(
                                        self.extract_entity_name(&entity_type, &location_handler)?,
                                    ),
                                    None => None,
                                };

                                self.add_field_definition(
                                    &module_resolution,
                                    fragment_definitions,
                                    UnresolvedTSFieldDefinition {
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
                                    &location_handler,
                                )?
                            } else {
                                self.add_type_definition(
                                    &module_resolution,
                                    name,
                                    return_type,
                                    source_hash,
                                    is_live,
                                    description,
                                    &location_handler,
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
                                &location_handler,
                            )?
                        }
                    }
                }
            }
            Ok(())
        }));

        self.module_resolutions
            .insert(location_handler.source_location_key, module_resolution);

        // Funkiness that this needs to be set up before we run .resolve(),
        // but we can only set it up after we've parsed the module
        self.location_handler = Some(location_handler);

        result?;

        Ok(())
    }

    fn resolve(mut self) -> DiagnosticsResult<(Vec<DocblockIr>, Vec<TerseRelayResolverIr>)> {
        let location_handler = self
            .location_handler
            .expect("Expected location handler to exist");

        try_all(
            self.unresolved_field_definitions
                .into_iter()
                .map(|(field, source_location)| {
                    let module_resolution = self
                        .module_resolutions
                        .get(&source_location)
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                SchemaGenerationError::UnexpectedFailedToFindModuleResolution {
                                    path: source_location.path(),
                                },
                                field.field_name.location,
                            )]
                        })?;

                    let type_ = if let Some(entity_type) = field.entity_type {
                        entity_type
                    } else if let Some(entity_name) = field.entity_name {
                        let key = module_resolution.get(entity_name.item).ok_or_else(|| {
                            vec![Diagnostic::error(
                                SchemaGenerationError::ExpectedFlowDefinitionForType {
                                    name: entity_name.item,
                                },
                                entity_name.location,
                            )]
                        })?;
                        match self.type_definitions.get(key) {
                            Some(DocblockIr::Type(
                                ResolverTypeDocblockIr::StrongObjectResolver(object),
                            )) => Ok(object
                                .type_name
                                .name_with_location(object.location.source_location())),
                            Some(DocblockIr::Type(ResolverTypeDocblockIr::WeakObjectType(
                                object,
                            ))) => Ok(object
                                .type_name
                                .name_with_location(object.location.source_location())),
                            _ => Err(vec![Diagnostic::error(
                                SchemaGenerationError::ModuleNotFound {
                                    entity_name: entity_name.item,
                                    export_type: key.import_type,
                                    module_name: key.module_name,
                                },
                                entity_name.location,
                            )]),
                        }?
                    } else {
                        // Special case: we attach the field to the `Query` type when there is no entity
                        WithLocation::new(field.field_name.location, intern!("Query"))
                    };
                    let arguments = if let Some(args) = &field.arguments {
                        Some(ts_type_to_field_arguments(
                            &self.custom_scalar_map,
                            &args,
                            module_resolution,
                            &self.type_definitions,
                            &location_handler,
                        )?)
                    } else {
                        None
                    };
                    if let (Some(field_arguments), Some((root_fragment, fragment_arguments))) =
                        (&arguments, &field.root_fragment)
                    {
                        relay_docblock::validate_fragment_arguments(
                            location_handler.source_location_key,
                            field_arguments,
                            root_fragment.location.source_location(),
                            fragment_arguments,
                        )?;
                    }
                    let description_node = field.description.map(|desc| StringNode {
                        token: Token {
                            span: desc.location.span(),
                            kind: TokenKind::Empty,
                        },
                        value: desc.item,
                    });
                    let (type_annotation, semantic_non_null_levels) =
                        return_type_to_type_annotation(
                            &self.custom_scalar_map,
                            &field.return_type,
                            module_resolution,
                            &self.type_definitions,
                            true,
                            &location_handler,
                        )?;
                    let field_definition = FieldDefinition {
                        name: string_key_to_identifier(field.field_name),
                        type_: type_annotation,
                        arguments,
                        directives: vec![],
                        description: description_node,
                        hack_source: None,
                        span: field.field_name.location.span(),
                    };
                    let live = field
                        .is_live
                        .map(|loc| UnpopulatedIrField { key_location: loc });
                    let (root_fragment, fragment_arguments) = field.root_fragment.clone().unzip();
                    self.resolved_field_definitions.push(TerseRelayResolverIr {
                        field: field_definition,
                        type_,
                        root_fragment,
                        location: field.field_name.location,
                        deprecated: field.deprecated,
                        live,
                        fragment_arguments,
                        source_hash: field.source_hash,
                        semantic_non_null: semantic_non_null_levels_to_directive(
                            semantic_non_null_levels,
                            field.field_name.location,
                        ),
                    });
                    Ok(())
                }),
        )?;
        Ok((
            self.type_definitions.into_values().collect(),
            self.resolved_field_definitions,
        ))
    }
}

fn extract_module_resolution(
    module: &swc_ecma_ast::Module,
    source_location: &SourceLocationKey,
    to_location: impl Fn(&dyn swc_common::Spanned) -> Location,
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
                                import_type: JSImportType::Namespace(to_location(&node.span)),
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

pub struct LocationHandler {
    source_file: Box<SourceFile>,
    source_map: SourceMap,
    pub source_location_key: SourceLocationKey,
}

impl LocationHandler {
    fn new(source_file: &SourceFile, source_location_key: SourceLocationKey) -> Self {
        Self {
            source_file: Box::new(source_file.clone()),
            source_map: SourceMap::default(),
            source_location_key,
        }
    }

    pub fn to_location<T: Spanned + ?Sized>(&self, node: &T) -> Location {
        let (start, end) = self
            .source_map
            .span_to_char_offset(&self.source_file, node.span());

        return Location::new(self.source_location_key, Span::new(start, end));
    }
}

fn unwrap_nullable_type(
    return_type: &swc_ecma_ast::TsType,
    location_handler: &LocationHandler,
) -> DiagnosticsResult<(swc_ecma_ast::TsType, bool)> {
    let union_type = return_type.as_ts_union_or_intersection_type();

    let union_type = match union_type {
        Some(swc_ecma_ast::TsUnionOrIntersectionType::TsUnionType(ts_type)) => Some(ts_type),
        Some(swc_ecma_ast::TsUnionOrIntersectionType::TsIntersectionType(_)) => {
            return Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: format!("{:?}", return_type).leak(),
                },
                location_handler.to_location(&return_type.span()),
            )]);
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

            let non_optional_type = ts_type
                .types
                .iter()
                .filter(|type_ann| match type_ann.as_ts_keyword_type() {
                    Some(ts_keyword_type) => match ts_keyword_type.kind {
                        swc_ecma_ast::TsKeywordTypeKind::TsNullKeyword => false,
                        swc_ecma_ast::TsKeywordTypeKind::TsUndefinedKeyword => false,
                        _ => true,
                    },
                    _ => true,
                })
                .collect::<Vec<_>>();

            return Ok((
                non_optional_type
                    .first()
                    .unwrap()
                    .to_owned()
                    .as_ref()
                    .clone(),
                !is_required,
            ));
        }
        None => {}
    };

    Ok((return_type.clone(), false))
}

fn get_object_fields(
    node: &TsTypeLit,
    location: Location,
) -> DiagnosticsResult<FxHashMap<WithLocation<StringKey>, TsType>> {
    let mut field_map: FxHashMap<WithLocation<StringKey>, TsType> = FxHashMap::default();
    for property in node.members.iter() {
        if let TsTypeElement::TsPropertySignature(ref prop) = property {
            if let swc_ecma_ast::Expr::Ident(id) = prop.key.as_ref() {
                let name = WithLocation {
                    item: (&id.sym.as_str()).intern(),
                    location,
                };
                field_map.insert(
                    name,
                    prop.type_ann.as_ref().unwrap().type_ann.as_ref().clone(),
                );
            }
        }
    }
    Ok(field_map)
}

fn get_unqualified_identifier_or_fail(
    ident: &TsEntityName,
    location: Location,
) -> DiagnosticsResult<WithLocation<StringKey>> {
    match ident {
        TsEntityName::TsQualifiedName(ts_qualified_name) => Err(vec![Diagnostic::error(
            SchemaGenerationError::UnsupportedType {
                name: ts_qualified_name.right.sym.to_string().leak(),
            },
            location,
        )]),
        TsEntityName::Ident(ident) => Ok(WithLocation {
            item: ident.sym.as_str().intern(),
            location,
        }),
    }
}

// Converts a TS type annotation to a GraphQL type annotation.
/// The second return value is a list of semantic non-null levels.
/// If empty, the value is not semantically non-null.
fn return_type_to_type_annotation(
    custom_scalar_map: &FnvIndexMap<CustomType, ScalarName>,
    return_type: &TsType,
    module_resolution: &ModuleResolution,
    type_definitions: &FxHashMap<ModuleResolutionKey, DocblockIr>,
    use_semantic_non_null: bool,
    location_handler: &LocationHandler,
) -> DiagnosticsResult<(TypeAnnotation, Vec<i64>)> {
    let (return_type, mut is_optional) = unwrap_nullable_type(return_type, location_handler)?;
    let mut semantic_non_null_levels: Vec<i64> = vec![];

    let location = location_handler.to_location(&return_type);
    let type_annotation: TypeAnnotation = match return_type {
        TsType::TsTypeRef(node) => {
            let identifier = get_unqualified_identifier_or_fail(
                &node.type_name,
                location_handler.to_location(&node.type_name),
            )?;
            match &node.type_params {
                None => {
                    let module_key_opt = module_resolution.get(identifier.item);
                    let scalar_key = match module_key_opt {
                        Some(key) => CustomType::Path(CustomTypeImport {
                            name: identifier.item,
                            path: PathBuf::from_str(key.module_name.lookup()).unwrap(),
                        }),
                        None => CustomType::Name(identifier.item),
                    };
                    let custom_scalar = custom_scalar_map.get(&scalar_key);

                    let graphql_typename = match custom_scalar {
                        Some(scalar_name) => identifier.map(|_| scalar_name.0), // map identifer to keep the location
                        None => {
                            // If there is no custom scalar, expect that the Flow type is imported
                            let module_key = module_key_opt.ok_or_else(|| {
                                vec![Diagnostic::error(
                                    SchemaGenerationError::ExpectedFlowDefinitionForType {
                                        name: identifier.item,
                                    },
                                    identifier.location,
                                )]
                            })?;
                            match type_definitions.get(module_key) {
                                Some(DocblockIr::Type(
                                    ResolverTypeDocblockIr::StrongObjectResolver(object),
                                )) => Err(vec![Diagnostic::error(
                                    SchemaGenerationError::StrongReturnTypeNotAllowed {
                                        typename: object.type_name.value,
                                    },
                                    identifier.location,
                                )]),
                                Some(DocblockIr::Type(ResolverTypeDocblockIr::WeakObjectType(
                                    object,
                                ))) => Ok(object
                                    .type_name
                                    .name_with_location(object.location.source_location())),
                                _ => Err(vec![Diagnostic::error(
                                    SchemaGenerationError::ModuleNotFound {
                                        entity_name: identifier.item,
                                        export_type: module_key.import_type,
                                        module_name: module_key.module_name,
                                    },
                                    identifier.location,
                                )]),
                            }?
                        }
                    };

                    TypeAnnotation::Named(NamedTypeAnnotation {
                        name: string_key_to_identifier(graphql_typename),
                    })
                }
                Some(type_parameters) if type_parameters.params.len() == 1 => {
                    let identifier_name = identifier.item.lookup();
                    match identifier_name {
                        "Array" | "ReadOnlyArray" => {
                            let param = &type_parameters.params[0];
                            let (type_annotation, inner_semantic_non_null_levels) =
                                return_type_to_type_annotation(
                                    custom_scalar_map,
                                    param,
                                    module_resolution,
                                    type_definitions,
                                    // use_semantic_non_null is false because a resolver returning an array of
                                    // non-null items doesn't need to express that a single item will be null
                                    // due to error. So, array items can just be regular non-null.
                                    false,
                                    location_handler,
                                )?;

                            // increment each inner level by one
                            semantic_non_null_levels.extend(
                                inner_semantic_non_null_levels.iter().map(|level| level + 1),
                            );

                            TypeAnnotation::List(Box::new(ListTypeAnnotation {
                                span: location.span(),
                                open: generated_token(),
                                type_: type_annotation,
                                close: generated_token(),
                            }))
                        }
                        "IdOf" => {
                            let param = &type_parameters.params[0].as_ref();
                            let location = location_handler.to_location(param);
                            if let TsType::TsLitType(TsLitType {
                                lit: TsLit::Str(str),
                                ..
                            }) = param
                            {
                                TypeAnnotation::Named(NamedTypeAnnotation {
                                    name: Identifier {
                                        span: location.span(),
                                        token: Token {
                                            span: location.span(),
                                            kind: TokenKind::Identifier,
                                        },
                                        value: (&str.value).intern(),
                                    },
                                })
                            } else {
                                return Err(vec![Diagnostic::error(
                                    SchemaGenerationError::Todo,
                                    location,
                                )]);
                            }
                        }
                        "RelayResolverValue" => {
                            // Special case for `RelayResolverValue`, it is always optional
                            is_optional = true;
                            TypeAnnotation::Named(NamedTypeAnnotation {
                                name: Identifier {
                                    span: location.span(),
                                    token: Token {
                                        span: location.span(),
                                        kind: TokenKind::Identifier,
                                    },
                                    value: intern!("RelayResolverValue"),
                                },
                            })
                        }
                        _ => {
                            return Err(vec![Diagnostic::error(
                                SchemaGenerationError::UnSupportedGeneric {
                                    name: identifier.item,
                                },
                                location,
                            )]);
                        }
                    }
                }
                _ => {
                    return Err(vec![Diagnostic::error(
                        SchemaGenerationError::Todo,
                        location,
                    )]);
                }
            }
        }
        TsType::TsKeywordType(
            node @ TsKeywordType {
                kind: TsKeywordTypeKind::TsStringKeyword,
                ..
            },
        ) => {
            let identifier = WithLocation {
                item: intern!("String"),
                location: location_handler.to_location(&node),
            };
            TypeAnnotation::Named(NamedTypeAnnotation {
                name: string_key_to_identifier(identifier),
            })
        }
        TsType::TsKeywordType(
            node @ TsKeywordType {
                kind: TsKeywordTypeKind::TsNumberKeyword,
                ..
            },
        ) => {
            let identifier = WithLocation {
                item: intern!("Float"),
                location: location_handler.to_location(&node),
            };
            TypeAnnotation::Named(NamedTypeAnnotation {
                name: string_key_to_identifier(identifier),
            })
        }
        TsType::TsKeywordType(
            node @ TsKeywordType {
                kind: TsKeywordTypeKind::TsBooleanKeyword,
                ..
            },
        ) => {
            let identifier = WithLocation {
                item: intern!("Boolean"),
                location: location_handler.to_location(&node),
            };
            TypeAnnotation::Named(NamedTypeAnnotation {
                name: string_key_to_identifier(identifier),
            })
        }
        TsType::TsLitType(
            node @ TsLitType {
                lit: TsLit::Bool(_),
                ..
            },
        ) => {
            let identifier = WithLocation {
                item: intern!("Boolean"),
                location: location_handler.to_location(&node),
            };
            TypeAnnotation::Named(NamedTypeAnnotation {
                name: string_key_to_identifier(identifier),
            })
        }
        _ => {
            return Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: format!("{:?}", return_type).leak(),
                },
                location,
            )]);
        }
    };

    if !is_optional {
        if use_semantic_non_null {
            // Special case to add self (level 0)
            semantic_non_null_levels.push(0);
        } else {
            // Normal GraphQL non-null (`!``)
            let non_null_annotation = TypeAnnotation::NonNull(Box::new(NonNullTypeAnnotation {
                span: location.span(),
                type_: type_annotation,
                exclamation: generated_token(),
            }));
            return Ok((non_null_annotation, vec![]));
        }
    }

    Ok((type_annotation, semantic_non_null_levels))
}

fn ts_type_to_field_arguments(
    custom_scalar_map: &FnvIndexMap<CustomType, ScalarName>,
    args_type: &TsType,
    module_resolution: &ModuleResolution,
    type_definitions: &FxHashMap<ModuleResolutionKey, DocblockIr>,
    location_handler: &LocationHandler,
) -> DiagnosticsResult<List<InputValueDefinition>> {
    let obj = if let TsType::TsTypeLit(type_) = &args_type {
        // unwrap the ref then the box, then re-add the ref
        type_
    } else {
        return Err(vec![Diagnostic::error(
            SchemaGenerationError::IncorrectArgumentsDefinition,
            location_handler.to_location(args_type),
        )]);
    };
    let mut items = vec![];
    for prop_type in obj.members.iter() {
        let prop_span = location_handler.to_location(prop_type).span();
        if let TsTypeElement::TsPropertySignature(prop) = prop_type {
            let ident = if let Expr::Ident(ident) = prop.key.as_ref() {
                ident
            } else {
                return Err(vec![Diagnostic::error(
                    SchemaGenerationError::IncorrectArgumentsDefinition,
                    location_handler.to_location(&prop.key),
                )]);
            };

            let name_span = location_handler.to_location(ident).span();
            let (type_annotation, _) = return_type_to_type_annotation(
                custom_scalar_map,
                &prop
                    .type_ann
                    .as_ref()
                    .ok_or(vec![Diagnostic::error(
                        SchemaGenerationError::IncorrectArgumentsDefinition,
                        location_handler.to_location(prop),
                    )])?
                    .type_ann
                    .as_ref(),
                module_resolution,
                type_definitions,
                false, // Semantic-non-null doesn't make sense for argument types.
                location_handler,
            )?;
            let arg = InputValueDefinition {
                name: graphql_syntax::Identifier {
                    span: name_span,
                    token: Token {
                        span: name_span,
                        kind: TokenKind::Identifier,
                    },
                    value: ident.sym.as_str().intern(),
                },
                type_: type_annotation,
                default_value: None,
                directives: vec![],
                span: prop_span,
            };
            items.push(arg);
        }
    }

    let list_start: u32 = args_type.span_lo().to_u32();
    let list_end: u32 = args_type.span_hi().to_u32();
    Ok(List {
        items,
        span: location_handler.to_location(args_type).span(),
        start: Token {
            span: Span {
                start: list_start,
                end: list_start + 1,
            },
            kind: TokenKind::OpenBrace,
        },
        end: Token {
            span: Span {
                start: list_end - 1,
                end: list_end,
            },
            kind: TokenKind::CloseBrace,
        },
    })
}
