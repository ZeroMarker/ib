import { describe, expect, it } from 'vitest'
import { viewFromHash, viewMeta, type View } from '../types'

const VIEWS = Object.keys(viewMeta) as View[]

describe('viewFromHash', () => {
  it('accepts every known view', () => {
    for (const view of VIEWS) {
      window.location.hash = `#${view}`
      expect(viewFromHash()).toBe(view)
    }
  })

  it('falls back to overview for unknown and empty hashes', () => {
    window.location.hash = '#nope'
    expect(viewFromHash()).toBe('overview')
    window.location.hash = ''
    expect(viewFromHash()).toBe('overview')
  })

  it('ignores a hash that only looks like a view', () => {
    window.location.hash = '#overview/extra'
    expect(viewFromHash()).toBe('overview')
  })
})

describe('viewMeta', () => {
  it('describes every view with a label and title', () => {
    for (const view of VIEWS) {
      const meta = viewMeta[view]
      expect(meta.label.length).toBeGreaterThan(0)
      expect(meta.title.length).toBeGreaterThan(0)
      expect(meta.eyebrow.length).toBeGreaterThan(0)
    }
  })

  it('uses unique labels so the nav cannot show duplicates', () => {
    const labels = VIEWS.map((view) => viewMeta[view].label)
    expect(new Set(labels).size).toBe(labels.length)
  })
})
