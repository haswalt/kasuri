// Builds workerd-queues.capnp, the self-host config for RUN_EXECUTION=queues:
// the default config plus a RUN_QUEUE binding and workerd's built-in Queues
// Worker. That Worker is experimental in workerd, so it is kept out of the
// default config and the launchers add --experimental only for this one.
import { readFileSync, writeFileSync } from 'node:fs'

export function queuesConfig(base) {
  const binding = '        (name = "RUNS", durableObjectNamespace = "WorkflowRun"),\n'
  const service = '    (name = "assets", disk = (path = "public")),\n'
  if (!base.includes(binding) || !base.includes(service)) {
    throw new Error('workerd.capnp no longer has the anchors the queues config extends')
  }
  return base
    .replace(binding, binding + '        (name = "RUN_QUEUE", queue = (name = "queues", props = (json = "{\\"queue\\": \\"kasuri-runs\\"}"))),\n')
    .replace(service, `    # Run messages: workerd's built-in Queues Worker, storing messages in
    # Durable Objects on the same volume.
    (name = "queues", worker = (
      modules = [(name = "queues.js", bundled = "/workerd/queues.js")],
      compatibilityDate = "2026-11-18",
      compatibilityFlags = ["service_binding_extra_handlers"],
      bindings = [
        (name = "SHARD", durableObjectNamespace = "QueueShard"),
        (name = "CONSUMERS", json = "{\\"kasuri-runs\\": {\\"consumer\\": \\"KASURI\\", \\"maxBatchSize\\": 10, \\"maxBatchTimeout\\": 1, \\"maxRetries\\": 10}}"),
        (name = "KASURI", service = "main")
      ],
      durableObjectNamespaces = [(className = "QueueShard", uniqueKey = "queues", enableSql = true)],
      durableObjectStorage = (localDisk = "storage")
    )),
` + service)
}

if (process.argv[1] === new URL(import.meta.url).pathname) {
  const [input, output] = process.argv.slice(2)
  writeFileSync(output, queuesConfig(readFileSync(input, 'utf8')))
}
