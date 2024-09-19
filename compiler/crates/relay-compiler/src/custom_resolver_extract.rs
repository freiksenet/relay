use std::path::Path;

use common::Diagnostic;
use common::ScalarName;
use common::SourceLocationKey;
use extract_graphql::JavaScriptSourceFeature;
use graphql_syntax::ExecutableDefinition;
use indexmap::IndexMap;
use intern::string_key::Intern;
use relay_config::CustomType;
use relay_config::ProjectName;
use relay_docblock::DocblockIr;
use relay_docblock::ResolverFieldDocblockIr;
use relay_schema_generation::FlowRelayResolverExtractor;
use relay_schema_generation::RelayResolverExtractor;

use crate::compiler_state::CompilerState;
use crate::GraphQLAsts;

// TODO remove this
#[allow(unused_variables)]
pub fn custom_extract_resolver(
    project_config_name: ProjectName,
    custom_scalar_types: &IndexMap<
        ScalarName,
        CustomType,
        std::hash::BuildHasherDefault<fnv::FnvHasher>,
    >,
    compiler_state: &CompilerState,
    graphql_asts: Option<&GraphQLAsts>,
) -> Result<(Vec<DocblockIr>, Vec<DocblockIr>), Vec<Diagnostic>> {
    println!("!!!!custom_extract_relay_resolvers!!!!");
    let mut errors: Vec<Diagnostic> = vec![];
    let mut extractor = FlowRelayResolverExtractor::new();

    if let Err(err) = extractor.set_custom_scalar_map(&custom_scalar_types) {
        errors.extend(err);
    }

    if errors.len() > 0 {
        return Err(errors);
    }

    let files_to_process = &compiler_state
        .full_sources
        .get(&project_config_name)
        .unwrap()
        .pending;

    for (source_location_key, content) in files_to_process {
        let gql_operations = parse_document_definitions(content, source_location_key);
        if let Err(err) = extractor.parse_document(
            content,
            source_location_key.to_string_lossy().as_ref(),
            Some(&gql_operations),
        ) {
            errors.extend(err);
        }
    }

    match extractor.resolve() {
        Ok((objects, fields)) => {
            println!("After resolve extracted types: {:?}", objects.len());
            println!("After resolve extracted fields: {:?}", fields.len());
            let fields = fields
                .into_iter()
                .map(|field| DocblockIr::Field(ResolverFieldDocblockIr::TerseRelayResolver(field)))
                .collect();

            Ok((objects, fields))
        }
        Err(err) => {
            errors.extend(err);
            Err(errors)
        }
    }
}

fn parse_document_definitions(content: &str, path: &Path) -> Vec<ExecutableDefinition> {
    let features = extract_graphql::extract(content);
    features
        .into_iter()
        .enumerate()
        .filter_map(|(i, feature)| {
            if let JavaScriptSourceFeature::GraphQL(graphql_source) = feature {
                Some(
                    graphql_syntax::parse_executable(
                        &graphql_source.to_text_source().text,
                        SourceLocationKey::Embedded {
                            path: path.to_str().unwrap().intern(),
                            index: i as u16,
                        },
                    )
                    .unwrap()
                    .definitions,
                )
            } else {
                None
            }
        })
        .flatten()
        .collect()
}
