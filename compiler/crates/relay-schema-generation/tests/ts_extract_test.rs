/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @generated SignedSource<<4e3d61bbc0772bdc5606c006d7bf9234>>
 */

mod ts_extract;

use ts_extract::transform_fixture;
use fixture_tests::test_fixture;

#[tokio::test]
async fn mising_return_type() {
    let input = include_str!("ts_extract/fixtures/mising_return_type.ts");
    let expected = include_str!("ts_extract/fixtures/mising_return_type.expected");
    test_fixture(transform_fixture, file!(), "mising_return_type.ts", "ts_extract/fixtures/mising_return_type.expected", input, expected).await;
}

#[tokio::test]
async fn mising_return_type_fine_for_non_resolver_functions() {
    let input = include_str!("ts_extract/fixtures/mising_return_type_fine_for_non_resolver_functions.ts");
    let expected = include_str!("ts_extract/fixtures/mising_return_type_fine_for_non_resolver_functions.expected");
    test_fixture(transform_fixture, file!(), "mising_return_type_fine_for_non_resolver_functions.ts", "ts_extract/fixtures/mising_return_type_fine_for_non_resolver_functions.expected", input, expected).await;
}

#[tokio::test]
async fn ts_arguments() {
    let input = include_str!("ts_extract/fixtures/ts-arguments.ts");
    let expected = include_str!("ts_extract/fixtures/ts-arguments.expected");
    test_fixture(transform_fixture, file!(), "ts-arguments.ts", "ts_extract/fixtures/ts-arguments.expected", input, expected).await;
}

#[tokio::test]
async fn ts_functions_unsupported() {
    let input = include_str!("ts_extract/fixtures/ts-functions.unsupported.ts");
    let expected = include_str!("ts_extract/fixtures/ts-functions.unsupported.expected");
    test_fixture(transform_fixture, file!(), "ts-functions.unsupported.ts", "ts_extract/fixtures/ts-functions.unsupported.expected", input, expected).await;
}

#[tokio::test]
async fn ts_generics() {
    let input = include_str!("ts_extract/fixtures/ts-generics.ts");
    let expected = include_str!("ts_extract/fixtures/ts-generics.expected");
    test_fixture(transform_fixture, file!(), "ts-generics.ts", "ts_extract/fixtures/ts-generics.expected", input, expected).await;
}

#[tokio::test]
async fn ts_plural_optional() {
    let input = include_str!("ts_extract/fixtures/ts-plural-optional.ts");
    let expected = include_str!("ts_extract/fixtures/ts-plural-optional.expected");
    test_fixture(transform_fixture, file!(), "ts-plural-optional.ts", "ts_extract/fixtures/ts-plural-optional.expected", input, expected).await;
}

#[tokio::test]
async fn ts_primitives() {
    let input = include_str!("ts_extract/fixtures/ts-primitives.ts");
    let expected = include_str!("ts_extract/fixtures/ts-primitives.expected");
    test_fixture(transform_fixture, file!(), "ts-primitives.ts", "ts_extract/fixtures/ts-primitives.expected", input, expected).await;
}

#[tokio::test]
async fn ts_primitives_optional() {
    let input = include_str!("ts_extract/fixtures/ts-primitives-optional.ts");
    let expected = include_str!("ts_extract/fixtures/ts-primitives-optional.expected");
    test_fixture(transform_fixture, file!(), "ts-primitives-optional.ts", "ts_extract/fixtures/ts-primitives-optional.expected", input, expected).await;
}
