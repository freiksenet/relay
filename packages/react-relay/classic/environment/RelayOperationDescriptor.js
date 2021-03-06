/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

'use strict';

const QueryBuilder = require('../query/QueryBuilder');

const invariant = require('invariant');

const {getOperationVariables} = require('../query/RelayVariables');
const {ROOT_ID} = require('../store/RelayStoreConstants');

import type {ConcreteOperationDefinition} from '../query/ConcreteQuery';
import type {OperationDescriptor} from './RelayEnvironmentTypes';
import type {Variables} from 'relay-runtime';

/**
 * @public
 *
 * Implementation of `RelayCore#createOperationDescriptor()` defined in
 * `RelayEnvironmentTypes` for the classic core.
 */
function createOperationDescriptor(
  operation: ConcreteOperationDefinition,
  variables: Variables,
): OperationDescriptor {
  const concreteFragment = QueryBuilder.getFragment(operation.node);
  invariant(
    concreteFragment,
    'RelayOperationDescriptor: Expected a query, got %s `%s`.',
    operation.node.kind,
    operation.name,
  );

  const operationVariables = getOperationVariables(operation, variables);
  const fragment = {
    dataID: ROOT_ID,
    node: concreteFragment,
    variables: operationVariables,
  };

  return {
    fragment,
    node: operation,
    root: fragment,
    variables: operationVariables,
  };
}

module.exports = {
  createOperationDescriptor,
};
