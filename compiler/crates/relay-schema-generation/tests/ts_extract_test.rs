/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @generated SignedSource<<1d5aa181ac5bc2a2c76a28a08c556b41>>
 */

mod ts_extract;

use ts_extract::transform_fixture;
use fixture_tests::test_fixture;

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
