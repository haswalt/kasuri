import { mkdirSync, existsSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
mkdirSync('secrets', { recursive:true, mode:0o700 })
if (existsSync('secrets/basic_auth')) console.log('Existing self-host credentials retained.')
else { writeFileSync('secrets/basic_auth', `admin:${randomBytes(24).toString('base64url')}\n`, { mode:0o644, flag:'wx' }); console.log('Created secrets/basic_auth (user:password). Read this file locally to sign in; do not commit it.') }
console.log('Run docker compose up --build -d, then open http://localhost:8080.')
