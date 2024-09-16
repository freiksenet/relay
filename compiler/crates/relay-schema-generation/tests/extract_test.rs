/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @generated SignedSource<<7f528d4cf814d61c3cc43e92432aa963>>
 */

mod extract;

use extract::transform_fixture;
use fixture_tests::test_fixture;

#[tokio::test]
async fn arguments() {
    let input = include_str!("extract/fixtures/arguments.js");
    let expected = include_str!("extract/fixtures/arguments.expected");
    test_fixture(transform_fixture, file!(), "arguments.js", "extract/fixtures/arguments.expected", input, expected).await;
}

#[tokio::test]
async fn functions_unsupported() {
    let input = include_str!("extract/fixtures/functions.unsupported.js");
    let expected = include_str!("extract/fixtures/functions.unsupported.expected");
    test_fixture(transform_fixture, file!(), "functions.unsupported.js", "extract/fixtures/functions.unsupported.expected", input, expected).await;
}

#[tokio::test]
async fn generics() {
    let input = include_str!("extract/fixtures/generics.js");
    let expected = include_str!("extract/fixtures/generics.expected");
    test_fixture(transform_fixture, file!(), "generics.js", "extract/fixtures/generics.expected", input, expected).await;
}

#[tokio::test]
async fn plural_optional() {
    let input = include_str!("extract/fixtures/plural-optional.js");
    let expected = include_str!("extract/fixtures/plural-optional.expected");
    test_fixture(transform_fixture, file!(), "plural-optional.js", "extract/fixtures/plural-optional.expected", input, expected).await;
}

#[tokio::test]
async fn primitives() {
    let input = include_str!("extract/fixtures/primitives.js");
    let expected = include_str!("extract/fixtures/primitives.expected");
    test_fixture(transform_fixture, file!(), "primitives.js", "extract/fixtures/primitives.expected", input, expected).await;
}

#[tokio::test]
async fn ts_arguments() {
    let input = include_str!("extract/fixtures/ts-arguments.ts");
    let expected = include_str!("extract/fixtures/ts-arguments.expected");
    test_fixture(transform_fixture, file!(), "ts-arguments.ts", "extract/fixtures/ts-arguments.expected", input, expected).await;
}

#[tokio::test]
async fn ts_functions_unsupported() {
    let input = include_str!("extract/fixtures/ts-functions.unsupported.ts");
    let expected = include_str!("extract/fixtures/ts-functions.unsupported.expected");
    test_fixture(transform_fixture, file!(), "ts-functions.unsupported.ts", "extract/fixtures/ts-functions.unsupported.expected", input, expected).await;
}

#[tokio::test]
async fn ts_generics() {
    let input = include_str!("extract/fixtures/ts-generics.ts");
    let expected = include_str!("extract/fixtures/ts-generics.expected");
    test_fixture(transform_fixture, file!(), "ts-generics.ts", "extract/fixtures/ts-generics.expected", input, expected).await;
}

#[tokio::test]
async fn ts_plural_optional() {
    let input = include_str!("extract/fixtures/ts-plural-optional.ts");
    let expected = include_str!("extract/fixtures/ts-plural-optional.expected");
    test_fixture(transform_fixture, file!(), "ts-plural-optional.ts", "extract/fixtures/ts-plural-optional.expected", input, expected).await;
}

#[tokio::test]
async fn ts_primitives() {
    let input = include_str!("extract/fixtures/ts-primitives.ts");
    let expected = include_str!("extract/fixtures/ts-primitives.expected");
    test_fixture(transform_fixture, file!(), "ts-primitives.ts", "extract/fixtures/ts-primitives.expected", input, expected).await;
}
