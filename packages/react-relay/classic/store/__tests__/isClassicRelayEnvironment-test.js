/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @emails oncall+relay
 * @format
 */

'use strict';

const RelayEnvironment = require('../RelayEnvironment');

const isClassicRelayEnvironment = require('../isClassicRelayEnvironment');

describe('isClassicRelayEnvironment()', () => {
  it('returns true for `RelayEnvironment` instances', () => {
    expect(isClassicRelayEnvironment(new RelayEnvironment())).toBe(true);
  });

  it('returns true for objects that conform to the interface', () => {
    const environment = {
      applyMutation: () => null,
      forceFetch: () => null,
      getFragmentResolver: () => null,
      getStoreData: () => null,
      primeCache: () => null,
      sendMutation: () => null,
    };
    expect(isClassicRelayEnvironment(environment)).toBe(true);
  });

  it('returns false for objects that do not conform to the interface', () => {
    const fakeEnvironment = {
      forceFetch: true,
      getFragmentResolver: true,
      getStoreData: true,
      primeCache: true,
    };
    expect(isClassicRelayEnvironment(fakeEnvironment)).toBe(false);
  });

  it('returns false for non-objects', () => {
    expect(isClassicRelayEnvironment(null)).toBe(false);
    expect(isClassicRelayEnvironment(false)).toBe(false);
    expect(isClassicRelayEnvironment('relay')).toBe(false);
    expect(isClassicRelayEnvironment(1)).toBe(false);
  });
});
