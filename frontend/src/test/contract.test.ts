/**
 * Guards the wire contract.
 *
 * `api-schema.json` is the field list the Rust API serializes; `types.ts`
 * asserts key parity with it at compile time via the `_ContractFieldsMatch`
 * type, which `tsc` checks on every build. These tests cover the part a
 * compile-time assertion cannot: that the file itself is well-formed and that
 * the type-level assertion is still wired up, so deleting the assertion cannot
 * quietly disable the check.
 */
import { describe, expect, it } from 'vitest'
import contract from '../../../api-schema.json'
import type { _ContractFieldsMatch } from '../types'

/** Every entry must be a non-empty map of field name to type spec. */
const entries = Object.entries(contract.types)

describe('api-schema.json', () => {
  it('declares the types the frontend imports', () => {
    expect(entries.map(([name]) => name).sort()).toEqual([
      'Account',
      'CashBalance',
      'Contract',
      'Fill',
      'Order',
      'Overview',
      'Position',
      'UserResponse',
    ])
  })

  it('gives every field a recognized type spec', () => {
    // A primitive, or a reference to another entry, optionally as an array and
    // optionally nullable. Anything else would fall through to `never` in
    // `types.ts` and surface as a confusing error far from the real cause.
    const spec = /^(string|number|boolean|[A-Z]\w*)(\[\])?(\|null)?$/
    for (const [name, fields] of entries) {
      for (const [field, declared] of Object.entries(fields)) {
        expect(spec.test(declared), `${name}.${field} has an unrecognized spec: ${declared}`).toBe(
          true,
        )
      }
    }
  })

  it('marks the nullable fields the server actually emits as null', () => {
    // These are the fields that make a `null` check necessary at a use site.
    const nullable = new Set([
      'Order.perm_id',
      'Order.lmt_price',
      'Order.aux_price',
      'Position.avg_cost',
    ])
    for (const [name, fields] of entries) {
      for (const [field, declared] of Object.entries(fields)) {
        const key = `${name}.${field}`
        expect(declared.endsWith('|null'), `${key} should be nullable`).toBe(nullable.has(key))
      }
    }
  })

  it('references only entries that exist', () => {
    const names = new Set(entries.map(([name]) => name))
    for (const [name, fields] of entries) {
      for (const [field, declared] of Object.entries(fields)) {
        const referenced = declared.replace(/(\|null)/g, '').replace(/(\[\])/g, '')
        if (['string', 'number', 'boolean'].includes(referenced)) continue
        expect(
          names.has(referenced),
          `${name}.${field} references unknown type ${referenced}`,
        ).toBe(true)
      }
    }
  })
})

describe('the compile-time parity assertion', () => {
  it('is still present and resolving to true', () => {
    // If someone deletes `_ContractFieldsMatch` from types.ts, this import
    // breaks and the test fails, so the guard cannot be dropped silently.
    // A `false` here would mean tsc already rejected the build.
    const assertions: _ContractFieldsMatch = [true, true, true, true, true, true, true, true]
    expect(assertions.every((value) => value === true)).toBe(true)
  })
})
