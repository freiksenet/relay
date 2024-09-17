/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */


use common::Diagnostic;
use common::SourceLocationKey;
use common::TextSource;
use fixture_tests::Fixture;
use graphql_cli::DiagnosticPrinter;
use relay_schema_generation::TSTypeExtractor;
use swc_common::comments::Comments;
use swc_common::comments::SingleThreadedComments;
use swc_common::FileName;
use swc_common::SourceMap;
use swc_common::Spanned;
use swc_ecma_ast::Decl;
use swc_ecma_ast::ModuleItem;
use swc_ecma_ast::Stmt;
use swc_ecma_parser::error::Error;
use swc_ecma_parser::parse_file_as_module;
use swc_ecma_parser::TsSyntax;
use swc_common::sync::Lrc;

pub async fn transform_fixture(fixture: &Fixture<'_>) -> Result<String, String> {
    let extractor = TSTypeExtractor::new();

    let ts_config = TsSyntax {
        tsx: true,
        decorators: true,
        dts: false,
        no_early_errors: false,
        disallow_ambiguous_jsx_like: true,
    };

    let mut comments = SingleThreadedComments::default();

    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom(fixture.file_name.into()).into(),
        fixture.content.to_string(),
    );

    let mut errors: Vec<Error> = Vec::new();

    let result = parse_file_as_module(
        &fm,
        swc_ecma_parser::Syntax::Typescript(ts_config),
        swc_ecma_ast::EsVersion::EsNext,
        Some(&mut comments),
        &mut errors
    )
    .unwrap();

    let nodes_with_attached_comments = find_nodes_after_comments(&result, &comments);

    let output = nodes_with_attached_comments
    .into_iter()
    .filter_map(|item| {
        let (comment, node) = item;
        println!("comment: {:?}", comment);
        match comment.as_str().trim() {
            "extract" => match node {
                ModuleItem::Stmt(Stmt::Decl(Decl::Fn(function))) => {
                    Some(extractor.extract_function(&function))
                }
                _ => None,
            },
            _ => None,
        }
    })
    .map(|result| match result {
        Ok(data) => {
            format!("{:#?}", data)
        }
        Err(diag) => diagnostics_to_sorted_string(fixture.content, &diag),
    })
    .collect::<Vec<_>>()
    .join("\n\n");

    Ok(output)
}

fn diagnostics_to_sorted_string(source: &str, diagnostics: &[Diagnostic]) -> String {
    let printer = DiagnosticPrinter::new(|source_location| match source_location {
        SourceLocationKey::Embedded { .. } => unreachable!(),
        SourceLocationKey::Standalone { .. } => unreachable!(),
        SourceLocationKey::Generated => Some(TextSource::from_whole_document(source)),
    });
    let mut printed = diagnostics
        .iter()
        .map(|diagnostic| printer.diagnostic_to_string(diagnostic))
        .collect::<Vec<_>>();
    printed.sort();
    printed.join("\n\n")
}

fn find_nodes_after_comments(
    ast: &swc_ecma_ast::Module,
    comments: &SingleThreadedComments,
) -> Vec<(String, ModuleItem)> {
    ast.body.iter().filter(|node| comments.has_leading(node.span().lo()))
    .map(|node| {
        let comment = comments.get_leading(node.span().lo()).unwrap()
        .iter().last()
        .map(|comment| comment.text.to_string())
        .expect("Expected comment");
        
        (comment, node.clone())
    }).collect()
}