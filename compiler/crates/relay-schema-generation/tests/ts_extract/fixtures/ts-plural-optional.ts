/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */


//extract
function plural_string(user: User): Array<string> {}

//extract
function plural_optional_string(user: User): Array<string | null | undefined> {}

//extract
function optional_plural_string(user: User): Array<string> | null | undefined {}

//extract
function optional_plural_optional_string(user: User): Array<string | null | undefined> | null | undefined {}

function ignored(user: User): Array<string | null | undefined> | null | undefined {}

// Multiple top comments
// this another
function ignored(user: User): Array<string | null | undefined> | null | undefined {}