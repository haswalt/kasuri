import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { parseWorkflow } from '../src/lib/workflows.ts'
const examples = new URL('../../../examples/', import.meta.url)
const cases = JSON.parse(readFileSync(new URL('scenarios.json', examples), 'utf8'))
for (const scenario of cases) {
  test(`published example imports through the editor: ${scenario.file}`, () => {
    const source = JSON.parse(readFileSync(new URL(scenario.file, examples), 'utf8'))
    const imported = parseWorkflow(JSON.stringify(source))
    assert.equal(imported.status, 'draft')
    assert.equal(imported.revision, 0)
    assert.notEqual(imported.id, source.id)
    assert.deepEqual(imported.nodes, source.nodes)
    assert.deepEqual(imported.edges, source.edges)
  })
}
