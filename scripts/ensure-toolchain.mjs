import { spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { homedir, tmpdir } from 'node:os'
const install = process.argv.includes('--install')
const rustVersion = '1.100.0'
const workerBuildVersion = '0.9.0'
process.env.PATH = join(process.env.CARGO_HOME || join(homedir(), '.cargo'), 'bin') + (process.platform === 'win32' ? ';' : ':') + process.env.PATH
function inspect(command, args) { const r = spawnSync(command,args,{encoding:'utf8'}); return r.status === 0 ? r.stdout.trim() : '' }
function run(command,args) { const r = spawnSync(command,args,{stdio:'inherit'}); if(r.error) throw r.error; if(r.status !== 0) process.exit(r.status ?? 1) }
if(!inspect('rustup',['--version'])) {
  if(!install || process.platform === 'win32') throw new Error('Install Rust from https://rustup.rs, then run this command again.')
  const dir = mkdtempSync(join(tmpdir(),'kasuri-rustup-'))
  try { const script=join(dir,'rustup.sh'); run('curl',['--proto','=https','--tlsv1.2','--fail','--silent','--show-error','--location','https://sh.rustup.rs','--output',script]); run('sh',[script,'-y','--profile','minimal','--default-toolchain',rustVersion]) }
  finally { rmSync(dir,{recursive:true,force:true}) }
}
if(!inspect('rustc',['--version']).startsWith(`rustc ${rustVersion} `)) {
  if(!install) throw new Error(`Rust ${rustVersion} is required. Run with --install.`)
  run('rustup',['toolchain','install',rustVersion,'--profile','minimal'])
}
// This variable is scoped to child build processes, leaving the user's default toolchain alone.
process.env.RUSTUP_TOOLCHAIN = rustVersion
if(!inspect('rustup',['target','list','--installed','--toolchain',rustVersion]).split(/\s+/).includes('wasm32-unknown-unknown')) {
  if(!install) throw new Error('Install the wasm32-unknown-unknown Rust target.')
  run('rustup',['target','add','wasm32-unknown-unknown','--toolchain',rustVersion])
}
if(!inspect('cargo',['install','--list']).includes(`worker-build v${workerBuildVersion}:`)) {
  if(!install) throw new Error(`Install worker-build ${workerBuildVersion}.`)
  run('cargo',['install','worker-build','--version',workerBuildVersion,'--locked'])
}
console.log(`Rust ${rustVersion}, Wasm target and worker-build ${workerBuildVersion} are ready.`)
