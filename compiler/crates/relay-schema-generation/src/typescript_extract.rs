use common::Diagnostic;
use common::DiagnosticsResult;
use common::Location;
use common::SourceLocationKey;
use common::Span;
use intern::string_key::Intern;
use intern::Lookup;
use swc_common::source_map::SmallPos;
use swc_common::Spanned;
use swc_ecma_ast::TsType;

use crate::errors::SchemaGenerationError;

pub static LIVE_STATE_TYPE_NAME: &str = "LiveState";

pub fn extract_entity_type_from_resolver_function(
    node: &swc_ecma_ast::FnDecl,
    current_location: &SourceLocationKey,
) -> DiagnosticsResult<Option<TsType>> {
    if node.function.params.is_empty() {
        Ok(None)
    } else {
        let param = &node.function.params[0].pat;

        if let swc_ecma_ast::Pat::Ident(ident) = param {
            let type_annotation = ident
                .type_ann
                .as_ref()
                .ok_or_else(|| {
                    Diagnostic::error(
                        SchemaGenerationError::MissingParamType,
                        Location::new(current_location.clone(), to_relay_span(ident.span())),
                    )
                })?
                .clone();

            Ok(Some(*type_annotation.type_ann))
        } else {
            let printed_param = swc_ecma_codegen::to_code(param);

            return Err(vec![Diagnostic::error(
                SchemaGenerationError::UnsupportedType {
                    name: &printed_param.intern().lookup(),
                },
                Location::new(current_location.clone(), to_relay_span(node.span())),
            )]);
        }
    }
}

pub fn extract_params_from_second_argument(
    node: &swc_ecma_ast::FnDecl,
    current_location: &SourceLocationKey,
) -> DiagnosticsResult<Option<TsType>> {
    let params = &node.function.params;
    let arguments = if params.len() > 1 {
        let parent_param = &params[0];
        let arg_param = &params[1];
        if let swc_ecma_ast::Pat::Ident(ident) = &arg_param.pat {
            let type_annotation = ident.type_ann.as_ref().ok_or_else(|| {
                Diagnostic::error(
                    SchemaGenerationError::MissingParamType,
                    Location::new(current_location.clone(), to_relay_span(parent_param.span())),
                )
            })?;

            Ok(Some(type_annotation.type_ann.as_ref().clone()))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    };

    arguments
}

pub fn extract_return_type_from_resolver_function(
    node: &swc_ecma_ast::FnDecl,
    current_location: &SourceLocationKey,
) -> DiagnosticsResult<(TsType, Option<Location>)> {
    // Return type is the return type annotation of the function
    let return_type_annotation = node
        .function
        .return_type
        .as_ref()
        .ok_or_else(|| {
            Diagnostic::error(
                SchemaGenerationError::MissingReturnType,
                Location::new(current_location.clone(), to_relay_span(node.span())),
            )
        })?
        .type_ann
        .as_ref()
        .clone();

    // If the return type is the LiveState<T> type we don't care about LiveState but just want to take T
    let (return_type, is_live) = match &return_type_annotation {
        TsType::TsTypeRef(ts_type_ref) => {
            let is_live_state = ts_type_ref
                .type_name
                .as_ident()
                .map(|ident| ident.sym.as_str())
                .is_some_and(|ident| ident == LIVE_STATE_TYPE_NAME);

            if ts_type_ref.type_name.is_ts_qualified_name() {
                return Err(vec![Diagnostic::error(
                    SchemaGenerationError::UnsupportedType {
                        name: "Qualified names",
                    },
                    Location::new(current_location.clone(), to_relay_span(ts_type_ref.span)),
                )]);
            }

            if ts_type_ref
                .type_params
                .as_ref()
                .is_some_and(|type_params| type_params.params.len() > 1)
            {
                return Err(vec![Diagnostic::error(
                    SchemaGenerationError::UnsupportedType {
                        name: "Multiple type params",
                    },
                    Location::new(current_location.clone(), to_relay_span(ts_type_ref.span)),
                )]);
            }

            if is_live_state {
                let type_params = ts_type_ref.type_params.as_ref().ok_or_else(|| {
                    Diagnostic::error(
                        SchemaGenerationError::LiveStateExpectedSingleGeneric,
                        Location::new(current_location.clone(), to_relay_span(ts_type_ref.span)),
                    )
                })?;

                let type_param: &Box<TsType> = type_params.params.first().ok_or_else(|| {
                    Diagnostic::error(
                        SchemaGenerationError::LiveStateExpectedSingleGeneric,
                        Location::new(current_location.clone(), to_relay_span(type_params.span)),
                    )
                })?;

                (
                    type_param.as_ref().clone(),
                    Some(Location::new(
                        current_location.clone(),
                        to_relay_span(node.span()),
                    )),
                )
            } else {
                (return_type_annotation, None)
            }
        }
        _ => (return_type_annotation, None),
    };

    Ok((return_type, is_live))
}

// Duplicated from typescript.rs
fn to_relay_span(span: swc_common::Span) -> Span {
    Span::new(span.lo().to_u32(), span.hi().to_u32())
}
