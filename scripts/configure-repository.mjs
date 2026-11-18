import { readFileSync, writeFileSync } from 'node:fs'
const input=process.argv[2]
if(!input) throw new Error('Usage: pnpm setup:repository https://github.com/OWNER/REPO')
const url=new URL(input.replace(/\.git$/,''))
if(url.protocol !== 'https:' || !['github.com','gitlab.com'].includes(url.hostname) || url.username || url.password || url.search || url.hash || !/^\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\/?$/.test(url.pathname)) throw new Error('Use a public HTTPS GitHub/GitLab repository URL.')
const repository=url.origin+url.pathname.replace(/\/$/,'')
const badge=`[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=${encodeURIComponent(repository)})`
const file='README.md'; let readme=readFileSync(file,'utf8')
const block=`<!-- deploy-button:start -->\n${badge}\n<!-- deploy-button:end -->`
if(readme.includes('<!-- deploy-button:start -->')) readme=readme.replace(/<!-- deploy-button:start -->[\s\S]*?<!-- deploy-button:end -->/,block)
else readme=readme.replace(/^# Kasuri\n/,'# Kasuri\n\n'+block+'\n')
writeFileSync(file,readme)
console.log(`Configured the deploy button for ${repository}. The repository must be public before it can be used.`)
