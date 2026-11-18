import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { homedir } from 'node:os'
function run(command,args) { const r=spawnSync(command,args,{stdio:'inherit'}); if(r.error) throw r.error; if(r.status !== 0) process.exit(r.status ?? 1) }
run(process.execPath,['scripts/ensure-toolchain.mjs','--install'])
// rustup may have been installed by the bootstrap child; extend PATH for the actual build.
process.env.PATH = join(process.env.CARGO_HOME || join(homedir(),'.cargo'),'bin') + (process.platform === 'win32' ? ';' : ':') + process.env.PATH
process.env.RUSTUP_TOOLCHAIN = '1.100.0'
run('pnpm',['build:worker'])
run('pnpm',['build:web'])
