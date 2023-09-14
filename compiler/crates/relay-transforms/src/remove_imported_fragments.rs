/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
use graphql_ir::FragmentDefinition;
use graphql_ir::Program;
use graphql_ir::Transformed;
use graphql_ir::Transformer;

pub fn remove_imported_fragments(program: &Program) -> Program {
    let mut transform = RemoveImportedFragmentsTransform::new(program);
    transform
        .transform_program(program)
        .replace_or_else(|| program.clone())
}
struct RemoveImportedFragmentsTransform<'s> {
    #[allow(dead_code)]
    program: &'s Program,
}

impl<'s> RemoveImportedFragmentsTransform<'s> {
    fn new(program: &'s Program) -> Self {
        Self { program }
    }
}

impl<'s> Transformer for RemoveImportedFragmentsTransform<'s> {
    const NAME: &'static str = "RemoveImportedFragmentsTransform";
    const VISIT_ARGUMENTS: bool = false;
    const VISIT_DIRECTIVES: bool = false;

    fn transform_fragment(
        &mut self,
        _fragment: &FragmentDefinition,
    ) -> Transformed<FragmentDefinition> {
        Transformed::Delete
    }
}
