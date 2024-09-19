/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @generated SignedSource<<23bcd4cd1ce8b7444e528d8071a1227b>>
 */

mod ts_docblock;

use ts_docblock::transform_fixture;
use fixture_tests::test_fixture;

#[tokio::test]
async fn return_strong_object_directly_error() {
    let input = include_str!("ts_docblock/fixtures/return-strong-object-directly.error.ts");
    let expected = include_str!("ts_docblock/fixtures/return-strong-object-directly.error.expected");
    test_fixture(transform_fixture, file!(), "return-strong-object-directly.error.ts", "ts_docblock/fixtures/return-strong-object-directly.error.expected", input, expected).await;
}

#[tokio::test]
async fn root_fragment_arguments() {
    let input = include_str!("ts_docblock/fixtures/root-fragment-arguments.ts");
    let expected = include_str!("ts_docblock/fixtures/root-fragment-arguments.expected");
    test_fixture(transform_fixture, file!(), "root-fragment-arguments.ts", "ts_docblock/fixtures/root-fragment-arguments.expected", input, expected).await;
}

#[tokio::test]
async fn root_fragment_arguments_error() {
    let input = include_str!("ts_docblock/fixtures/root-fragment-arguments.error.ts");
    let expected = include_str!("ts_docblock/fixtures/root-fragment-arguments.error.expected");
    test_fixture(transform_fixture, file!(), "root-fragment-arguments.error.ts", "ts_docblock/fixtures/root-fragment-arguments.error.expected", input, expected).await;
}

#[tokio::test]
async fn ts_arguments() {
    let input = include_str!("ts_docblock/fixtures/ts-arguments.input");
    let expected = include_str!("ts_docblock/fixtures/ts-arguments.expected");
    test_fixture(transform_fixture, file!(), "ts-arguments.input", "ts_docblock/fixtures/ts-arguments.expected", input, expected).await;
}

#[tokio::test]
async fn ts_primitive_types() {
    let input = include_str!("ts_docblock/fixtures/ts-primitive-types.input");
    let expected = include_str!("ts_docblock/fixtures/ts-primitive-types.expected");
    test_fixture(transform_fixture, file!(), "ts-primitive-types.input", "ts_docblock/fixtures/ts-primitive-types.expected", input, expected).await;
}

#[tokio::test]
async fn ts_root_fragment() {
    let input = include_str!("ts_docblock/fixtures/ts-root-fragment.input");
    let expected = include_str!("ts_docblock/fixtures/ts-root-fragment.expected");
    test_fixture(transform_fixture, file!(), "ts-root-fragment.input", "ts_docblock/fixtures/ts-root-fragment.expected", input, expected).await;
}

#[tokio::test]
async fn ts_single_module() {
    let input = include_str!("ts_docblock/fixtures/ts-single-module.input");
    let expected = include_str!("ts_docblock/fixtures/ts-single-module.expected");
    test_fixture(transform_fixture, file!(), "ts-single-module.input", "ts_docblock/fixtures/ts-single-module.expected", input, expected).await;
}

#[tokio::test]
async fn ts_strong_type_define_flow_within() {
    let input = include_str!("ts_docblock/fixtures/ts-strong-type-define-flow-within.input");
    let expected = include_str!("ts_docblock/fixtures/ts-strong-type-define-flow-within.expected");
    test_fixture(transform_fixture, file!(), "ts-strong-type-define-flow-within.input", "ts_docblock/fixtures/ts-strong-type-define-flow-within.expected", input, expected).await;
}

#[tokio::test]
async fn ts_weak_object() {
    let input = include_str!("ts_docblock/fixtures/ts-weak-object.input");
    let expected = include_str!("ts_docblock/fixtures/ts-weak-object.expected");
    test_fixture(transform_fixture, file!(), "ts-weak-object.input", "ts_docblock/fixtures/ts-weak-object.expected", input, expected).await;
}
