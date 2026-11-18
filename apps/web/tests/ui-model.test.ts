import test from 'node:test'
import assert from 'node:assert/strict'
import { category,csvCell,runState,tone } from '../src/design/ui-model.ts'
import type { Run } from '../../../packages/contracts/src/index.ts'
test('run status distinguishes a durable wait from a paused run',()=>{
 const run={status:'running',nodes:[{status:'waiting'}]} as Run
 assert.equal(runState(run),'waiting')
 assert.equal(runState({...run,status:'paused'}),'paused')
 assert.equal(runState({...run,status:'cancelled'}),'cancelled')
})
test('node statuses preserve skipped versus cancelled semantics',()=>{
 assert.equal(tone('skipped').label,'Skipped')
 assert.equal(tone('pending').label,'Pending')
 assert.equal(tone('succeeded').label,'Succeeded')
 assert.equal(tone('paused').fg,'var(--warn)')
})
test('native model and portal connectors use the AI category',()=>{
 assert.equal(category('Cloudflare AI Gateway'),'http')
 assert.equal(category('Cloudflare MCP Portals'),'http')
 assert.equal(category('Cloudflare D1'),'cf')
 assert.equal(category('Webhook'),'trigger')
})
test('CSV export quotes embedded cells and neutralizes formulas',()=>{
 assert.equal(csvCell('A, "quoted" workflow'),'"A, ""quoted"" workflow"')
 assert.equal(csvCell('=HYPERLINK("https://example.com")'),'"\'=HYPERLINK(""https://example.com"")"')
 assert.equal(csvCell('  +SUM(1,2)'),'"\'  +SUM(1,2)"')
})
